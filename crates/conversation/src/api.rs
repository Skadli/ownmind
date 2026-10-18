//! 公共入口：其他模块只能用这里导出的东西。

use serde::{Deserialize, Serialize};
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

/// 消息的发送方。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "conversation.ts", rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    /// 工具结果（pi 的 toolResult 消息）。
    Tool,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

/// 会话里的一条消息。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "conversation.ts", rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    #[ts(type = "number")]
    pub seq: i64,
    pub role: Role,
    /// pi 的消息 JSON，原样回放给模型。
    #[ts(type = "unknown")]
    pub body: serde_json::Value,
    pub text: String,
    #[ts(type = "number | null")]
    pub input_tokens: Option<i64>,
    #[ts(type = "number | null")]
    pub output_tokens: Option<i64>,
    #[ts(type = "number")]
    pub created_at: i64,
}

/// 压缩摘要。
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Compaction {
    pub upto_seq: i64,
    pub summary: String,
}

/// 追加消息的入参。
#[derive(Debug, Clone)]
pub struct NewMessage {
    pub role: Role,
    pub body: serde_json::Value,
    pub text: String,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct MessageRow {
    id: String,
    conversation_id: String,
    seq: i64,
    role: String,
    body: String,
    text: String,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    created_at: i64,
}

impl From<MessageRow> for Message {
    fn from(r: MessageRow) -> Self {
        Self {
            id: r.id,
            conversation_id: r.conversation_id,
            seq: r.seq,
            role: match r.role.as_str() {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                "tool" => Role::Tool,
                other => unreachable!("role 受 CHECK 约束：{other}"),
            },
            body: serde_json::from_str(&r.body).expect("body 由本模块写入，是合法 JSON"),
            text: r.text,
            input_tokens: r.input_tokens,
            output_tokens: r.output_tokens,
            created_at: r.created_at,
        }
    }
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

    pub async fn get(&self, id: &str) -> Result<Conversation, ConversationError> {
        sqlx::query_as("SELECT * FROM conv_conversation WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.db)
            .await?
            .ok_or(ConversationError::NotFound)
    }

    /// 追加一条消息（seq 自增），并刷新会话的 updated_at。
    pub async fn append(
        &self,
        conversation_id: &str,
        msg: NewMessage,
    ) -> Result<Message, ConversationError> {
        let now = ownmind_kernel::now_ms();
        let mut tx = self.db.begin().await?;
        let touched = sqlx::query("UPDATE conv_conversation SET updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(conversation_id)
            .execute(&mut *tx)
            .await?;
        if touched.rows_affected() == 0 {
            return Err(ConversationError::NotFound);
        }
        let row: MessageRow = sqlx::query_as(
            "INSERT INTO conv_message \
             (id, conversation_id, seq, role, body, text, input_tokens, output_tokens, created_at) \
             VALUES (?, ?, (SELECT COALESCE(MAX(seq), 0) + 1 FROM conv_message WHERE conversation_id = ?), \
                     ?, ?, ?, ?, ?, ?) RETURNING *",
        )
        .bind(ownmind_kernel::new_id())
        .bind(conversation_id)
        .bind(conversation_id)
        .bind(msg.role.as_str())
        .bind(msg.body.to_string())
        .bind(&msg.text)
        .bind(msg.input_tokens)
        .bind(msg.output_tokens)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row.into())
    }

    /// 会话的全部消息，按 seq 升序。
    pub async fn messages(&self, conversation_id: &str) -> Result<Vec<Message>, ConversationError> {
        self.get(conversation_id).await?;
        let rows: Vec<MessageRow> =
            sqlx::query_as("SELECT * FROM conv_message WHERE conversation_id = ? ORDER BY seq")
                .bind(conversation_id)
                .fetch_all(&self.db)
                .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// 最近一次压缩摘要。
    pub async fn latest_compaction(
        &self,
        conversation_id: &str,
    ) -> Result<Option<Compaction>, ConversationError> {
        Ok(sqlx::query_as(
            "SELECT upto_seq, summary FROM conv_compaction WHERE conversation_id = ? \
             ORDER BY upto_seq DESC LIMIT 1",
        )
        .bind(conversation_id)
        .fetch_optional(&self.db)
        .await?)
    }

    /// 保存压缩摘要：upto_seq 及之前的消息此后由它代替。
    pub async fn add_compaction(
        &self,
        conversation_id: &str,
        upto_seq: i64,
        summary: &str,
    ) -> Result<(), ConversationError> {
        self.get(conversation_id).await?;
        sqlx::query(
            "INSERT INTO conv_compaction (id, conversation_id, upto_seq, summary, created_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(ownmind_kernel::new_id())
        .bind(conversation_id)
        .bind(upto_seq)
        .bind(summary)
        .bind(ownmind_kernel::now_ms())
        .execute(&self.db)
        .await?;
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
