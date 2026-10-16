-- 压缩摘要（决策 005）：upto_seq 及之前的消息由 summary 代替发给模型；界面仍显示全部原文。
CREATE TABLE conv_compaction (
    id              TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conv_conversation (id) ON DELETE CASCADE,
    upto_seq        INTEGER NOT NULL,
    summary         TEXT NOT NULL,
    created_at      INTEGER NOT NULL
) STRICT;

CREATE INDEX conv_compaction_by_conversation ON conv_compaction (conversation_id, upto_seq);
