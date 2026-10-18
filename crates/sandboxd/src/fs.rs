//! POST /read、/write、/edit：路径一律相对工作目录，拒绝 `..` 与符号链接逃逸。

use std::path::{Component, Path, PathBuf};

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};

use crate::{Error, Sandbox};

/// /read 最多返回的字节数。
pub const MAX_READ: u64 = 1024 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadRequest {
    pub path: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadResponse {
    pub content: String,
    /// 文件总字节数。
    pub size: u64,
    pub truncated: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteRequest {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteResponse {
    pub bytes: u64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditRequest {
    pub path: String,
    pub old_string: String,
    pub new_string: String,
}

/// 把相对路径解析成工作目录内的绝对路径。
///
/// 拒绝绝对路径和 `..`；再把最深的已存在祖先规范化（解析符号链接），确认仍在 `root` 下。
/// `root` 须已规范化。
pub(crate) fn resolve(root: &Path, rel: &str) -> Result<PathBuf, Error> {
    let rel_path = Path::new(rel);
    let mut joined = root.to_path_buf();
    for c in rel_path.components() {
        match c {
            Component::Normal(part) => joined.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(Error::Escape);
            }
        }
    }
    let mut existing = joined.as_path();
    loop {
        match existing.symlink_metadata() {
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                existing = existing.parent().ok_or(Error::Escape)?;
            }
            Err(e) => return Err(e.into()),
        }
    }
    // 悬空符号链接 canonicalize 会失败：一律当越界处理。
    let real = existing.canonicalize().map_err(|_| Error::Escape)?;
    if !real.starts_with(root) {
        return Err(Error::Escape);
    }
    let rest = joined
        .strip_prefix(existing)
        .expect("existing 是 joined 的祖先");
    // join("") 会追加结尾斜杠，路径本身已存在时直接返回。
    if rest.as_os_str().is_empty() {
        return Ok(real);
    }
    Ok(real.join(rest))
}

fn not_found(rel: &str, e: std::io::Error) -> Error {
    if e.kind() == std::io::ErrorKind::NotFound {
        Error::NotFound(rel.to_owned())
    } else {
        e.into()
    }
}

pub(crate) async fn read(
    State(sb): State<Sandbox>,
    Json(req): Json<ReadRequest>,
) -> Result<Json<ReadResponse>, Error> {
    use tokio::io::AsyncReadExt;
    let path = resolve(sb.root(), &req.path)?;
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| not_found(&req.path, e))?;
    let meta = file.metadata().await?;
    let size = meta.len();
    if !meta.is_file() {
        return Err(Error::BadRequest(format!("不是文件：{}", req.path)));
    }
    let mut bytes = Vec::new();
    file.take(MAX_READ).read_to_end(&mut bytes).await?;
    Ok(Json(ReadResponse {
        content: String::from_utf8_lossy(&bytes).into_owned(),
        size,
        truncated: size > MAX_READ,
    }))
}

pub(crate) async fn write(
    State(sb): State<Sandbox>,
    Json(req): Json<WriteRequest>,
) -> Result<Json<WriteResponse>, Error> {
    let path = resolve(sb.root(), &req.path)?;
    if path == sb.root() {
        return Err(Error::BadRequest("path 不能为空".into()));
    }
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir).await?;
        // 建目录期间可能被并发换成符号链接：建完再确认一次。
        resolve(sb.root(), &req.path)?;
    }
    tokio::fs::write(&path, req.content.as_bytes()).await?;
    Ok(Json(WriteResponse {
        bytes: req.content.len() as u64,
    }))
}

pub(crate) async fn edit(
    State(sb): State<Sandbox>,
    Json(req): Json<EditRequest>,
) -> Result<Json<WriteResponse>, Error> {
    if req.old_string.is_empty() {
        return Err(Error::BadRequest("oldString 不能为空".into()));
    }
    let path = resolve(sb.root(), &req.path)?;
    let text = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| not_found(&req.path, e))?;
    let count = text.matches(&req.old_string).count();
    if count != 1 {
        return Err(Error::BadRequest(format!(
            "oldString 必须恰好出现一次（实际 {count} 次）"
        )));
    }
    let updated = text.replacen(&req.old_string, &req.new_string, 1);
    tokio::fs::write(&path, updated.as_bytes()).await?;
    Ok(Json(WriteResponse {
        bytes: updated.len() as u64,
    }))
}
