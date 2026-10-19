-- 定时任务：到点把 prompt 发给 agent（新开一个会话）。next_run_at 为空表示不会再触发。
CREATE TABLE sched_task (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    prompt        TEXT NOT NULL,
    kind          TEXT NOT NULL CHECK (kind IN ('interval', 'once')),
    interval_secs INTEGER,
    next_run_at   INTEGER,
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_at    INTEGER NOT NULL,
    last_run_at   INTEGER
) STRICT;

CREATE INDEX sched_task_due ON sched_task (next_run_at) WHERE enabled = 1;

-- 触发记录。
CREATE TABLE sched_fire (
    id              TEXT PRIMARY KEY,
    task_id         TEXT NOT NULL REFERENCES sched_task (id) ON DELETE CASCADE,
    fired_at        INTEGER NOT NULL,
    conversation_id TEXT,
    status          TEXT NOT NULL CHECK (status IN ('started', 'error')),
    error           TEXT
) STRICT;

CREATE INDEX sched_fire_task ON sched_fire (task_id, fired_at DESC);
