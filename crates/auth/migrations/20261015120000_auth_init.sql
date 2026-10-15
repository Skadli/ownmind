-- 主人：每个部署只有一个，首次打开管理页面时由主人自己设密码（仓库内无默认账号）。
CREATE TABLE auth_owner (
    id            TEXT PRIMARY KEY,
    is_singleton  INTEGER NOT NULL DEFAULT 1 UNIQUE CHECK (is_singleton = 1),
    password_hash TEXT NOT NULL,
    totp_secret   TEXT,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
) STRICT;

-- 登录会话：只存令牌的哈希，Cookie 里是原始令牌。
CREATE TABLE auth_login_session (
    id           TEXT PRIMARY KEY,
    token_hash   TEXT NOT NULL UNIQUE,
    user_agent   TEXT,
    created_at   INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    expires_at   INTEGER NOT NULL
) STRICT;

CREATE INDEX auth_login_session_expires_at ON auth_login_session (expires_at);
