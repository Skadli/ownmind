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

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
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
