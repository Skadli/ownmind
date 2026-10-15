-- 会话：主人与 agent 的一条对话线程。消息、运行、压缩摘要的表随各自的票加入。
CREATE TABLE conv_conversation (
    id         TEXT PRIMARY KEY,
    title      TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE INDEX conv_conversation_updated_at ON conv_conversation (updated_at DESC);
