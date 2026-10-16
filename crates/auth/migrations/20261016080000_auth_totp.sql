-- TOTP 两步验证（票 11）。totp_secret（已有列）与 totp_pending 存主密钥加密后的 base64。
-- totp_pending：开启流程中、尚未用验证码确认的密钥。totp_last_step：最近一次用过的步数，防重放。
ALTER TABLE auth_owner ADD COLUMN totp_pending TEXT;
ALTER TABLE auth_owner ADD COLUMN totp_enabled_at INTEGER;
ALTER TABLE auth_owner ADD COLUMN totp_last_step INTEGER;
