-- 消息：会话内按 seq 排序。body 是 pi 的消息 JSON（原样回放给模型），text 是纯文本（显示与估算 token）。
-- 外键只在本模块内部（硬规则 2 只禁止跨模块外键）。
CREATE TABLE conv_message (
    id              TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conv_conversation (id) ON DELETE CASCADE,
    seq             INTEGER NOT NULL,
    role            TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
    body            TEXT NOT NULL,
    text            TEXT NOT NULL,
    input_tokens    INTEGER,
    output_tokens   INTEGER,
    created_at      INTEGER NOT NULL,
    UNIQUE (conversation_id, seq)
) STRICT;
