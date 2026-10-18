-- 工具结果消息（票 16）：role 增加 'tool'。SQLite 不能改 CHECK，只能重建表（本表无人引用）。
CREATE TABLE conv_message__rebuild (
    id              TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conv_conversation (id) ON DELETE CASCADE,
    seq             INTEGER NOT NULL,
    role            TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'tool')),
    body            TEXT NOT NULL,
    text            TEXT NOT NULL,
    input_tokens    INTEGER,
    output_tokens   INTEGER,
    created_at      INTEGER NOT NULL,
    UNIQUE (conversation_id, seq)
) STRICT;

INSERT INTO conv_message__rebuild SELECT * FROM conv_message;
DROP TABLE conv_message;
ALTER TABLE conv_message__rebuild RENAME TO conv_message;
