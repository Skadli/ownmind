//! 实现细节，不对外公开。

pub mod password {
    use argon2::Argon2;
    use argon2::password_hash::phc::PasswordHash;
    use argon2::password_hash::{PasswordHasher, PasswordVerifier};

    pub fn hash(password: &str) -> String {
        Argon2::default()
            .hash_password(password.as_bytes())
            .expect("argon2 哈希失败")
            .to_string()
    }

    pub fn verify(password: &str, stored: &str) -> bool {
        let parsed = PasswordHash::new(stored).expect("库中的密码哈希格式损坏");
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    }
}

pub mod token {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use sha2::{Digest, Sha256};

    /// 256 位随机令牌，base64url 编码。
    pub fn generate() -> String {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).expect("系统随机数不可用");
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// 库里只存令牌的 SHA-256。
    pub fn hash(raw: &str) -> String {
        URL_SAFE_NO_PAD.encode(Sha256::digest(raw.as_bytes()))
    }
}
