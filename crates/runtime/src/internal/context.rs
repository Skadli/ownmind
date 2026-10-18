//! 上下文组装（决策 005）：系统提示 → 摘要 → 摘要后的原文 → 末尾时间片段；超窗 70% 时请求压缩。

use ownmind_conversation::api::{Conversations, Message, Role};
use ownmind_settings::api::{Profile, Settings};
use serde_json::{Value, json};

use crate::api::{CompactionRequest, ContextBuildParams, ContextBuildResult};

const SYSTEM_PROMPT: &str = "你是 Ownmind，主人部署在自己服务器上的个人 AI 助手。\
回答准确、简洁，用主人使用的语言回复。不知道就说不知道，不编造事实。";

/// 超过窗口这个比例就压缩。
const COMPACT_RATIO: f64 = 0.7;
/// 压缩时保留最近这么多轮原文（一轮从一条主人消息开始）。
const KEEP_TURNS: usize = 6;
/// 工具结果只在最近这么多轮里保留原文。
const KEEP_TOOL_TURNS: usize = 10;

pub(crate) async fn build(
    conversations: &Conversations,
    settings: &Settings,
    p: &ContextBuildParams,
) -> Result<ContextBuildResult, String> {
    let profile = settings.profile().await.map_err(|e| e.to_string())?;
    let system = settings.system().await.map_err(|e| e.to_string())?;
    let all = conversations
        .messages(&p.conversation_id)
        .await
        .map_err(|e| e.to_string())?;
    let compaction = conversations
        .latest_compaction(&p.conversation_id)
        .await
        .map_err(|e| e.to_string())?;
    let (summary, upto) = match &compaction {
        Some(c) => (Some(c.summary.as_str()), c.upto_seq),
        None => (None, 0),
    };
    // 旧工具结果先换成占位：估算、压缩请求和最终消息都用同一份表示。
    let mut live_owned: Vec<Message> = all.iter().filter(|m| m.seq > upto).cloned().collect();
    slim_old_tool_results(&mut live_owned);
    let live: Vec<&Message> = live_owned.iter().collect();

    let mut system_prompt = SYSTEM_PROMPT.to_owned();
    system_prompt.push_str(&core_layer(&profile, &system.timezone));
    if let Some(s) = summary {
        system_prompt.push_str("\n\n## 之前对话的摘要\n");
        system_prompt.push_str(s);
    }
    let estimated_tokens = estimate_chars(&system_prompt) + estimate(&live);

    let compaction = (estimated_tokens as f64 > f64::from(p.context_window) * COMPACT_RATIO)
        .then(|| compaction_request(&live, summary))
        .flatten();

    let mut messages: Vec<Value> = live.iter().map(|m| m.body.clone()).collect();
    append_time(&mut messages, ownmind_kernel::now_ms(), &system.timezone);
    Ok(ContextBuildResult {
        system_prompt,
        messages,
        estimated_tokens,
        compaction,
        tools: Vec::new(),
    })
}

/// 核心层：主人资料（决策 005 第 4 层；记忆部分在 M2 加）。
fn core_layer(p: &Profile, timezone: &str) -> String {
    let mut lines = vec![];
    if !p.name.is_empty() {
        lines.push(format!("- 名字：{}", p.name));
    }
    if !p.preferred_name.is_empty() {
        lines.push(format!("- 称呼：{}", p.preferred_name));
    }
    if !p.bio.is_empty() {
        lines.push(format!("- 简介：{}", p.bio));
    }
    lines.push(format!("- 时区：{timezone}"));
    format!("\n\n## 主人资料\n{}", lines.join("\n"))
}

/// token 估算：最后一条带 usage 的回复，其 input+output 就是到它为止的上下文；之后的按字符估。
fn estimate(msgs: &[&Message]) -> i64 {
    // 有的兼容接口不返回 usage（记为 0），这种情况按字符估算。
    let last_usage = msgs
        .iter()
        .rposition(|m| m.input_tokens.is_some_and(|t| t > 0));
    match last_usage {
        Some(i) => {
            let m = msgs[i];
            m.input_tokens.unwrap_or(0)
                + m.output_tokens.unwrap_or(0)
                + msgs[i + 1..]
                    .iter()
                    .map(|m| estimate_chars(&m.text))
                    .sum::<i64>()
        }
        None => msgs.iter().map(|m| estimate_chars(&m.text)).sum(),
    }
}

/// 按字符估算：ASCII 约 4 个字符一个 token，其他（中文等）约 1 个字符一个 token。
pub(crate) fn estimate_chars(text: &str) -> i64 {
    let (ascii, other) = text.chars().fold((0i64, 0i64), |(a, o), c| {
        if c.is_ascii() { (a + 1, o) } else { (a, o + 1) }
    });
    (ascii + 3) / 4 + other + 4
}

/// 保留最近 KEEP_TURNS 轮，之前的交给 agent 总结；轮数不够就不压缩。
fn compaction_request(live: &[&Message], previous: Option<&str>) -> Option<CompactionRequest> {
    let user_idx: Vec<usize> = live
        .iter()
        .enumerate()
        .filter(|(_, m)| m.role == Role::User)
        .map(|(i, _)| i)
        .collect();
    if user_idx.len() <= KEEP_TURNS {
        return None;
    }
    let keep_from = user_idx[user_idx.len() - KEEP_TURNS];
    let old = &live[..keep_from];
    let transcript = old
        .iter()
        .map(|m| {
            let who = match m.role {
                Role::User => "主人",
                Role::Assistant => "助手",
                Role::Tool => "工具结果",
            };
            format!("[{who}]\n{}", m.text)
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    Some(CompactionRequest {
        upto_seq: old.last()?.seq,
        previous_summary: previous.map(str::to_owned),
        transcript,
    })
}

/// 时间放在最后一条主人消息的末尾，前面的内容保持不变，利于提示缓存。
fn append_time(messages: &mut [Value], now_ms: i64, timezone: &str) {
    let Some(last) = messages.last_mut() else {
        return;
    };
    if last.get("role").and_then(Value::as_str) != Some("user") {
        return;
    }
    let note = json!({ "type": "text", "text": format!(
        "\n\n<context>当前时间：{}（主人时区 {timezone}）</context>",
        format_utc(now_ms)
    ) });
    let content = last.get_mut("content");
    match content {
        Some(Value::String(s)) => {
            let text = json!({ "type": "text", "text": s.clone() });
            last["content"] = json!([text, note]);
        }
        Some(Value::Array(blocks)) => blocks.push(note),
        _ => {}
    }
}

/// 超过最近 KEEP_TOOL_TURNS 轮的工具结果换成一行占位（决策 005.4），原文仍在库里。
/// 同时改 body 和 text，这样 token 估算与压缩请求也只看到占位。
fn slim_old_tool_results(live: &mut [Message]) {
    let user_idx: Vec<usize> = live
        .iter()
        .enumerate()
        .filter(|(_, m)| m.role == Role::User)
        .map(|(i, _)| i)
        .collect();
    if user_idx.len() <= KEEP_TOOL_TURNS {
        return;
    }
    let cutoff = user_idx[user_idx.len() - KEEP_TOOL_TURNS];
    for m in live.iter_mut().take(cutoff) {
        if m.role != Role::Tool {
            continue;
        }
        let name = m
            .body
            .get("toolName")
            .and_then(Value::as_str)
            .unwrap_or("工具")
            .to_owned();
        let path = m
            .text
            .find(".outputs/")
            .map(|i| {
                let rest = &m.text[i..];
                let end = rest
                    .find(|c: char| c.is_whitespace() || "，。）)\\\"'".contains(c))
                    .unwrap_or(rest.len());
                format!("；全文在沙盒 /workspace/{}", &rest[..end])
            })
            .unwrap_or_default();
        let note = format!("[较早的 {name} 结果已省略{path}]");
        m.body["content"] = json!([{ "type": "text", "text": note }]);
        m.text = note;
    }
}

/// UTC 毫秒 → "2026-10-16 07:00 UTC"（分钟精度，避免每秒都变）。
fn format_utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // 公历换算（Howard Hinnant 的 civil_from_days）。
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        rem / 3600,
        rem % 3600 / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_utc_minutes() {
        assert_eq!(format_utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_utc(1_791_961_200_000), "2026-10-14 07:00 UTC");
    }

    #[test]
    fn estimates_cjk_heavier_than_ascii() {
        assert_eq!(estimate_chars("abcdefgh"), 2 + 4);
        assert_eq!(estimate_chars("你好"), 2 + 4);
    }

    #[test]
    fn core_layer_lists_only_filled_fields() {
        let p = Profile {
            name: "".into(),
            preferred_name: "老板".into(),
            bio: "".into(),
        };
        assert_eq!(
            core_layer(&p, "Asia/Shanghai"),
            "\n\n## 主人资料\n- 称呼：老板\n- 时区：Asia/Shanghai"
        );
    }

    fn msg(seq: i64, role: Role, text: &str, body: Value) -> Message {
        Message {
            id: seq.to_string(),
            conversation_id: "c".into(),
            seq,
            role,
            body,
            text: text.into(),
            input_tokens: None,
            output_tokens: None,
            created_at: 0,
        }
    }

    #[test]
    fn old_tool_results_become_placeholders() {
        let mut all = vec![
            msg(
                1,
                Role::User,
                "q0",
                json!({ "role": "user", "content": "q0" }),
            ),
            msg(
                2,
                Role::Tool,
                "全文在 /workspace/.outputs/abc。",
                json!({ "role": "toolResult", "toolName": "exec", "content": [{ "type": "text", "text": "很长" }] }),
            ),
        ];
        for i in 0..10 {
            all.push(msg(
                3 + 2 * i,
                Role::User,
                "q",
                json!({ "role": "user", "content": "q" }),
            ));
            all.push(msg(
                4 + 2 * i,
                Role::Tool,
                "近",
                json!({ "role": "toolResult", "toolName": "exec", "content": [{ "type": "text", "text": "近" }] }),
            ));
        }
        slim_old_tool_results(&mut all);
        let bodies: Vec<Value> = all.iter().map(|m| m.body.clone()).collect();
        assert!(all[1].text.starts_with("[较早的 exec"));
        assert_eq!(all[3].text, "近");
        assert_eq!(
            bodies[1]["content"][0]["text"],
            "[较早的 exec 结果已省略；全文在沙盒 /workspace/.outputs/abc]"
        );
        assert_eq!(bodies[3]["content"][0]["text"], "近");
        assert_eq!(bodies[21]["content"][0]["text"], "近");
    }

    #[test]
    fn time_goes_into_the_last_user_message() {
        let mut msgs = vec![json!({ "role": "user", "content": "hi" })];
        append_time(&mut msgs, 0, "UTC");
        assert_eq!(msgs[0]["content"][0]["text"], "hi");
        assert!(
            msgs[0]["content"][1]["text"]
                .as_str()
                .unwrap()
                .contains("1970-01-01")
        );
    }
}
