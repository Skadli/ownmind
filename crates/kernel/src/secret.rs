//! 主密钥：加密存库的密钥类数据（API Key、TOTP 密钥），决策 008。

use std::path::Path;

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("主密钥文件 {0} 长度不对（应为 32 字节）")]
    BadKeyFile(String),
    #[error("解密失败：数据损坏或主密钥不匹配")]
    Decrypt,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// 部署时生成、存在数据目录里的 32 字节密钥。Clone 便宜（只拷贝密钥字节）。
#[derive(Clone)]
pub struct MasterKey([u8; KEY_LEN]);

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MasterKey(..)")
    }
}

impl MasterKey {
    /// 读取主密钥；文件不存在时生成（权限 600）。
    pub fn load_or_create(path: &Path) -> Result<Self, SecretError> {
        match std::fs::read(path) {
            Ok(bytes) => {
                let key: [u8; KEY_LEN] = bytes
                    .try_into()
                    .map_err(|_| SecretError::BadKeyFile(path.display().to_string()))?;
                Ok(Self(key))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let key = Self::random();
                write_private(path, &key.0)?;
                Ok(key)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// 随机密钥（测试用，或首次生成）。
    pub fn random() -> Self {
        let mut key = [0u8; KEY_LEN];
        getrandom::fill(&mut key).expect("系统随机数不可用");
        Self(key)
    }

    /// 加密：输出 = 24 字节随机 nonce ‖ 密文。
    pub fn encrypt(&self, plaintext: &[u8]) -> Vec<u8> {
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce).expect("系统随机数不可用");
        let ciphertext = self
            .cipher()
            .encrypt(&XNonce::from(nonce), plaintext)
            .expect("XChaCha20-Poly1305 加密不会失败");
        [nonce.as_slice(), &ciphertext].concat()
    }

    pub fn decrypt(&self, sealed: &[u8]) -> Result<Vec<u8>, SecretError> {
        if sealed.len() < NONCE_LEN {
            return Err(SecretError::Decrypt);
        }
        let (nonce, ciphertext) = sealed.split_at(NONCE_LEN);
        self.cipher()
            .decrypt(
                &XNonce::try_from(nonce).map_err(|_| SecretError::Decrypt)?,
                ciphertext,
            )
            .map_err(|_| SecretError::Decrypt)
    }

    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new_from_slice(&self.0).expect("密钥长度固定为 32")
    }
}

/// 主服务与沙盒之间的共享令牌（决策 003）：读取文件；不存在时生成 32 字节随机数的十六进制（权限 600）。
pub fn load_or_create_token(path: &Path) -> std::io::Result<String> {
    match std::fs::read_to_string(path) {
        Ok(s) if !s.trim().is_empty() => Ok(s.trim().to_owned()),
        Ok(_) => Err(std::io::Error::other(format!(
            "令牌文件 {} 为空",
            path.display()
        ))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let mut raw = [0u8; 32];
            getrandom::fill(&mut raw).expect("系统随机数不可用");
            let token: String = raw.iter().map(|b| format!("{b:02x}")).collect();
            write_private(path, token.as_bytes())?;
            Ok(token)
        }
        Err(e) => Err(e),
    }
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(bytes)
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_created_once_and_reused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret/sandbox.token");
        let a = load_or_create_token(&path).unwrap();
        assert_eq!(a.len(), 64);
        assert_eq!(load_or_create_token(&path).unwrap(), a);
    }

    #[test]
    fn round_trip_and_tamper_detection() {
        let key = MasterKey::random();
        let sealed = key.encrypt(b"sk-test");
        assert_ne!(&sealed[NONCE_LEN..], b"sk-test");
        assert_eq!(key.decrypt(&sealed).unwrap(), b"sk-test");
        let mut bad = sealed.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(key.decrypt(&bad).is_err());
        assert!(MasterKey::random().decrypt(&sealed).is_err());
    }

    #[test]
    fn key_file_is_created_once_with_private_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("master.key");
        let a = MasterKey::load_or_create(&path).unwrap();
        let b = MasterKey::load_or_create(&path).unwrap();
        assert_eq!(a.0, b.0);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }
}
