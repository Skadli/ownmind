//! POST /exec：在工作目录里用 `sh -c` 跑命令；超时杀整个进程组，输出各限 1 MB。

use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

use crate::{Error, Sandbox};

/// 默认超时（秒）。
pub const DEFAULT_TIMEOUT_SECS: u64 = 60;
/// 超时上限（秒）。
pub const MAX_TIMEOUT_SECS: u64 = 600;
/// stdout、stderr 各自最多保留的字节数。
pub const MAX_OUTPUT: usize = 1024 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecRequest {
    pub command: String,
    /// 相对工作目录的子目录；缺省为工作目录。
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecResponse {
    /// 正常退出时的退出码；被信号杀死（含超时）时为 null。
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub timed_out: bool,
    pub duration_ms: u64,
}

pub(crate) async fn handle(
    State(sb): State<Sandbox>,
    Json(req): Json<ExecRequest>,
) -> Result<Json<ExecResponse>, Error> {
    if req.command.trim().is_empty() {
        return Err(Error::BadRequest("command 不能为空".into()));
    }
    let timeout = Duration::from_secs(
        req.timeout_secs
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .clamp(1, MAX_TIMEOUT_SECS),
    );
    let cwd = match req.cwd.as_deref() {
        None | Some("") => sb.root().to_path_buf(),
        Some(dir) => {
            let p = crate::fs::resolve(sb.root(), dir)?;
            if !p.is_dir() {
                return Err(Error::BadRequest(format!("cwd 不是目录：{dir}")));
            }
            p
        }
    };
    Ok(Json(run(&req.command, &cwd, timeout).await?))
}

async fn run(
    command: &str,
    cwd: &std::path::Path,
    timeout: Duration,
) -> Result<ExecResponse, Error> {
    let started = Instant::now();
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .kill_on_drop(true)
        .spawn()?;
    let pgid = child.id().and_then(|id| i32::try_from(id).ok());
    let out_buf = Captured::default();
    let err_buf = Captured::default();
    let out = tokio::spawn(read_capped(
        child.stdout.take().expect("piped"),
        out_buf.clone(),
    ));
    let err = tokio::spawn(read_capped(
        child.stderr.take().expect("piped"),
        err_buf.clone(),
    ));

    let (status, timed_out) = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(status) => (Some(status?), false),
        Err(_) => {
            kill_group(pgid);
            let _ = child.wait().await;
            (None, true)
        }
    };
    // 后台子进程可能还握着管道：命令结束后再给 2 秒收尾，然后杀掉整组。
    let grace = Duration::from_secs(2);
    let ((stdout, stdout_truncated), (stderr, stderr_truncated)) = tokio::join!(
        join_output(out, &out_buf, grace, pgid),
        join_output(err, &err_buf, grace, pgid),
    );
    Ok(ExecResponse {
        exit_code: status.and_then(|s| s.code()),
        stdout,
        stderr,
        stdout_truncated,
        stderr_truncated,
        timed_out,
        duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

/// 读取任务写入的共享缓冲：任务被放弃时也能拿到已读部分。
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<(Vec<u8>, bool)>>);

impl Captured {
    fn take(&self) -> (String, bool) {
        let mut g = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (bytes, truncated) = std::mem::take(&mut *g);
        (String::from_utf8_lossy(&bytes).into_owned(), truncated)
    }
}

/// 等读取任务读到 EOF。超过收尾时间就杀进程组；用 setsid 等脱离进程组的后代仍握着管道时，
/// 再等 500 毫秒后放弃读取（标为截断），保证请求一定返回。
async fn join_output(
    mut task: tokio::task::JoinHandle<()>,
    buf: &Captured,
    grace: Duration,
    pgid: Option<i32>,
) -> (String, bool) {
    let mut abandoned = false;
    if tokio::time::timeout(grace, &mut task).await.is_err() {
        kill_group(pgid);
        if tokio::time::timeout(Duration::from_millis(500), &mut task)
            .await
            .is_err()
        {
            task.abort();
            abandoned = true;
        }
    }
    let (text, truncated) = buf.take();
    (text, truncated || abandoned)
}

fn kill_group(pgid: Option<i32>) {
    if let Some(pgid) = pgid {
        let _ = nix::sys::signal::killpg(
            nix::unistd::Pid::from_raw(pgid),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
}

/// 读到 EOF；只保留前 MAX_OUTPUT 字节，其余读掉丢弃（避免写端阻塞）。
async fn read_capped(mut r: impl AsyncRead + Unpin, out: Captured) {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match r.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let mut g = out
                    .0
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let room = MAX_OUTPUT - g.0.len();
                if n > room {
                    g.1 = true;
                }
                g.0.extend_from_slice(&buf[..n.min(room)]);
            }
        }
    }
}
