-- 登录限速（票 12）：连续失败次数与锁定截止时间，存库以免重启清零。
ALTER TABLE auth_owner ADD COLUMN failed_logins INTEGER NOT NULL DEFAULT 0;
ALTER TABLE auth_owner ADD COLUMN locked_until INTEGER;
