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

/// RFC 6238 TOTP：HMAC-SHA1、6 位、30 秒一步。
pub mod totp {
    use hmac::{Hmac, KeyInit, Mac};
    use sha1::Sha1;

    pub const STEP_MS: i64 = 30_000;
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

    /// 160 位随机密钥。
    pub fn generate_secret() -> Vec<u8> {
        let mut bytes = vec![0u8; 20];
        getrandom::fill(&mut bytes).expect("系统随机数不可用");
        bytes
    }

    /// RFC 4648 base32，无填充（验证器 App 用的格式）。
    pub fn base32(bytes: &[u8]) -> String {
        let mut out = String::new();
        let (mut buf, mut bits) = (0u32, 0u32);
        for &b in bytes {
            buf = (buf << 8) | u32::from(b);
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                out.push(ALPHABET[((buf >> bits) & 31) as usize] as char);
            }
        }
        if bits > 0 {
            out.push(ALPHABET[((buf << (5 - bits)) & 31) as usize] as char);
        }
        out
    }

    pub fn from_base32(text: &str) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        let (mut buf, mut bits) = (0u32, 0u32);
        for c in text.trim_end_matches('=').bytes() {
            let v = ALPHABET.iter().position(|&a| a == c.to_ascii_uppercase())? as u32;
            buf = (buf << 5) | v;
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                out.push((buf >> bits) as u8);
                buf &= (1 << bits) - 1;
            }
        }
        Some(out)
    }

    /// 第 step 步的 6 位验证码。
    pub fn code(secret: &[u8], step: i64) -> String {
        let mut mac = Hmac::<Sha1>::new_from_slice(secret).expect("HMAC 接受任意长度密钥");
        mac.update(&step.to_be_bytes());
        let digest = mac.finalize().into_bytes();
        let offset = (digest[19] & 0x0f) as usize;
        let bin = u32::from_be_bytes([
            digest[offset] & 0x7f,
            digest[offset + 1],
            digest[offset + 2],
            digest[offset + 3],
        ]);
        format!("{:06}", bin % 1_000_000)
    }

    /// 校验验证码（允许 ±1 步），返回匹配的步数。
    pub fn verify(secret: &[u8], input: &str, now_ms: i64) -> Option<i64> {
        let input: String = input.chars().filter(|c| !c.is_whitespace()).collect();
        if input.len() != 6 || !input.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let now = now_ms.div_euclid(STEP_MS);
        (now - 1..=now + 1).find(|&s| code(secret, s) == input)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn rfc6238_sha1_vectors() {
            // RFC 6238 附录 B 的 SHA1 用例（取后 6 位）。
            let secret = b"12345678901234567890";
            assert_eq!(code(secret, 59 / 30), "287082");
            assert_eq!(code(secret, 1_111_111_109 / 30), "081804");
            assert_eq!(code(secret, 2_000_000_000 / 30), "279037");
        }

        #[test]
        fn base32_matches_rfc4648() {
            assert_eq!(base32(b"foobar"), "MZXW6YTBOI");
            assert_eq!(from_base32("MZXW6YTBOI").unwrap(), b"foobar");
        }

        #[test]
        fn verify_allows_one_step_of_drift() {
            let s = b"12345678901234567890";
            let now = 1_111_111_109_000;
            let prev = code(s, now / STEP_MS - 1);
            assert!(verify(s, &prev, now).is_some());
            let far = code(s, now / STEP_MS - 2);
            assert!(verify(s, &far, now).is_none());
            assert!(verify(s, "abc", now).is_none());
        }
    }
}
