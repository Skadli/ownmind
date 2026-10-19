//! 公共入口：其他模块只能用这里导出的东西。

use ownmind_kernel::{new_id, now_ms};
use serde::Serialize;
use sqlx::SqlitePool;

/// 本模块的数据库迁移，由 server 合并执行。
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// 核心记忆上限（token 估算）。
pub const CORE_MAX_TOKENS: i64 = 2000;
const ENTRY_MAX_CHARS: usize = 2000;
const SEARCH_MAX: i64 = 50;

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("核心记忆太长：约 {0} token，上限 {CORE_MAX_TOKENS}；请精简后再写")]
    CoreTooLong(i64),
    #[error("记忆条目不能为空，且不超过 {ENTRY_MAX_CHARS} 字")]
    BadEntry,
    #[error("核心记忆已被别的会话改过，请重新 read_core 后合并再写")]
    Conflict,
    #[error("数据库错误：{0}")]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub content: String,
    pub created_at: i64,
}

#[derive(Clone)]
pub struct Memory {
    db: SqlitePool,
}

/// token 估算：ASCII 约 4 字符一个，其他（中文等）约一字符一个。
pub fn estimate_tokens(text: &str) -> i64 {
    let (ascii, other) = text.chars().fold((0i64, 0i64), |(a, o), c| {
        if c.is_ascii() { (a + 1, o) } else { (a, o + 1) }
    });
    (ascii + 3) / 4 + other
}

impl Memory {
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }

    /// 核心记忆全文；还没写过返回空串。
    pub async fn core(&self) -> Result<String, MemoryError> {
        let row: Option<(String,)> = sqlx::query_as("SELECT content FROM mem_core WHERE id = 1")
            .fetch_optional(&self.db)
            .await?;
        Ok(row.map(|r| r.0).unwrap_or_default())
    }

    /// 整体替换核心记忆；超过 2k token 拒绝。仅当当前内容仍等于 `base`（读到的那版）
    /// 才写入，否则返回 Conflict，调用方须重新读取并合并。
    pub async fn set_core(&self, base: &str, content: &str) -> Result<(), MemoryError> {
        let content = content.trim();
        let t = estimate_tokens(content);
        if t > CORE_MAX_TOKENS {
            return Err(MemoryError::CoreTooLong(t));
        }
        let now = now_ms();
        let base = base.trim();
        // 单条原子语句：行缺失时插入（仅空 base 合法）；否则仅当内容仍等于 base 才更新。
        let r = sqlx::query(
            "INSERT INTO mem_core (id, content, updated_at) \
             SELECT 1, ?1, ?2 WHERE ?3 = '' OR EXISTS (SELECT 1 FROM mem_core WHERE id = 1) \
             ON CONFLICT(id) DO UPDATE SET content = excluded.content, updated_at = excluded.updated_at \
             WHERE mem_core.content = ?3",
        )
        .bind(content)
        .bind(now)
        .bind(base)
        .execute(&self.db)
        .await?;
        if r.rows_affected() == 0 {
            return Err(MemoryError::Conflict);
        }
        Ok(())
    }

    pub async fn add(&self, content: &str) -> Result<Entry, MemoryError> {
        let content = content.trim();
        if content.is_empty() || content.chars().count() > ENTRY_MAX_CHARS {
            return Err(MemoryError::BadEntry);
        }
        let e = Entry {
            id: new_id(),
            content: content.to_owned(),
            created_at: now_ms(),
        };
        sqlx::query("INSERT INTO mem_entry (id, content, created_at) VALUES (?, ?, ?)")
            .bind(&e.id)
            .bind(&e.content)
            .bind(e.created_at)
            .execute(&self.db)
            .await?;
        Ok(e)
    }

    pub async fn delete(&self, id: &str) -> Result<bool, MemoryError> {
        let r = sqlx::query("DELETE FROM mem_entry WHERE id = ?")
            .bind(id)
            .execute(&self.db)
            .await?;
        Ok(r.rows_affected() > 0)
    }

    /// 最近的条目，新的在前。
    pub async fn recent(&self, limit: i64) -> Result<Vec<Entry>, MemoryError> {
        Ok(sqlx::query_as(
            "SELECT id, content, created_at FROM mem_entry ORDER BY created_at DESC, id DESC LIMIT ?",
        )
        .bind(limit.clamp(1, SEARCH_MAX))
        .fetch_all(&self.db)
        .await?)
    }

    /// 按关键词检索（空白分隔，全部命中）。词都不短于 3 个字符走 FTS5（trigram），
    /// 否则退回子串匹配（中文两字词）。
    pub async fn search(&self, query: &str, limit: i64) -> Result<Vec<Entry>, MemoryError> {
        let terms: Vec<&str> = query.split_whitespace().collect();
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let limit = limit.clamp(1, SEARCH_MAX);
        if terms.iter().all(|t| t.chars().count() >= 3) {
            let m = terms
                .iter()
                .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(" AND ");
            return Ok(sqlx::query_as(
                "SELECT e.id, e.content, e.created_at FROM mem_entry_fts f \
                 JOIN mem_entry e ON e.rowid = f.rowid \
                 WHERE mem_entry_fts MATCH ? ORDER BY f.rank, e.created_at DESC LIMIT ?",
            )
            .bind(m)
            .bind(limit)
            .fetch_all(&self.db)
            .await?);
        }
        let mut sql = String::from("SELECT id, content, created_at FROM mem_entry WHERE 1=1");
        for _ in &terms {
            sql.push_str(" AND content LIKE ? ESCAPE '\\'");
        }
        sql.push_str(" ORDER BY created_at DESC, id DESC LIMIT ?");
        let mut q = sqlx::query_as::<_, Entry>(sqlx::AssertSqlSafe(sql));
        for t in &terms {
            let esc = t
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            q = q.bind(format!("%{esc}%"));
        }
        Ok(q.bind(limit).fetch_all(&self.db).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem() -> (Memory, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}?mode=rwc", dir.path().join("t.db").display());
        let pool = SqlitePool::connect(&url).await.unwrap();
        MIGRATOR.run(&pool).await.unwrap();
        (Memory::new(pool), dir)
    }

    #[tokio::test]
    async fn core_roundtrip_and_limit() {
        let (m, _d) = mem().await;
        assert_eq!(m.core().await.unwrap(), "");
        m.set_core("", "喜欢喝茶").await.unwrap();
        m.set_core("喜欢喝茶", "喜欢喝咖啡").await.unwrap();
        assert_eq!(m.core().await.unwrap(), "喜欢喝咖啡");
        let long = "字".repeat(2001);
        assert!(matches!(
            m.set_core("喜欢喝咖啡", &long).await,
            Err(MemoryError::CoreTooLong(_))
        ));
        assert_eq!(m.core().await.unwrap(), "喜欢喝咖啡");
        // 清空后，空 base 仍可再次写入；过期 base 仍冲突。
        m.set_core("喜欢喝咖啡", "").await.unwrap();
        assert!(matches!(
            m.set_core("喜欢喝咖啡", "x").await,
            Err(MemoryError::Conflict)
        ));
        m.set_core("", "新内容").await.unwrap();
        assert!(matches!(
            m.set_core("", "y").await,
            Err(MemoryError::Conflict)
        ));
    }

    #[tokio::test]
    async fn search_chinese_english_and_short_terms() {
        let (m, _d) = mem().await;
        m.add("主人的生日是三月五号").await.unwrap();
        m.add("Prefers Rust over Go for servers").await.unwrap();
        m.add("宠物是一只叫 Mochi 的猫").await.unwrap();
        assert_eq!(m.search("生日是", 10).await.unwrap().len(), 1);
        assert_eq!(m.search("生日", 10).await.unwrap().len(), 1);
        assert_eq!(m.search("rust servers", 10).await.unwrap().len(), 1);
        assert_eq!(m.search("mochi", 10).await.unwrap().len(), 1);
        assert!(m.search("不存在的东西", 10).await.unwrap().is_empty());
        assert!(m.search("   ", 10).await.unwrap().is_empty());
        assert_eq!(m.search("100%", 10).await.unwrap().len(), 0);
        assert_eq!(m.search("\"", 10).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn delete_removes_from_index() {
        let (m, _d) = mem().await;
        let e = m.add("临时的一条记忆").await.unwrap();
        assert_eq!(m.search("临时的", 10).await.unwrap().len(), 1);
        assert!(m.delete(&e.id).await.unwrap());
        assert!(!m.delete(&e.id).await.unwrap());
        assert!(m.search("临时的", 10).await.unwrap().is_empty());
        assert!(m.recent(5).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn rejects_bad_entries() {
        let (m, _d) = mem().await;
        assert!(matches!(m.add("  ").await, Err(MemoryError::BadEntry)));
        assert!(m.add(&"x".repeat(2001)).await.is_err());
    }
}
