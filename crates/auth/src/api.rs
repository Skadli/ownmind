//! 公共入口：其他模块只能用这里导出的东西。

use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ownmind_kernel::MasterKey;
use sqlx::SqlitePool;

use crate::internal::{password, token, totp};

/// 本模块的数据库迁移，由 server 合并执行。
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// 登录会话有效期：30 天，每次使用后从使用时刻重新计算。
pub const SESSION_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// 主人密码最短长度（字符）。
pub const MIN_PASSWORD_CHARS: usize = 8;

/// 连续失败这么多次后开始锁定。
pub const MAX_FAILED_LOGINS: i64 = 5;
/// 锁定时长上限。
pub const MAX_LOCK_MS: i64 = 60 * 60 * 1000;

/// 返回当前 UTC 毫秒的时钟；测试时可替换。
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("主人已设置")]
    AlreadySetUp,
    #[error("主人尚未设置")]
    NotSetUp,
    #[error("密码至少 {MIN_PASSWORD_CHARS} 个字符")]
    WeakPassword,
    #[error("密码错误")]
    InvalidPassword,
    #[error("未登录或登录已过期")]
    Unauthenticated,
    #[error("尝试次数过多，{retry_after_secs} 秒后再试")]
    Locked { retry_after_secs: i64 },
    #[error("需要两步验证码")]
    TotpRequired,
    #[error("验证码错误或已使用")]
    InvalidTotp,
    #[error("没有进行中的两步验证设置")]
    TotpNotPending,
    #[error(transparent)]
    Secret(#[from] ownmind_kernel::SecretError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// 主人身份与登录会话。
#[derive(Clone)]
pub struct Auth {
    db: SqlitePool,
    key: MasterKey,
    clock: Clock,
}

/// 开启两步验证时给主人看的内容：密钥（手动输入用）与 otpauth 链接（生成二维码）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TotpSetup {
    pub secret: String,
    pub uri: String,
}

/// 按 base32 密钥算出某时刻的验证码（测试与排错用）。
#[doc(hidden)]
pub fn totp_code_at(secret_base32: &str, now_ms: i64) -> Option<String> {
    let secret = totp::from_base32(secret_base32)?;
    Some(totp::code(&secret, now_ms.div_euclid(totp::STEP_MS)))
}

impl Auth {
    pub fn new(db: SqlitePool, key: MasterKey) -> Self {
        Self::with_clock(db, key, Arc::new(ownmind_kernel::now_ms))
    }

    pub fn with_clock(db: SqlitePool, key: MasterKey, clock: Clock) -> Self {
        Self { db, key, clock }
    }

    pub async fn is_set_up(&self) -> Result<bool, AuthError> {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auth_owner")
            .fetch_one(&self.db)
            .await?;
        Ok(n > 0)
    }

    /// 首次设置主人密码，并直接登录，返回登录令牌。
    pub async fn set_up(
        &self,
        password: &str,
        user_agent: Option<&str>,
    ) -> Result<String, AuthError> {
        if password.chars().count() < MIN_PASSWORD_CHARS {
            return Err(AuthError::WeakPassword);
        }
        let hash = password::hash(password);
        let now = (self.clock)();
        let inserted = sqlx::query(
            "INSERT INTO auth_owner (id, password_hash, created_at, updated_at) VALUES (?, ?, ?, ?) \
             ON CONFLICT (is_singleton) DO NOTHING",
        )
        .bind(ownmind_kernel::new_id())
        .bind(hash)
        .bind(now)
        .bind(now)
        .execute(&self.db)
        .await?;
        if inserted.rows_affected() == 0 {
            return Err(AuthError::AlreadySetUp);
        }
        self.open_session(user_agent).await
    }

    /// 密码登录；开启了两步验证时还要验证码（缺少时返回 TotpRequired）。
    pub async fn log_in(
        &self,
        password: &str,
        code: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<String, AuthError> {
        self.ensure_not_locked().await?;
        let checked = async {
            self.check_password(password).await?;
            if let Some(secret) = self.enabled_secret().await? {
                let code = code.filter(|c| !c.trim().is_empty());
                let code = code.ok_or(AuthError::TotpRequired)?;
                self.consume_code(&secret, code).await?;
            }
            Ok(())
        }
        .await;
        match checked {
            Ok(()) => {
                sqlx::query("UPDATE auth_owner SET failed_logins = 0, locked_until = NULL")
                    .execute(&self.db)
                    .await?;
                self.open_session(user_agent).await
            }
            Err(e @ (AuthError::InvalidPassword | AuthError::InvalidTotp)) => {
                self.record_failure().await?;
                Err(e)
            }
            Err(e) => Err(e),
        }
    }

    /// 锁定中则返回剩余秒数。
    async fn ensure_not_locked(&self) -> Result<(), AuthError> {
        let until: Option<i64> = sqlx::query_scalar("SELECT locked_until FROM auth_owner")
            .fetch_optional(&self.db)
            .await?
            .flatten();
        let now = (self.clock)();
        match until {
            Some(u) if u > now => Err(AuthError::Locked {
                retry_after_secs: (u - now + 999) / 1000,
            }),
            _ => Ok(()),
        }
    }

    /// 记一次失败；第 5 次起锁定 1、2、4…分钟，上限 1 小时。
    async fn record_failure(&self) -> Result<(), AuthError> {
        let failed: i64 = sqlx::query_scalar(
            "UPDATE auth_owner SET failed_logins = failed_logins + 1 RETURNING failed_logins",
        )
        .fetch_one(&self.db)
        .await?;
        if failed >= MAX_FAILED_LOGINS {
            let exp = (failed - MAX_FAILED_LOGINS).min(10) as u32;
            let lock = (60_000i64 << exp).min(MAX_LOCK_MS);
            sqlx::query("UPDATE auth_owner SET locked_until = ?")
                .bind((self.clock)() + lock)
                .execute(&self.db)
                .await?;
        }
        Ok(())
    }

    /// 两步验证是否已开启。
    pub async fn totp_enabled(&self) -> Result<bool, AuthError> {
        Ok(self.enabled_secret().await?.is_some())
    }

    /// 开始开启两步验证：生成新密钥（未确认前不生效）。
    pub async fn totp_begin(&self) -> Result<TotpSetup, AuthError> {
        let secret = totp::generate_secret();
        let updated = sqlx::query("UPDATE auth_owner SET totp_pending = ?")
            .bind(STANDARD.encode(self.key.encrypt(&secret)))
            .execute(&self.db)
            .await?;
        if updated.rows_affected() == 0 {
            return Err(AuthError::NotSetUp);
        }
        let b32 = totp::base32(&secret);
        Ok(TotpSetup {
            uri: format!("otpauth://totp/Ownmind:owner?secret={b32}&issuer=Ownmind"),
            secret: b32,
        })
    }

    /// 用一次正确的验证码确认，两步验证生效。
    pub async fn totp_confirm(&self, code: &str) -> Result<(), AuthError> {
        let pending: Option<String> = sqlx::query_scalar("SELECT totp_pending FROM auth_owner")
            .fetch_optional(&self.db)
            .await?
            .flatten();
        let secret = self.open(&pending.ok_or(AuthError::TotpNotPending)?)?;
        let step = totp::verify(&secret, code, (self.clock)()).ok_or(AuthError::InvalidTotp)?;
        sqlx::query(
            "UPDATE auth_owner SET totp_secret = totp_pending, totp_pending = NULL, \
             totp_enabled_at = ?, totp_last_step = ?, updated_at = ?",
        )
        .bind((self.clock)())
        .bind(step)
        .bind((self.clock)())
        .execute(&self.db)
        .await?;
        Ok(())
    }

    /// 关闭两步验证，需要密码。
    pub async fn totp_disable(&self, password: &str) -> Result<(), AuthError> {
        self.ensure_not_locked().await?;
        if let Err(e) = self.check_password(password).await {
            if matches!(e, AuthError::InvalidPassword) {
                self.record_failure().await?;
            }
            return Err(e);
        }
        sqlx::query(
            "UPDATE auth_owner SET totp_secret = NULL, totp_pending = NULL, \
             totp_enabled_at = NULL, totp_last_step = NULL, updated_at = ?",
        )
        .bind((self.clock)())
        .execute(&self.db)
        .await?;
        Ok(())
    }

    async fn check_password(&self, password: &str) -> Result<(), AuthError> {
        let hash: Option<String> = sqlx::query_scalar("SELECT password_hash FROM auth_owner")
            .fetch_optional(&self.db)
            .await?;
        let hash = hash.ok_or(AuthError::NotSetUp)?;
        if !password::verify(password, &hash) {
            return Err(AuthError::InvalidPassword);
        }
        Ok(())
    }

    async fn enabled_secret(&self) -> Result<Option<Vec<u8>>, AuthError> {
        let sealed: Option<String> = sqlx::query_scalar(
            "SELECT totp_secret FROM auth_owner WHERE totp_enabled_at IS NOT NULL",
        )
        .fetch_optional(&self.db)
        .await?
        .flatten();
        sealed.map(|s| self.open(&s)).transpose()
    }

    fn open(&self, sealed: &str) -> Result<Vec<u8>, AuthError> {
        let bytes = STANDARD
            .decode(sealed)
            .map_err(|_| AuthError::Secret(ownmind_kernel::SecretError::Decrypt))?;
        Ok(self.key.decrypt(&bytes)?)
    }

    /// 校验验证码，并记下步数：同一步（及更早的）验证码不能再用。
    async fn consume_code(&self, secret: &[u8], code: &str) -> Result<(), AuthError> {
        let step = totp::verify(secret, code, (self.clock)()).ok_or(AuthError::InvalidTotp)?;
        let updated = sqlx::query(
            "UPDATE auth_owner SET totp_last_step = ? \
             WHERE totp_last_step IS NULL OR totp_last_step < ?",
        )
        .bind(step)
        .bind(step)
        .execute(&self.db)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(AuthError::InvalidTotp);
        }
        Ok(())
    }

    /// 校验令牌；有效则续期。
    pub async fn authenticate(&self, raw_token: &str) -> Result<(), AuthError> {
        let now = (self.clock)();
        let updated = sqlx::query(
            "UPDATE auth_login_session SET last_seen_at = ?, expires_at = ? \
             WHERE token_hash = ? AND expires_at > ?",
        )
        .bind(now)
        .bind(now + SESSION_TTL_MS)
        .bind(token::hash(raw_token))
        .bind(now)
        .execute(&self.db)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(AuthError::Unauthenticated);
        }
        Ok(())
    }

    pub async fn log_out(&self, raw_token: &str) -> Result<(), AuthError> {
        sqlx::query("DELETE FROM auth_login_session WHERE token_hash = ?")
            .bind(token::hash(raw_token))
            .execute(&self.db)
            .await?;
        Ok(())
    }

    async fn open_session(&self, user_agent: Option<&str>) -> Result<String, AuthError> {
        let now = (self.clock)();
        let raw = token::generate();
        sqlx::query("DELETE FROM auth_login_session WHERE expires_at <= ?")
            .bind(now)
            .execute(&self.db)
            .await?;
        sqlx::query(
            "INSERT INTO auth_login_session (id, token_hash, user_agent, created_at, last_seen_at, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(ownmind_kernel::new_id())
        .bind(token::hash(&raw))
        .bind(user_agent)
        .bind(now)
        .bind(now)
        .bind(now + SESSION_TTL_MS)
        .execute(&self.db)
        .await?;
        Ok(raw)
    }
}
