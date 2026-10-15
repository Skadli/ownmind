//! 内部实现。

/// 页面上显示的 Key 提示：末 4 位。
pub(crate) fn key_hint(key: &str) -> String {
    let tail: String = key
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("…{tail}")
}

pub(crate) fn valid_base_url(url: &str) -> bool {
    (url.starts_with("http://") || url.starts_with("https://")) && url.len() > "https://".len()
}
