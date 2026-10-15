//! 公共入口：其他模块只能用这里导出的东西。

use serde::Serialize;
use sqlx::SqlitePool;
use ts_rs::TS;

/// 本模块的数据库迁移，由 server 合并执行。
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub const DEFAULT_TITLE: &str = "新会话";
pub const MAX_TITLE_CHARS: usize = 200;

/// 会话：主人与 agent 的一条对话线程。
#[derive(Debug, Clone, Serialize, TS, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "conversation.ts", rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub title: String,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum ConversationError {
    #[error("会话不存在")]
    NotFound,
    #[error("标题不能为空，且不超过 {MAX_TITLE_CHARS} 个字符")]
    InvalidTitle,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Clone)]
pub struct Conversations {
    db: SqlitePool,
}

impl Conversations {
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }

    pub async fn create(&self, title: Option<&str>) -> Result<Conversation, ConversationError> {
        let title = match title {
            Some(t) => clean_title(t)?,
            None => DEFAULT_TITLE.to_owned(),
        };
        let now = ownmind_kernel::now_ms();
        let conv = Conversation {
            id: ownmind_kernel::new_id(),
            title,
            created_at: now,
            updated_at: now,
        };
        sqlx::query(
            "INSERT INTO conv_conversation (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&conv.id)
        .bind(&conv.title)
        .bind(conv.created_at)
        .bind(conv.updated_at)
        .execute(&self.db)
        .await?;
        Ok(conv)
    }

    /// 全部会话，最近更新的在前。
    pub async fn list(&self) -> Result<Vec<Conversation>, ConversationError> {
        Ok(
            sqlx::query_as("SELECT * FROM conv_conversation ORDER BY updated_at DESC, id DESC")
                .fetch_all(&self.db)
                .await?,
        )
    }

    pub async fn rename(&self, id: &str, title: &str) -> Result<Conversation, ConversationError> {
        let title = clean_title(title)?;
        sqlx::query_as(
            "UPDATE conv_conversation SET title = ?, updated_at = ? WHERE id = ? RETURNING *",
        )
        .bind(title)
        .bind(ownmind_kernel::now_ms())
        .bind(id)
        .fetch_optional(&self.db)
        .await?
        .ok_or(ConversationError::NotFound)
    }

    pub async fn delete(&self, id: &str) -> Result<(), ConversationError> {
        let deleted = sqlx::query("DELETE FROM conv_conversation WHERE id = ?")
            .bind(id)
            .execute(&self.db)
            .await?;
        if deleted.rows_affected() == 0 {
            return Err(ConversationError::NotFound);
        }
        Ok(())
    }
}

fn clean_title(raw: &str) -> Result<String, ConversationError> {
    let t = raw.trim();
    if t.is_empty() || t.chars().count() > MAX_TITLE_CHARS {
        return Err(ConversationError::InvalidTitle);
    }
    Ok(t.to_owned())
}
