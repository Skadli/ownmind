//! 共享内核：ID、时间、主密钥加密，保持极小。

mod secret;

pub use secret::{MasterKey, SecretError, load_or_create_token};

/// 新建一个 ID：UUIDv7 文本（决策 007）。按生成时间有序。
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// 当前时间：UTC Unix 毫秒（决策 007）。
pub fn now_ms() -> i64 {
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("系统时钟早于 1970 年");
    i64::try_from(since.as_millis()).expect("时间超出 i64")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_uuid_v7_and_time_ordered() {
        let a = new_id();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = new_id();
        assert_eq!(uuid::Uuid::parse_str(&a).unwrap().get_version_num(), 7);
        assert!(a < b);
    }
}
