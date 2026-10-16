//! 聊天编排：存主人消息 → 启动运行 → 转发事件 → 存回复（票 09）。

use std::collections::HashMap;

use ownmind_conversation::api::{Conversations, NewMessage, Role};
use serde_json::json;
use tokio::sync::{Mutex, broadcast};

use crate::api::{AgentEvent, ChatEvent, Runtime, RuntimeError};

/// 每个会话正在进行的运行。value 为 None 表示正在启动、run_id 还没拿到。
pub(crate) struct Chats {
    conversations: Conversations,
    active: Mutex<HashMap<String, Option<String>>>,
}

impl Chats {
    pub(crate) fn new(conversations: Conversations) -> Self {
        Self {
            conversations,
            active: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) async fn active_run(&self, conversation_id: &str) -> Option<String> {
        self.active
            .lock()
            .await
            .get(conversation_id)
            .cloned()
            .flatten()
    }

    pub(crate) async fn send(
        &self,
        rt: &Runtime,
        conversation_id: &str,
        text: &str,
    ) -> Result<broadcast::Receiver<ChatEvent>, RuntimeError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(RuntimeError::EmptyMessage);
        }
        self.conversations.get(conversation_id).await?;
        // 先检查默认模型，没配置就不存消息。
        rt.settings_ref().resolve_default_model().await?;
        {
            let mut active = self.active.lock().await;
            if active.contains_key(conversation_id) {
                return Err(RuntimeError::Busy);
            }
            active.insert(conversation_id.to_owned(), None);
        }
        match self.start(rt, conversation_id, text).await {
            Ok(rx) => Ok(rx),
            Err(e) => {
                self.active.lock().await.remove(conversation_id);
                Err(e)
            }
        }
    }

    async fn start(
        &self,
        rt: &Runtime,
        conversation_id: &str,
        text: &str,
    ) -> Result<broadcast::Receiver<ChatEvent>, RuntimeError> {
        let user = self
            .conversations
            .append(
                conversation_id,
                NewMessage {
                    role: Role::User,
                    body: json!({ "role": "user", "content": text, "timestamp": ownmind_kernel::now_ms() }),
                    text: text.to_owned(),
                    input_tokens: None,
                    output_tokens: None,
                },
            )
            .await?;
        let mut run = rt.start_run(conversation_id).await?;
        self.active
            .lock()
            .await
            .insert(conversation_id.to_owned(), Some(run.id.clone()));

        let (tx, rx) = broadcast::channel(1024);
        let _ = tx.send(ChatEvent::User { message: user });
        let rt = rt.clone();
        let conv = conversation_id.to_owned();
        let conversations = self.conversations.clone();
        tokio::spawn(async move {
            let mut finished = false;
            while let Some(ev) = run.events.recv().await {
                let out = match ev {
                    AgentEvent::TextDelta { delta } => ChatEvent::Delta { delta },
                    AgentEvent::ThinkingDelta { delta } => ChatEvent::Thinking { delta },
                    AgentEvent::Error { message } => ChatEvent::Error { message },
                    AgentEvent::Done {
                        message,
                        text,
                        usage,
                    } => {
                        let saved = conversations
                            .append(
                                &conv,
                                NewMessage {
                                    role: Role::Assistant,
                                    body: message,
                                    text,
                                    input_tokens: Some(usage.input),
                                    output_tokens: Some(usage.output),
                                },
                            )
                            .await;
                        match saved {
                            Ok(message) => ChatEvent::Done { message },
                            Err(e) => ChatEvent::Error {
                                message: format!("保存回复失败：{e}"),
                            },
                        }
                    }
                };
                finished = out.is_final();
                // 浏览器已断开时没有接收者，忽略发送失败。
                let _ = tx.send(out);
                if finished {
                    break;
                }
            }
            if !finished {
                let _ = tx.send(ChatEvent::Error {
                    message: "agent 连接中断".into(),
                });
            }
            rt.chats_ref().active.lock().await.remove(&conv);
        });
        Ok(rx)
    }
}
