//! 公共入口：agent 运行时与 server↔agent 协议（决策 002、008）。

use std::sync::Arc;

use ownmind_conversation::api::Conversations;
use ownmind_settings::api::{Settings, SettingsError};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use ts_rs::TS;

use crate::internal::Supervisor;

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
#[ts(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    TextDelta {
        delta: String,
    },
    ThinkingDelta {
        delta: String,
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
}

impl Runtime {
    /// 拉起 agent 子进程（cmd 为命令及参数），退出后自动重启。
    pub fn spawn(cmd: Vec<String>, conversations: Conversations, settings: Settings) -> Self {
        Self {
            sup: Supervisor::spawn(cmd, conversations),
            settings,
        }
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
