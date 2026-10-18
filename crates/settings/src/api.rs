//! 公共入口：其他模块只能用这里导出的东西。

use ownmind_kernel::MasterKey;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ts_rs::TS;

use crate::internal::{key_hint, valid_base_url};

/// 本模块的数据库迁移，由 server 合并执行。
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

const MAX_LABEL_CHARS: usize = 100;
const DEFAULT_MODEL_KEY: &str = "default_model";
const PROFILE_KEY: &str = "profile";
const SEARCH_KEY: &str = "search";
const SYSTEM_KEY: &str = "system";
const MAX_NAME_CHARS: usize = 100;
const MAX_BIO_CHARS: usize = 2000;

/// 主人资料：放进上下文的核心层，让 agent 知道在和谁说话。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "settings.ts", rename_all = "camelCase")]
pub struct Profile {
    pub name: String,
    /// 希望 agent 怎么称呼主人。
    pub preferred_name: String,
    pub bio: String,
}

/// 界面语言。auto 跟随浏览器。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "settings.ts", rename_all = "lowercase")]
pub enum Language {
    #[default]
    Auto,
    Zh,
    En,
}

/// 系统偏好。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "settings.ts", rename_all = "camelCase")]
pub struct SystemPrefs {
    pub language: Language,
    /// IANA 时区名，如 Asia/Shanghai。
    pub timezone: String,
}

impl Default for SystemPrefs {
    fn default() -> Self {
        Self {
            language: Language::Auto,
            timezone: "UTC".into(),
        }
    }
}

/// 模型供应商种类（SCOPE 预设顺序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = "settings.ts", rename_all = "kebab-case")]
pub enum ProviderKind {
    OpenaiCompatible,
    AnthropicCompatible,
    Openai,
    Anthropic,
    Google,
    Deepseek,
    Openrouter,
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenaiCompatible => "openai-compatible",
            Self::AnthropicCompatible => "anthropic-compatible",
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Google => "google",
            Self::Deepseek => "deepseek",
            Self::Openrouter => "openrouter",
        }
    }

    fn parse(s: &str) -> Self {
        serde_json::from_value(serde_json::Value::String(s.to_owned()))
            .expect("库里的 kind 受 CHECK 约束")
    }

    /// 兼容类必须由主人提供地址；内置供应商可选覆盖地址（如代理）。
    pub fn requires_base_url(self) -> bool {
        matches!(self, Self::OpenaiCompatible | Self::AnthropicCompatible)
    }
}

/// 页面看到的凭据：没有 Key，只有末 4 位提示。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "settings.ts", rename_all = "camelCase")]
pub struct Credential {
    pub id: String,
    pub kind: ProviderKind,
    pub label: String,
    pub base_url: Option<String>,
    pub key_hint: String,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

/// 新增或修改凭据。修改时 api_key 为空表示保留原 Key。
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "settings.ts", rename_all = "camelCase")]
pub struct CredentialInput {
    pub kind: ProviderKind,
    pub label: String,
    #[ts(optional)]
    pub base_url: Option<String>,
    #[ts(optional)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "settings.ts", rename_all = "camelCase")]
pub struct DefaultModel {
    pub credential_id: String,
    pub model_id: String,
}

/// runtime 用的完整模型配置，含明文 Key。只在 server 进程内与 agent 之间流动。
#[derive(Clone)]
pub struct ResolvedModel {
    pub kind: ProviderKind,
    pub base_url: Option<String>,
    pub api_key: String,
    pub model_id: String,
}

impl std::fmt::Debug for ResolvedModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedModel")
            .field("kind", &self.kind)
            .field("base_url", &self.base_url)
            .field("model_id", &self.model_id)
            .finish_non_exhaustive()
    }
}

/// 联网搜索后端。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "settings.ts", rename_all = "lowercase")]
pub enum SearchProvider {
    Brave,
    Tavily,
    Searxng,
}

/// 页面看到的搜索配置：没有 Key，只有末 4 位提示。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "settings.ts", rename_all = "camelCase")]
pub struct SearchConfig {
    pub provider: SearchProvider,
    /// SearXNG 的地址。
    pub base_url: Option<String>,
    pub key_hint: Option<String>,
}

/// 保存搜索配置。api_key 为空表示保留原 Key（同一供应商下）。
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "settings.ts", rename_all = "camelCase")]
pub struct SearchInput {
    pub provider: SearchProvider,
    #[ts(optional)]
    pub base_url: Option<String>,
    #[ts(optional)]
    pub api_key: Option<String>,
}

/// runtime 用的完整搜索配置，含明文 Key。
#[derive(Clone)]
pub struct ResolvedSearch {
    pub provider: SearchProvider,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
}

impl std::fmt::Debug for ResolvedSearch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedSearch")
            .field("provider", &self.provider)
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

/// 库里存的搜索配置（Key 为主密钥加密后的十六进制）。
#[derive(Serialize, Deserialize)]
struct StoredSearch {
    provider: SearchProvider,
    base_url: Option<String>,
    key_hex: Option<String>,
    key_hint: Option<String>,
}

fn to_hex(b: &[u8]) -> String {
    use std::fmt::Write;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("Brave / Tavily 需要填 API Key；SearXNG 需要填地址")]
    InvalidSearch,
    #[error("凭据不存在")]
    NotFound,
    #[error("名称不能为空，且不超过 {MAX_LABEL_CHARS} 个字符")]
    InvalidLabel,
    #[error("地址必须以 http:// 或 https:// 开头")]
    InvalidBaseUrl,
    #[error("API Key 不能为空")]
    MissingKey,
    #[error("模型 ID 不能为空")]
    InvalidModel,
    #[error("还没有设置默认模型")]
    NoDefaultModel,
    #[error("名字与称呼不超过 {MAX_NAME_CHARS} 个字符，简介不超过 {MAX_BIO_CHARS} 个字符")]
    InvalidProfile,
    #[error("时区不合法")]
    InvalidTimezone,
    #[error(transparent)]
    Secret(#[from] ownmind_kernel::SecretError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(sqlx::FromRow)]
struct CredentialRow {
    id: String,
    kind: String,
    label: String,
    base_url: Option<String>,
    key_ciphertext: Vec<u8>,
    key_hint: String,
    created_at: i64,
    updated_at: i64,
}

impl CredentialRow {
    fn public(&self) -> Credential {
        Credential {
            id: self.id.clone(),
            kind: ProviderKind::parse(&self.kind),
            label: self.label.clone(),
            base_url: self.base_url.clone(),
            key_hint: self.key_hint.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(Clone)]
pub struct Settings {
    db: SqlitePool,
    key: MasterKey,
}

struct Clean {
    label: String,
    base_url: Option<String>,
    api_key: Option<String>,
}

fn clean(input: CredentialInput) -> Result<Clean, SettingsError> {
    let label = input.label.trim().to_owned();
    if label.is_empty() || label.chars().count() > MAX_LABEL_CHARS {
        return Err(SettingsError::InvalidLabel);
    }
    let base_url = input
        .base_url
        .map(|u| u.trim().trim_end_matches('/').to_owned())
        .filter(|u| !u.is_empty());
    match &base_url {
        Some(u) if !valid_base_url(u) => return Err(SettingsError::InvalidBaseUrl),
        None if input.kind.requires_base_url() => return Err(SettingsError::InvalidBaseUrl),
        _ => {}
    }
    let api_key = input
        .api_key
        .map(|k| k.trim().to_owned())
        .filter(|k| !k.is_empty());
    Ok(Clean {
        label,
        base_url,
        api_key,
    })
}

impl Settings {
    pub fn new(db: SqlitePool, key: MasterKey) -> Self {
        Self { db, key }
    }

    pub async fn list_credentials(&self) -> Result<Vec<Credential>, SettingsError> {
        let rows: Vec<CredentialRow> =
            sqlx::query_as("SELECT * FROM settings_credential ORDER BY created_at, id")
                .fetch_all(&self.db)
                .await?;
        Ok(rows.iter().map(CredentialRow::public).collect())
    }

    pub async fn create_credential(
        &self,
        input: CredentialInput,
    ) -> Result<Credential, SettingsError> {
        let kind = input.kind;
        let c = clean(input)?;
        let api_key = c.api_key.ok_or(SettingsError::MissingKey)?;
        let now = ownmind_kernel::now_ms();
        let row = CredentialRow {
            id: ownmind_kernel::new_id(),
            kind: kind.as_str().to_owned(),
            label: c.label,
            base_url: c.base_url,
            key_ciphertext: self.key.encrypt(api_key.as_bytes()),
            key_hint: key_hint(&api_key),
            created_at: now,
            updated_at: now,
        };
        sqlx::query(
            "INSERT INTO settings_credential \
             (id, kind, label, base_url, key_ciphertext, key_hint, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.kind)
        .bind(&row.label)
        .bind(&row.base_url)
        .bind(&row.key_ciphertext)
        .bind(&row.key_hint)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(&self.db)
        .await?;
        Ok(row.public())
    }

    pub async fn update_credential(
        &self,
        id: &str,
        input: CredentialInput,
    ) -> Result<Credential, SettingsError> {
        let kind = input.kind;
        let c = clean(input)?;
        let mut row = self.row(id).await?;
        row.kind = kind.as_str().to_owned();
        row.label = c.label;
        row.base_url = c.base_url;
        if let Some(k) = c.api_key {
            row.key_ciphertext = self.key.encrypt(k.as_bytes());
            row.key_hint = key_hint(&k);
        }
        row.updated_at = ownmind_kernel::now_ms();
        sqlx::query(
            "UPDATE settings_credential SET kind = ?, label = ?, base_url = ?, \
             key_ciphertext = ?, key_hint = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&row.kind)
        .bind(&row.label)
        .bind(&row.base_url)
        .bind(&row.key_ciphertext)
        .bind(&row.key_hint)
        .bind(row.updated_at)
        .bind(&row.id)
        .execute(&self.db)
        .await?;
        Ok(row.public())
    }

    /// 删除凭据；它若是默认模型所用的，默认模型一起清空。
    pub async fn delete_credential(&self, id: &str) -> Result<(), SettingsError> {
        let mut tx = self.db.begin().await?;
        let deleted = sqlx::query("DELETE FROM settings_credential WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if deleted == 0 {
            return Err(SettingsError::NotFound);
        }
        sqlx::query(
            "DELETE FROM settings_preference WHERE key = ? \
             AND json_extract(value, '$.credentialId') = ?",
        )
        .bind(DEFAULT_MODEL_KEY)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn default_model(&self) -> Result<Option<DefaultModel>, SettingsError> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT value FROM settings_preference WHERE key = ?")
                .bind(DEFAULT_MODEL_KEY)
                .fetch_optional(&self.db)
                .await?;
        Ok(value.map(|v| serde_json::from_str(&v).expect("default_model 由本模块写入")))
    }

    pub async fn set_default_model(&self, model: DefaultModel) -> Result<(), SettingsError> {
        let model_id = model.model_id.trim().to_owned();
        if model_id.is_empty() {
            return Err(SettingsError::InvalidModel);
        }
        self.row(&model.credential_id).await?;
        let value = serde_json::to_string(&DefaultModel {
            credential_id: model.credential_id,
            model_id,
        })
        .expect("可序列化");
        sqlx::query(
            "INSERT INTO settings_preference (key, value, updated_at) VALUES (?, ?, ?) \
             ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(DEFAULT_MODEL_KEY)
        .bind(value)
        .bind(ownmind_kernel::now_ms())
        .execute(&self.db)
        .await?;
        Ok(())
    }

    pub async fn profile(&self) -> Result<Profile, SettingsError> {
        self.get_pref(PROFILE_KEY).await
    }

    pub async fn set_profile(&self, p: Profile) -> Result<Profile, SettingsError> {
        let p = Profile {
            name: p.name.trim().to_owned(),
            preferred_name: p.preferred_name.trim().to_owned(),
            bio: p.bio.trim().to_owned(),
        };
        let too_long = |s: &str, max| s.chars().count() > max;
        if too_long(&p.name, MAX_NAME_CHARS)
            || too_long(&p.preferred_name, MAX_NAME_CHARS)
            || too_long(&p.bio, MAX_BIO_CHARS)
        {
            return Err(SettingsError::InvalidProfile);
        }
        self.put_pref(PROFILE_KEY, &p).await?;
        Ok(p)
    }

    pub async fn system(&self) -> Result<SystemPrefs, SettingsError> {
        self.get_pref(SYSTEM_KEY).await
    }

    pub async fn set_system(&self, p: SystemPrefs) -> Result<SystemPrefs, SettingsError> {
        let tz = p.timezone.trim();
        let ok = !tz.is_empty()
            && tz.len() <= 64
            && tz
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "/_+-".contains(c));
        if !ok {
            return Err(SettingsError::InvalidTimezone);
        }
        let p = SystemPrefs {
            language: p.language,
            timezone: tz.to_owned(),
        };
        self.put_pref(SYSTEM_KEY, &p).await?;
        Ok(p)
    }

    /// 当前搜索配置；没配过返回 None。
    pub async fn search_config(&self) -> Result<Option<SearchConfig>, SettingsError> {
        Ok(self.stored_search().await?.map(|s| SearchConfig {
            provider: s.provider,
            base_url: s.base_url,
            key_hint: s.key_hint,
        }))
    }

    pub async fn set_search_config(
        &self,
        input: SearchInput,
    ) -> Result<SearchConfig, SettingsError> {
        let old = self
            .stored_search()
            .await?
            .filter(|o| o.provider == input.provider);
        let base_url = input
            .base_url
            .map(|u| u.trim().trim_end_matches('/').to_owned())
            .filter(|u| !u.is_empty());
        if base_url.as_deref().is_some_and(|u| !valid_base_url(u)) {
            return Err(SettingsError::InvalidBaseUrl);
        }
        let new_key = input
            .api_key
            .map(|k| k.trim().to_owned())
            .filter(|k| !k.is_empty());
        let stored = if input.provider == SearchProvider::Searxng {
            StoredSearch {
                provider: input.provider,
                base_url: Some(base_url.ok_or(SettingsError::InvalidSearch)?),
                key_hex: None,
                key_hint: None,
            }
        } else {
            let (key_hex, key_hint) = match (new_key, old) {
                (Some(k), _) => (to_hex(&self.key.encrypt(k.as_bytes())), key_hint(&k)),
                (
                    None,
                    Some(StoredSearch {
                        key_hex: Some(h),
                        key_hint: Some(t),
                        ..
                    }),
                ) => (h, t),
                _ => return Err(SettingsError::InvalidSearch),
            };
            StoredSearch {
                provider: input.provider,
                base_url: None,
                key_hex: Some(key_hex),
                key_hint: Some(key_hint),
            }
        };
        self.put_pref(SEARCH_KEY, &stored).await?;
        Ok(SearchConfig {
            provider: stored.provider,
            base_url: stored.base_url,
            key_hint: stored.key_hint,
        })
    }

    pub async fn clear_search_config(&self) -> Result<(), SettingsError> {
        sqlx::query("DELETE FROM settings_preference WHERE key = ?")
            .bind(SEARCH_KEY)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    /// 搜索配置 + 解密后的 Key，给 web_search 工具用。
    pub async fn resolve_search(&self) -> Result<Option<ResolvedSearch>, SettingsError> {
        let Some(s) = self.stored_search().await? else {
            return Ok(None);
        };
        let api_key = match s.key_hex {
            Some(h) => {
                let bytes = from_hex(&h).ok_or(ownmind_kernel::SecretError::Decrypt)?;
                Some(
                    String::from_utf8(self.key.decrypt(&bytes)?)
                        .map_err(|_| ownmind_kernel::SecretError::Decrypt)?,
                )
            }
            None => None,
        };
        Ok(Some(ResolvedSearch {
            provider: s.provider,
            base_url: s.base_url,
            api_key,
        }))
    }

    async fn stored_search(&self) -> Result<Option<StoredSearch>, SettingsError> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT value FROM settings_preference WHERE key = ?")
                .bind(SEARCH_KEY)
                .fetch_optional(&self.db)
                .await?;
        Ok(value.and_then(|v| serde_json::from_str(&v).ok()))
    }

    async fn get_pref<T: serde::de::DeserializeOwned + Default>(
        &self,
        key: &str,
    ) -> Result<T, SettingsError> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT value FROM settings_preference WHERE key = ?")
                .bind(key)
                .fetch_optional(&self.db)
                .await?;
        Ok(value
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default())
    }

    async fn put_pref<T: Serialize>(&self, key: &str, value: &T) -> Result<(), SettingsError> {
        sqlx::query(
            "INSERT INTO settings_preference (key, value, updated_at) VALUES (?, ?, ?) \
             ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(serde_json::to_string(value).expect("可序列化"))
        .bind(ownmind_kernel::now_ms())
        .execute(&self.db)
        .await?;
        Ok(())
    }

    /// 默认模型 + 解密后的 Key，给 runtime 发起运行用。
    pub async fn resolve_default_model(&self) -> Result<ResolvedModel, SettingsError> {
        let model = self
            .default_model()
            .await?
            .ok_or(SettingsError::NoDefaultModel)?;
        let row = self.row(&model.credential_id).await?;
        let api_key = String::from_utf8(self.key.decrypt(&row.key_ciphertext)?)
            .map_err(|_| ownmind_kernel::SecretError::Decrypt)?;
        Ok(ResolvedModel {
            kind: ProviderKind::parse(&row.kind),
            base_url: row.base_url,
            api_key,
            model_id: model.model_id,
        })
    }

    async fn row(&self, id: &str) -> Result<CredentialRow, SettingsError> {
        sqlx::query_as("SELECT * FROM settings_credential WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.db)
            .await?
            .ok_or(SettingsError::NotFound)
    }
}
