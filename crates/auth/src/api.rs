//! 公共入口：其他模块只能用这里导出的东西。

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::internal::{password, token};

/// 本模块的数据库迁移，由 server 合并执行。
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// 登录会话有效期：30 天，每次使用后从使用时刻重新计算。
pub const SESSION_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// 主人密码最短长度（字符）。
pub const MIN_PASSWORD_CHARS: usize = 8;

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
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// 主人身份与登录会话。
#[derive(Clone)]
pub struct Auth {
    db: SqlitePool,
    clock: Clock,
}

impl Auth {
    pub fn new(db: SqlitePool) -> Self {
        Self::with_clock(db, Arc::new(ownmind_kernel::now_ms))
    }

    pub fn with_clock(db: SqlitePool, clock: Clock) -> Self {
        Self { db, clock }
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

    pub async fn log_in(
        &self,
        password: &str,
        user_agent: Option<&str>,
    ) -> Result<String, AuthError> {
        let hash: Option<String> = sqlx::query_scalar("SELECT password_hash FROM auth_owner")
            .fetch_optional(&self.db)
            .await?;
        let hash = hash.ok_or(AuthError::NotSetUp)?;
        if !password::verify(password, &hash) {
            return Err(AuthError::InvalidPassword);
        }
        self.open_session(user_agent).await
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
