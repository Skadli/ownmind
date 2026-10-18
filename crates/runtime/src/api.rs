//! 公共入口：agent 运行时与 server↔agent 协议（决策 002、008）。

use std::sync::Arc;

use ownmind_conversation::api::{ConversationError, Conversations, Message};
use ownmind_settings::api::{Settings, SettingsError};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};
use ts_rs::TS;

use crate::internal::{Chats, Supervisor, sandbox_tools, web_tools};

/// 协议版本：握手时双方必须一致。
pub const PROTOCOL_VERSION: u32 = 1;

// ---------- 协议类型（ts-rs 生成到 agent/src/protocol/generated） ----------

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../../agent/src/protocol/generated/protocol.ts"
)]
#[ts(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../../agent/src/protocol/generated/protocol.ts"
)]
#[ts(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: u32,
    pub agent_version: String,
}

/// 一次运行用的模型：供应商种类、地址、Key、模型 ID。
#[derive(Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../../agent/src/protocol/generated/protocol.ts"
)]
#[ts(rename_all = "camelCase")]
pub struct ModelSpec {
    /// settings 的 ProviderKind，如 openai、openai-compatible。
    pub kind: String,
    pub base_url: Option<String>,
    pub api_key: String,
    pub model_id: String,
}

impl std::fmt::Debug for ModelSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelSpec")
            .field("kind", &self.kind)
            .field("base_url", &self.base_url)
            .field("model_id", &self.model_id)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../../agent/src/protocol/generated/protocol.ts"
)]
#[ts(rename_all = "camelCase")]
pub struct RunStartParams {
    pub run_id: String,
    pub conversation_id: String,
    pub model: ModelSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../../agent/src/protocol/generated/protocol.ts"
)]
#[ts(rename_all = "camelCase")]
pub struct RunCancelParams {
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../../agent/src/protocol/generated/protocol.ts"
)]
#[ts(rename_all = "camelCase")]
pub struct ContextBuildParams {
    pub conversation_id: String,
    /// 当前模型的上下文窗口（token），用来判断是否需要压缩。
    pub context_window: u32,
}

/// 需要压缩时随上下文一起返回：agent 用当前模型总结后调 context.compact。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../../agent/src/protocol/generated/protocol.ts"
)]
#[ts(rename_all = "camelCase")]
pub struct CompactionRequest {
    /// 摘要覆盖到这条消息（含）。
    #[ts(type = "number")]
    pub upto_seq: i64,
    /// 上一次的摘要，新摘要要把它并进去。
    pub previous_summary: Option<String>,
    /// 待总结部分的纯文本对话记录。
    pub transcript: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../../agent/src/protocol/generated/protocol.ts"
)]
#[ts(rename_all = "camelCase")]
pub struct ContextCompactParams {
    pub conversation_id: String,
    #[ts(type = "number")]
    pub upto_seq: i64,
    pub summary: String,
}

/// 发给模型的上下文（决策 005 的分层组装结果）。messages 是 pi 消息 JSON。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../../agent/src/protocol/generated/protocol.ts"
)]
#[ts(rename_all = "camelCase")]
pub struct ContextBuildResult {
    pub system_prompt: String,
    #[ts(type = "unknown[]")]
    pub messages: Vec<serde_json::Value>,
    /// 估算的输入 token 数。
    #[ts(type = "number")]
    pub estimated_tokens: i64,
    /// 超过窗口 70% 且可压缩时给出。
    pub compaction: Option<CompactionRequest>,
    /// 本次可用的工具声明。
    pub tools: Vec<ToolSpec>,
}

/// 交给模型的工具声明。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../../agent/src/protocol/generated/protocol.ts"
)]
#[ts(rename_all = "camelCase")]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema（object）。
    #[ts(type = "Record<string, unknown>")]
    pub parameters: serde_json::Value,
}

/// agent→server：执行一次工具调用。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../../agent/src/protocol/generated/protocol.ts"
)]
#[ts(rename_all = "camelCase")]
pub struct ToolCallParams {
    pub run_id: String,
    pub conversation_id: String,
    pub name: String,
    #[ts(type = "Record<string, unknown>")]
    pub arguments: serde_json::Value,
}

/// 工具执行结果：给模型看的文本，以及是否出错。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../../agent/src/protocol/generated/protocol.ts"
)]
#[ts(rename_all = "camelCase")]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
        }
    }
}

/// 工具调用的上下文。
#[derive(Debug, Clone)]
pub struct ToolContext {
    pub run_id: String,
    pub conversation_id: String,
}

pub type ToolFuture = std::pin::Pin<Box<dyn std::future::Future<Output = ToolOutput> + Send>>;

/// 一个工具：由各模块实现，server 组装时注册给 Runtime。
pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    fn call(&self, ctx: ToolContext, arguments: serde_json::Value) -> ToolFuture;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../../agent/src/protocol/generated/protocol.ts"
)]
pub struct Usage {
    #[ts(type = "number")]
    pub input: i64,
    #[ts(type = "number")]
    pub output: i64,
}

/// agent 推送的运行事件。done / error 之后该运行结束。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../../agent/src/protocol/generated/protocol.ts"
)]
#[serde(rename_all_fields = "camelCase")]
#[ts(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum AgentEvent {
    TextDelta {
        delta: String,
    },
    ThinkingDelta {
        delta: String,
    },
    /// 中间一步：带工具调用的 assistant 消息，存库后继续。
    Step {
        #[ts(type = "unknown")]
        message: serde_json::Value,
        text: String,
        usage: Usage,
    },
    ToolStart {
        tool_call_id: String,
        name: String,
        #[ts(type = "Record<string, unknown>")]
        arguments: serde_json::Value,
    },
    /// 工具结果：pi 的 toolResult 消息，存库后继续。
    ToolEnd {
        tool_call_id: String,
        name: String,
        #[ts(type = "unknown")]
        message: serde_json::Value,
        text: String,
        is_error: bool,
    },
    Done {
        /// pi 的 AssistantMessage JSON，原样存库、原样回放。
        #[ts(type = "unknown")]
        message: serde_json::Value,
        text: String,
        usage: Usage,
    },
    Error {
        message: String,
    },
}

impl AgentEvent {
    pub fn is_final(&self) -> bool {
        matches!(self, Self::Done { .. } | Self::Error { .. })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../../agent/src/protocol/generated/protocol.ts"
)]
#[ts(rename_all = "camelCase")]
pub struct RunEventParams {
    pub run_id: String,
    pub event: AgentEvent,
}

// ---------- 运行时 ----------

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("agent 不可用：{0}")]
    AgentUnavailable(String),
    #[error("agent 返回错误：{0}")]
    Agent(String),
    #[error(transparent)]
    Settings(#[from] SettingsError),
    #[error(transparent)]
    Conversation(#[from] ConversationError),
    #[error("该会话已有进行中的回复")]
    Busy,
    #[error("消息不能为空")]
    EmptyMessage,
    #[error("该会话没有进行中的回复")]
    NotRunning,
}

/// 聊天流事件（给浏览器的 SSE）。
#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[serde(rename_all_fields = "camelCase")]
#[ts(
    export,
    export_to = "chat.ts",
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ChatEvent {
    /// 主人消息已保存。
    User {
        #[ts(type = "import(\"./conversation\").Message")]
        message: Message,
    },
    Delta {
        delta: String,
    },
    Thinking {
        delta: String,
    },
    /// 运行中间已保存的消息（带工具调用的回复、工具结果）。
    Saved {
        #[ts(type = "import(\"./conversation\").Message")]
        message: Message,
    },
    ToolStart {
        tool_call_id: String,
        name: String,
        #[ts(type = "Record<string, unknown>")]
        arguments: serde_json::Value,
    },
    /// 回复完成并已保存。
    Done {
        #[ts(type = "import(\"./conversation\").Message")]
        message: Message,
    },
    Error {
        message: String,
    },
}

impl ChatEvent {
    pub fn is_final(&self) -> bool {
        matches!(self, Self::Done { .. } | Self::Error { .. })
    }
}

/// 一次运行：事件流在 done 或 error 后结束。
pub struct Run {
    pub id: String,
    pub events: mpsc::UnboundedReceiver<AgentEvent>,
}

/// agent 子进程的拥有者。Clone 便宜。
#[derive(Clone)]
pub struct Runtime {
    sup: Arc<Supervisor>,
    settings: Settings,
    chats: Arc<Chats>,
}

/// 沙盒工具集（exec、read_file、write_file、edit_file），供 server 注册。
pub fn sandbox_tool_set(sandbox: ownmind_sandbox::api::Sandbox) -> Vec<Arc<dyn Tool>> {
    sandbox_tools(sandbox)
}

/// 联网工具的可调参数；生产用 `WebOptions::default()`。
#[derive(Debug, Clone, Default)]
pub struct WebOptions {
    /// 允许访问内网/回环地址（仅测试用，生产必须 false）。
    pub allow_private: bool,
    /// 覆盖 Brave 搜索接口地址（测试用）。
    pub brave_endpoint: Option<String>,
    /// 覆盖 Tavily 搜索接口地址（测试用）。
    pub tavily_endpoint: Option<String>,
}

/// 联网工具集（web_search、web_fetch），供 server 注册。
pub fn web_tool_set(settings: Settings) -> Vec<Arc<dyn Tool>> {
    web_tools(settings, WebOptions::default())
}

/// 同 web_tool_set，可调参数。
pub fn web_tool_set_with(settings: Settings, options: WebOptions) -> Vec<Arc<dyn Tool>> {
    web_tools(settings, options)
}

impl Runtime {
    /// 拉起 agent 子进程（cmd 为命令及参数），退出后自动重启。
    pub fn spawn(cmd: Vec<String>, conversations: Conversations, settings: Settings) -> Self {
        Self::spawn_with_tools(cmd, conversations, settings, Vec::new())
    }

    /// 同 spawn，并注册工具。
    pub fn spawn_with_tools(
        cmd: Vec<String>,
        conversations: Conversations,
        settings: Settings,
        tools: Vec<Arc<dyn Tool>>,
    ) -> Self {
        Self {
            sup: Supervisor::spawn(cmd, conversations.clone(), settings.clone(), tools),
            settings,
            chats: Arc::new(Chats::new(conversations)),
        }
    }

    /// 主人发一条消息：保存它、启动回复，返回事件流。
    /// 同一会话同时只允许一个回复；浏览器断开不影响回复继续并保存。
    pub async fn send(
        &self,
        conversation_id: &str,
        text: &str,
    ) -> Result<broadcast::Receiver<ChatEvent>, RuntimeError> {
        self.chats.send(self, conversation_id, text).await
    }

    /// 中止会话里进行中的回复。
    pub async fn stop(&self, conversation_id: &str) -> Result<(), RuntimeError> {
        let run_id = self
            .chats
            .active_run(conversation_id)
            .await
            .ok_or(RuntimeError::NotRunning)?;
        self.cancel(&run_id).await
    }

    /// 会话是否有进行中的回复。
    pub async fn is_running(&self, conversation_id: &str) -> bool {
        self.chats.active_run(conversation_id).await.is_some()
    }

    /// 用默认模型在会话上启动一次运行。会话里的消息由调用方事先存好。
    pub async fn start_run(&self, conversation_id: &str) -> Result<Run, RuntimeError> {
        let model = self.settings.resolve_default_model().await?;
        let spec = ModelSpec {
            kind: model.kind.as_str().to_owned(),
            base_url: model.base_url,
            api_key: model.api_key,
            model_id: model.model_id,
        };
        self.sup.start_run(conversation_id, spec).await
    }

    /// 中止运行；运行随后以 error 事件结束。
    pub async fn cancel(&self, run_id: &str) -> Result<(), RuntimeError> {
        self.sup.cancel(run_id).await
    }
}

impl Runtime {
    pub(crate) fn settings_ref(&self) -> &Settings {
        &self.settings
    }

    pub(crate) fn chats_ref(&self) -> &Chats {
        &self.chats
    }
}
