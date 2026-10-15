-- 模型供应商连接：一行一个，Key 用主密钥加密（决策 008）。
CREATE TABLE settings_credential (
    id             TEXT PRIMARY KEY,
    kind           TEXT NOT NULL CHECK (kind IN (
                       'openai-compatible', 'anthropic-compatible',
                       'openai', 'anthropic', 'google', 'deepseek', 'openrouter')),
    label          TEXT NOT NULL,
    base_url       TEXT,
    key_ciphertext BLOB NOT NULL,
    key_hint       TEXT NOT NULL,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL
) STRICT;

-- 偏好：键值对，值为 JSON。
CREATE TABLE settings_preference (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;
