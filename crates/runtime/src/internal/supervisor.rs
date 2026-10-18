//! agent 子进程：拉起、握手、NDJSON JSON-RPC 2.0 双向通信、崩溃后重启（决策 002）。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use ownmind_conversation::api::Conversations;
use ownmind_settings::api::Settings;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, mpsc, oneshot, watch};

use crate::api::{
    AgentEvent, ContextBuildParams, ContextCompactParams, InitializeParams, InitializeResult,
    ModelSpec, PROTOCOL_VERSION, Run, RunCancelParams, RunEventParams, RunStartParams,
    RuntimeError, Tool, ToolCallParams, ToolContext, ToolOutput,
};

const READY_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

type Pending = HashMap<u64, oneshot::Sender<Result<Value, String>>>;

pub(crate) struct Supervisor {
    conversations: Conversations,
    settings: Settings,
    tools: Vec<Arc<dyn Tool>>,
    /// 每个运行的中止信号，工具执行时监听。
    cancels: Mutex<HashMap<String, watch::Sender<bool>>>,
    next_id: AtomicU64,
    pending: Mutex<Pending>,
    runs: Mutex<HashMap<String, mpsc::UnboundedSender<AgentEvent>>>,
    /// 当前子进程的写端；None 表示没有可用的 agent。
    stdin: Mutex<Option<mpsc::UnboundedSender<String>>>,
    /// 握手成功后为 true。
    ready: watch::Sender<bool>,
}

impl Supervisor {
    pub(crate) fn spawn(
        cmd: Vec<String>,
        conversations: Conversations,
        settings: Settings,
        tools: Vec<Arc<dyn Tool>>,
    ) -> Arc<Self> {
        assert!(!cmd.is_empty(), "agent 命令不能为空");
        let sup = Arc::new(Self {
            conversations,
            settings,
            tools,
            cancels: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            runs: Mutex::new(HashMap::new()),
            stdin: Mutex::new(None),
            ready: watch::channel(false).0,
        });
        tokio::spawn(supervise(Arc::downgrade(&sup), cmd));
        sup
    }

    pub(crate) async fn start_run(
        &self,
        conversation_id: &str,
        model: ModelSpec,
    ) -> Result<Run, RuntimeError> {
        let run_id = ownmind_kernel::new_id();
        let (tx, rx) = mpsc::unbounded_channel();
        self.runs.lock().await.insert(run_id.clone(), tx);
        self.cancels
            .lock()
            .await
            .insert(run_id.clone(), watch::channel(false).0);
        let params = RunStartParams {
            run_id: run_id.clone(),
            conversation_id: conversation_id.to_owned(),
            model,
        };
        if let Err(e) = self.request("run.start", to_value(&params)).await {
            self.runs.lock().await.remove(&run_id);
            self.cancels.lock().await.remove(&run_id);
            return Err(e);
        }
        Ok(Run {
            id: run_id,
            events: rx,
        })
    }

    pub(crate) async fn cancel(&self, run_id: &str) -> Result<(), RuntimeError> {
        if let Some(c) = self.cancels.lock().await.get(run_id) {
            c.send_replace(true);
        }
        let params = RunCancelParams {
            run_id: run_id.to_owned(),
        };
        self.request("run.cancel", to_value(&params)).await?;
        Ok(())
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, RuntimeError> {
        let mut ready = self.ready.subscribe();
        tokio::time::timeout(READY_TIMEOUT, ready.wait_for(|r| *r))
            .await
            .map_err(|_| RuntimeError::AgentUnavailable("agent 未就绪".into()))?
            .map_err(|_| RuntimeError::AgentUnavailable("运行时已关闭".into()))?;
        self.call(method, params).await
    }

    /// 发请求不等就绪（握手本身用）。
    async fn call(&self, method: &str, params: Value) -> Result<Value, RuntimeError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let line = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.send(line).await?;
        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(msg))) => Err(RuntimeError::Agent(msg)),
            Ok(Err(_)) => Err(RuntimeError::AgentUnavailable("agent 已退出".into())),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(RuntimeError::AgentUnavailable("agent 响应超时".into()))
            }
        }
    }

    async fn send(&self, msg: Value) -> Result<(), RuntimeError> {
        let stdin = self.stdin.lock().await;
        let tx = stdin
            .as_ref()
            .ok_or_else(|| RuntimeError::AgentUnavailable("agent 未运行".into()))?;
        tx.send(msg.to_string())
            .map_err(|_| RuntimeError::AgentUnavailable("agent 已退出".into()))
    }

    /// 处理 agent 发来的一行。
    async fn on_line(self: &Arc<Self>, line: &str) {
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "agent 输出了非 JSON 行");
                return;
            }
        };
        let method = msg.get("method").and_then(Value::as_str);
        let id = msg.get("id").cloned().filter(|v| !v.is_null());
        match (method, id) {
            (Some(method), Some(id)) => {
                let sup = self.clone();
                let method = method.to_owned();
                let params = msg.get("params").cloned().unwrap_or(Value::Null);
                tokio::spawn(async move { sup.answer(id, &method, params).await });
            }
            (Some("run.event"), None) => self.on_run_event(msg.get("params")).await,
            (Some(other), None) => tracing::warn!(method = other, "未知的 agent 通知"),
            (None, Some(id)) => {
                let Some(id) = id.as_u64() else { return };
                let Some(tx) = self.pending.lock().await.remove(&id) else {
                    return;
                };
                let result = match msg.get("error") {
                    Some(err) => Err(err
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("未知错误")
                        .to_owned()),
                    None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
                };
                let _ = tx.send(result);
            }
            (None, None) => tracing::warn!("agent 消息既无 method 也无 id"),
        }
    }

    async fn on_run_event(&self, params: Option<&Value>) {
        let Some(Ok(p)) = params.map(|p| serde_json::from_value::<RunEventParams>(p.clone()))
        else {
            tracing::warn!("run.event 参数不合法");
            return;
        };
        let mut runs = self.runs.lock().await;
        let finished = p.event.is_final();
        if let Some(tx) = runs.get(&p.run_id) {
            let _ = tx.send(p.event);
        }
        if finished {
            runs.remove(&p.run_id);
            if let Some(c) = self.cancels.lock().await.remove(&p.run_id) {
                c.send_replace(true);
            }
        }
    }

    /// 执行工具；运行被中止时立即返回错误结果（工具的 future 随之丢弃）。
    async fn call_tool(&self, p: ToolCallParams) -> ToolOutput {
        let Some(tool) = self.tools.iter().find(|t| t.spec().name == p.name) else {
            return ToolOutput::error(format!("没有这个工具：{}", p.name));
        };
        let cancel = self
            .cancels
            .lock()
            .await
            .get(&p.run_id)
            .map(|c| c.subscribe());
        let Some(mut cancel) = cancel else {
            return ToolOutput::error("运行已结束");
        };
        let ctx = ToolContext {
            run_id: p.run_id,
            conversation_id: p.conversation_id,
        };
        tokio::select! {
            out = tool.call(ctx, p.arguments) => out,
            _ = cancel.wait_for(|c| *c) => ToolOutput::error("已中止"),
        }
    }

    /// 回答 agent 发来的请求（反向 RPC）。
    async fn answer(&self, id: Value, method: &str, params: Value) {
        let result = match method {
            "context.build" => match serde_json::from_value::<ContextBuildParams>(params) {
                Ok(p) => super::context::build(&self.conversations, &self.settings, &p)
                    .await
                    .map(|mut r| {
                        r.tools = self.tools.iter().map(|t| t.spec()).collect();
                        to_value(&r)
                    }),
                Err(e) => Err(format!("参数不合法：{e}")),
            },
            "context.compact" => match serde_json::from_value::<ContextCompactParams>(params) {
                Ok(p) => self
                    .conversations
                    .add_compaction(&p.conversation_id, p.upto_seq, &p.summary)
                    .await
                    .map(|()| serde_json::json!({}))
                    .map_err(|e| e.to_string()),
                Err(e) => Err(format!("参数不合法：{e}")),
            },
            "tool.call" => match serde_json::from_value::<ToolCallParams>(params) {
                Ok(p) => Ok(to_value(&self.call_tool(p).await)),
                Err(e) => Err(format!("参数不合法：{e}")),
            },
            other => Err(format!("未知方法：{other}")),
        };
        let reply = match result {
            Ok(v) => json!({ "jsonrpc": "2.0", "id": id, "result": v }),
            Err(m) => {
                json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32000, "message": m } })
            }
        };
        if let Err(e) = self.send(reply).await {
            tracing::warn!(error = %e, "回复 agent 失败");
        }
    }

    /// 子进程退出：挂起的请求和进行中的运行全部以错误结束。
    async fn on_exit(&self, reason: &str) {
        self.ready.send_replace(false);
        *self.stdin.lock().await = None;
        self.pending.lock().await.clear();
        for (_, c) in self.cancels.lock().await.drain() {
            c.send_replace(true);
        }
        for (_, tx) in self.runs.lock().await.drain() {
            let _ = tx.send(AgentEvent::Error {
                message: format!("agent 意外退出：{reason}"),
            });
        }
    }
}

fn to_value<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).expect("协议类型可序列化")
}

/// 拉起 → 握手 → 读到退出 → 退避后重启。Supervisor 被释放后结束。
async fn supervise(weak: std::sync::Weak<Supervisor>, cmd: Vec<String>) {
    let mut backoff = Duration::from_millis(200);
    loop {
        let Some(sup) = weak.upgrade() else { return };
        let started = tokio::time::Instant::now();
        let reason = match run_once(&sup, &cmd).await {
            Ok(status) => format!("退出状态 {status}"),
            Err(e) => e,
        };
        tracing::error!(%reason, "agent 子进程结束，准备重启");
        sup.on_exit(&reason).await;
        drop(sup);
        if started.elapsed() > Duration::from_secs(60) {
            backoff = Duration::from_millis(200);
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

async fn run_once(sup: &Arc<Supervisor>, cmd: &[String]) -> Result<String, String> {
    let mut child: Child = Command::new(&cmd[0])
        .args(&cmd[1..])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("无法启动 {}：{e}", cmd[0]))?;
    let mut stdin = child.stdin.take().expect("已设为 piped");
    let stdout = child.stdout.take().expect("已设为 piped");

    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let writer = tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            if stdin.write_all(line.as_bytes()).await.is_err()
                || stdin.write_all(b"\n").await.is_err()
                || stdin.flush().await.is_err()
            {
                break;
            }
        }
    });
    *sup.stdin.lock().await = Some(tx);

    let reader_sup = sup.clone();
    let mut reader = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            reader_sup.on_line(&line).await;
        }
    });

    let handshake = sup.call(
        "initialize",
        to_value(&InitializeParams {
            protocol_version: PROTOCOL_VERSION,
        }),
    );
    let outcome = tokio::select! {
        r = handshake => match r.map(serde_json::from_value::<InitializeResult>) {
            Ok(Ok(init)) if init.protocol_version == PROTOCOL_VERSION => {
                tracing::info!(agent_version = %init.agent_version, "agent 已就绪");
                sup.ready.send_replace(true);
                None
            }
            Ok(Ok(init)) => Some(format!(
                "协议版本不符：server {PROTOCOL_VERSION}，agent {}",
                init.protocol_version
            )),
            Ok(Err(e)) => Some(format!("握手结果不合法：{e}")),
            Err(e) => Some(format!("握手失败：{e}")),
        },
        _ = &mut reader => Some("握手前 agent 关闭了输出".into()),
    };
    if let Some(reason) = outcome {
        let _ = child.kill().await;
        writer.abort();
        reader.abort();
        return Err(reason);
    }

    let status = tokio::select! {
        s = child.wait() => s.map_err(|e| e.to_string())?.to_string(),
        _ = &mut reader => {
            let _ = child.kill().await;
            "agent 关闭了输出".to_owned()
        }
    };
    writer.abort();
    reader.abort();
    Ok(status)
}
