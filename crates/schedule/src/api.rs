//! 公共入口：其他模块只能用这里导出的东西。

use ownmind_kernel::{new_id, now_ms};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::broadcast;

/// 本模块的数据库迁移，由 server 合并执行。
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// 最短间隔（秒）。
pub const MIN_INTERVAL_SECS: i64 = 60;
const NAME_MAX_CHARS: usize = 100;
const PROMPT_MAX_CHARS: usize = 8000;
const FIRES_MAX: i64 = 200;

#[derive(Debug, thiserror::Error)]
pub enum ScheduleError {
    #[error("任务名不能为空，且不超过 {NAME_MAX_CHARS} 字")]
    BadName,
    #[error("任务内容不能为空，且不超过 {PROMPT_MAX_CHARS} 字")]
    BadPrompt,
    #[error("间隔不能短于 {MIN_INTERVAL_SECS} 秒")]
    BadInterval,
    #[error("任务不存在")]
    NotFound,
    #[error("数据库错误：{0}")]
    Db(#[from] sqlx::Error),
}

/// 触发计划：按固定间隔重复，或在某一时刻只跑一次。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Schedule {
    #[serde(rename_all = "camelCase")]
    Interval { every_secs: i64 },
    #[serde(rename_all = "camelCase")]
    Once { run_at: i64 },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInput {
    pub name: String,
    pub prompt: String,
    pub schedule: Schedule,
}

/// 局部更新：没给的字段保持不变；给了 schedule 就重新计算下次时间。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskPatch {
    pub name: Option<String>,
    pub prompt: Option<String>,
    pub schedule: Option<Schedule>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub schedule: Schedule,
    pub next_run_at: Option<i64>,
    pub enabled: bool,
    pub created_at: i64,
    pub last_run_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Fire {
    pub id: String,
    pub task_id: String,
    pub fired_at: i64,
    pub conversation_id: Option<String>,
    pub status: String,
    pub error: Option<String>,
}

/// 任务到期事件。
#[derive(Debug, Clone)]
pub struct TaskDue {
    pub task: Task,
}

#[derive(sqlx::FromRow)]
struct Row {
    id: String,
    name: String,
    prompt: String,
    kind: String,
    interval_secs: Option<i64>,
    next_run_at: Option<i64>,
    enabled: i64,
    created_at: i64,
    last_run_at: Option<i64>,
}

impl Row {
    fn into_task(self) -> Task {
        let schedule = if self.kind == "interval" {
            Schedule::Interval {
                every_secs: self.interval_secs.unwrap_or(MIN_INTERVAL_SECS),
            }
        } else {
            Schedule::Once {
                run_at: self.next_run_at.unwrap_or(self.created_at),
            }
        };
        Task {
            id: self.id,
            name: self.name,
            prompt: self.prompt,
            schedule,
            next_run_at: self.next_run_at,
            enabled: self.enabled != 0,
            created_at: self.created_at,
            last_run_at: self.last_run_at,
        }
    }
}

const COLS: &str =
    "id, name, prompt, kind, interval_secs, next_run_at, enabled, created_at, last_run_at";

fn check_name(s: &str) -> Result<String, ScheduleError> {
    let s = s.trim();
    if s.is_empty() || s.chars().count() > NAME_MAX_CHARS {
        return Err(ScheduleError::BadName);
    }
    Ok(s.to_owned())
}

fn check_prompt(s: &str) -> Result<String, ScheduleError> {
    let s = s.trim();
    if s.is_empty() || s.chars().count() > PROMPT_MAX_CHARS {
        return Err(ScheduleError::BadPrompt);
    }
    Ok(s.to_owned())
}

/// 返回 (kind, interval_secs, 首次触发时间)。
fn plan(s: &Schedule, now: i64) -> Result<(&'static str, Option<i64>, i64), ScheduleError> {
    match *s {
        Schedule::Interval { every_secs } => {
            if every_secs < MIN_INTERVAL_SECS {
                return Err(ScheduleError::BadInterval);
            }
            Ok((
                "interval",
                Some(every_secs),
                now.saturating_add(every_secs.saturating_mul(1000)),
            ))
        }
        Schedule::Once { run_at } => Ok(("once", None, run_at)),
    }
}

#[derive(Clone)]
pub struct Scheduler {
    db: SqlitePool,
    due: broadcast::Sender<TaskDue>,
}

impl Scheduler {
    pub fn new(db: SqlitePool) -> Self {
        let (due, _) = broadcast::channel(64);
        Self { db, due }
    }

    /// 订阅任务到期事件。
    pub fn subscribe(&self) -> broadcast::Receiver<TaskDue> {
        self.due.subscribe()
    }

    pub async fn create(&self, input: TaskInput) -> Result<Task, ScheduleError> {
        let name = check_name(&input.name)?;
        let prompt = check_prompt(&input.prompt)?;
        let now = now_ms();
        let (kind, interval, next) = plan(&input.schedule, now)?;
        let id = new_id();
        sqlx::query(
            "INSERT INTO sched_task (id, name, prompt, kind, interval_secs, next_run_at, enabled, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, 1, ?)",
        )
        .bind(&id)
        .bind(&name)
        .bind(&prompt)
        .bind(kind)
        .bind(interval)
        .bind(next)
        .bind(now)
        .execute(&self.db)
        .await?;
        self.get(&id).await
    }

    pub async fn get(&self, id: &str) -> Result<Task, ScheduleError> {
        let row: Option<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {COLS} FROM sched_task WHERE id = ?"
        )))
        .bind(id)
        .fetch_optional(&self.db)
        .await?;
        row.map(Row::into_task).ok_or(ScheduleError::NotFound)
    }

    /// 新的在前。
    pub async fn list(&self) -> Result<Vec<Task>, ScheduleError> {
        let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {COLS} FROM sched_task ORDER BY created_at DESC, id DESC"
        )))
        .fetch_all(&self.db)
        .await?;
        Ok(rows.into_iter().map(Row::into_task).collect())
    }

    pub async fn update(&self, id: &str, patch: TaskPatch) -> Result<Task, ScheduleError> {
        let cur = self.get(id).await?;
        let name = match &patch.name {
            Some(n) => check_name(n)?,
            None => cur.name.clone(),
        };
        let prompt = match &patch.prompt {
            Some(p) => check_prompt(p)?,
            None => cur.prompt.clone(),
        };
        let enabled = patch.enabled.unwrap_or(cur.enabled);
        let now = now_ms();
        let (kind, interval, mut next) = match &patch.schedule {
            Some(s) => {
                let (k, i, n) = plan(s, now)?;
                (k, i, Some(n))
            }
            None => {
                let (k, i) = match cur.schedule {
                    Schedule::Interval { every_secs } => ("interval", Some(every_secs)),
                    Schedule::Once { .. } => ("once", None),
                };
                (k, i, cur.next_run_at)
            }
        };
        // 重新启用一个已跑完的任务：间隔任务从现在起算；一次性任务没有下次，保持不变。
        if patch.schedule.is_none()
            && patch.enabled == Some(true)
            && !cur.enabled
            && let Schedule::Interval { every_secs } = cur.schedule
        {
            next = Some(now.saturating_add(every_secs.saturating_mul(1000)));
        }
        sqlx::query(
            "UPDATE sched_task SET name = ?, prompt = ?, kind = ?, interval_secs = ?, \
             next_run_at = ?, enabled = ? WHERE id = ?",
        )
        .bind(&name)
        .bind(&prompt)
        .bind(kind)
        .bind(interval)
        .bind(next)
        .bind(i64::from(enabled))
        .bind(id)
        .execute(&self.db)
        .await?;
        self.get(id).await
    }

    pub async fn delete(&self, id: &str) -> Result<(), ScheduleError> {
        let r = sqlx::query("DELETE FROM sched_task WHERE id = ?")
            .bind(id)
            .execute(&self.db)
            .await?;
        if r.rows_affected() == 0 {
            return Err(ScheduleError::NotFound);
        }
        Ok(())
    }

    /// 认领所有到期任务：间隔任务把下次时间推到现在之后（错过的不补跑），一次性任务清空下次时间。
    /// 每个任务只会被认领一次。
    pub async fn claim_due(&self, now: i64) -> Result<Vec<Task>, ScheduleError> {
        let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {COLS} FROM sched_task WHERE enabled = 1 AND next_run_at IS NOT NULL \
             AND next_run_at <= ? ORDER BY next_run_at"
        )))
        .bind(now)
        .fetch_all(&self.db)
        .await?;
        let mut out = Vec::new();
        for row in rows {
            let due_at = row.next_run_at;
            let next = match (row.kind.as_str(), row.interval_secs) {
                ("interval", Some(s)) => Some(now.saturating_add(s.saturating_mul(1000))),
                _ => None,
            };
            let r = sqlx::query(
                "UPDATE sched_task SET next_run_at = ?, last_run_at = ? \
                 WHERE id = ? AND enabled = 1 AND next_run_at IS ?",
            )
            .bind(next)
            .bind(now)
            .bind(&row.id)
            .bind(due_at)
            .execute(&self.db)
            .await?;
            if r.rows_affected() == 1 {
                out.push(self.get(&row.id).await?);
            }
        }
        Ok(out)
    }

    /// 认领到期任务并发出 TaskDue 事件，返回认领数量。
    pub async fn tick(&self) -> Result<usize, ScheduleError> {
        let tasks = self.claim_due(now_ms()).await?;
        let n = tasks.len();
        for task in tasks {
            // 没有订阅者时发送失败是正常的。
            let _ = self.due.send(TaskDue { task });
        }
        Ok(n)
    }

    pub async fn record_fire(
        &self,
        task_id: &str,
        conversation_id: Option<&str>,
        error: Option<&str>,
    ) -> Result<(), ScheduleError> {
        sqlx::query(
            "INSERT INTO sched_fire (id, task_id, fired_at, conversation_id, status, error) \
             SELECT ?, ?, ?, ?, ?, ? WHERE EXISTS (SELECT 1 FROM sched_task WHERE id = ?)",
        )
        .bind(new_id())
        .bind(task_id)
        .bind(now_ms())
        .bind(conversation_id)
        .bind(if error.is_some() { "error" } else { "started" })
        .bind(error)
        .bind(task_id)
        .execute(&self.db)
        .await?;
        Ok(())
    }

    /// 某任务最近的触发记录，新的在前。
    pub async fn fires(&self, task_id: &str, limit: i64) -> Result<Vec<Fire>, ScheduleError> {
        self.get(task_id).await?;
        Ok(sqlx::query_as(
            "SELECT id, task_id, fired_at, conversation_id, status, error FROM sched_fire \
             WHERE task_id = ? ORDER BY fired_at DESC, id DESC LIMIT ?",
        )
        .bind(task_id)
        .bind(limit.clamp(1, FIRES_MAX))
        .fetch_all(&self.db)
        .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn sched() -> (Scheduler, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}?mode=rwc", dir.path().join("t.db").display());
        let pool = SqlitePool::connect(&url).await.unwrap();
        MIGRATOR.run(&pool).await.unwrap();
        (Scheduler::new(pool), dir)
    }

    fn input(s: Schedule) -> TaskInput {
        TaskInput {
            name: "日报".into(),
            prompt: "总结今天的新闻".into(),
            schedule: s,
        }
    }

    #[tokio::test]
    async fn validates_input() {
        let (s, _d) = sched().await;
        let bad = |name: &str, prompt: &str, sch| TaskInput {
            name: name.into(),
            prompt: prompt.into(),
            schedule: sch,
        };
        let ok = Schedule::Interval { every_secs: 60 };
        assert!(matches!(
            s.create(bad(" ", "x", ok.clone())).await,
            Err(ScheduleError::BadName)
        ));
        assert!(matches!(
            s.create(bad("a", " ", ok)).await,
            Err(ScheduleError::BadPrompt)
        ));
        assert!(matches!(
            s.create(input(Schedule::Interval { every_secs: 59 })).await,
            Err(ScheduleError::BadInterval)
        ));
    }

    #[tokio::test]
    async fn interval_task_is_claimed_once_per_period() {
        let (s, _d) = sched().await;
        let t = s
            .create(input(Schedule::Interval { every_secs: 60 }))
            .await
            .unwrap();
        let first = t.next_run_at.unwrap();
        assert!(s.claim_due(first - 1).await.unwrap().is_empty());
        let got = s.claim_due(first).await.unwrap();
        assert_eq!(got.len(), 1);
        // 刚认领过，不会重复；下次时间在现在之后。
        assert!(s.claim_due(first).await.unwrap().is_empty());
        let after = s.get(&t.id).await.unwrap();
        assert_eq!(after.next_run_at, Some(first + 60_000));
        assert_eq!(after.last_run_at, Some(first));
        // 错过很多个周期也只认领一次。
        assert_eq!(s.claim_due(first + 10_000_000).await.unwrap().len(), 1);
        assert!(s.claim_due(first + 10_000_000).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn once_task_runs_once_then_stops() {
        let (s, _d) = sched().await;
        let t = s
            .create(input(Schedule::Once { run_at: 1000 }))
            .await
            .unwrap();
        assert_eq!(s.claim_due(2000).await.unwrap().len(), 1);
        assert!(s.claim_due(i64::MAX).await.unwrap().is_empty());
        assert_eq!(s.get(&t.id).await.unwrap().next_run_at, None);
    }

    #[tokio::test]
    async fn disabled_task_is_skipped_and_reenable_restarts_interval() {
        let (s, _d) = sched().await;
        let t = s
            .create(input(Schedule::Interval { every_secs: 60 }))
            .await
            .unwrap();
        let off = s
            .update(
                &t.id,
                TaskPatch {
                    enabled: Some(false),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(!off.enabled);
        assert!(s.claim_due(i64::MAX).await.unwrap().is_empty());
        let on = s
            .update(
                &t.id,
                TaskPatch {
                    enabled: Some(true),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(on.enabled);
        assert!(on.next_run_at.unwrap() > t.created_at);
    }

    #[tokio::test]
    async fn update_reschedules_and_delete_cascades_fires() {
        let (s, _d) = sched().await;
        let t = s.create(input(Schedule::Once { run_at: 5 })).await.unwrap();
        let u = s
            .update(
                &t.id,
                TaskPatch {
                    name: Some("周报".into()),
                    schedule: Some(Schedule::Interval { every_secs: 3600 }),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(u.name, "周报");
        assert_eq!(u.schedule, Schedule::Interval { every_secs: 3600 });
        s.record_fire(&t.id, Some("c1"), None).await.unwrap();
        s.record_fire(&t.id, None, Some("boom")).await.unwrap();
        let fires = s.fires(&t.id, 10).await.unwrap();
        assert_eq!(fires.len(), 2);
        assert!(fires.iter().any(|f| f.status == "error"));
        s.delete(&t.id).await.unwrap();
        assert!(matches!(s.get(&t.id).await, Err(ScheduleError::NotFound)));
        assert!(matches!(
            s.delete(&t.id).await,
            Err(ScheduleError::NotFound)
        ));
        // 任务已删，记录触发不报错也不写入。
        s.record_fire(&t.id, None, None).await.unwrap();
    }

    #[tokio::test]
    async fn tick_emits_task_due() {
        let (s, _d) = sched().await;
        let mut rx = s.subscribe();
        s.create(input(Schedule::Once { run_at: 1 })).await.unwrap();
        assert_eq!(s.tick().await.unwrap(), 1);
        assert_eq!(rx.try_recv().unwrap().task.name, "日报");
        assert_eq!(s.tick().await.unwrap(), 0);
    }
}
