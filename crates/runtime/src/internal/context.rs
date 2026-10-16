//! 上下文组装（决策 005）。M1 第一版：系统提示 + 全部原文 + 末尾时间片段；压缩见票 10。

use ownmind_conversation::api::Conversations;

use crate::api::ContextBuildResult;

const SYSTEM_PROMPT: &str = "你是 Ownmind，主人部署在自己服务器上的个人 AI 助手。\
回答准确、简洁，用主人使用的语言回复。不知道就说不知道，不编造事实。";

pub(crate) async fn build(
    conversations: &Conversations,
    conversation_id: &str,
) -> Result<ContextBuildResult, String> {
    let messages = conversations
        .messages(conversation_id)
        .await
        .map_err(|e| e.to_string())?;
    let now = ownmind_kernel::now_ms();
    Ok(ContextBuildResult {
        system_prompt: format!("{SYSTEM_PROMPT}\n\n当前时间（UTC 毫秒）：{now}"),
        messages: messages.into_iter().map(|m| m.body).collect(),
    })
}
