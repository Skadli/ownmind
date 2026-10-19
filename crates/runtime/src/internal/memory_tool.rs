//! memory 工具：读写核心记忆，增删、检索记忆条目。

use std::sync::Arc;

use ownmind_memory::api::Memory;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{Tool, ToolContext, ToolFuture, ToolOutput, ToolSpec};

pub(crate) fn memory_tools(memory: Memory) -> Vec<Arc<dyn Tool>> {
    vec![Arc::new(MemoryTool { memory })]
}

struct MemoryTool {
    memory: Memory,
}

#[derive(Deserialize)]
struct In {
    action: String,
    text: Option<String>,
    base: Option<String>,
    id: Option<String>,
    query: Option<String>,
}

impl Tool for MemoryTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "memory".into(),
            description: "主人的长期记忆，跨会话保留。核心记忆（约 2000 token，每次对话都会带上）\
放主人的资料与长期偏好；记忆条目放其他值得记住的事，需要时再检索。\
action：read_core 读核心记忆；write_core 整体替换核心记忆（text 为新全文；base 为刚 read_core 读到的原文，核心记忆为空时可省略；\
别的会话改过则冲突，须重新读取再合并）；\
add 新增条目（text）；delete 删除条目（id）；search 按关键词检索条目（query，空格分隔表示都要命中）；\
recent 最近的条目。"
                .into(),
            parameters: json!({ "type": "object", "properties": {
                "action": { "type": "string",
                    "enum": ["read_core", "write_core", "add", "delete", "search", "recent"] },
                "text": { "type": "string" },
                "base": { "type": "string" },
                "id": { "type": "string" },
                "query": { "type": "string" }
            }, "required": ["action"] }),
        }
    }

    fn call(&self, _ctx: ToolContext, arguments: Value) -> ToolFuture {
        let m = self.memory.clone();
        Box::pin(async move {
            let a = match serde_json::from_value::<In>(arguments) {
                Ok(a) => a,
                Err(e) => return ToolOutput::error(format!("参数不合法：{e}")),
            };
            match run(&m, a).await {
                Ok(t) => ToolOutput::ok(t),
                Err(e) => ToolOutput::error(e),
            }
        })
    }
}

fn need(v: Option<String>, name: &str) -> Result<String, String> {
    v.ok_or_else(|| format!("缺少参数 {name}"))
}

fn show(es: &[ownmind_memory::api::Entry]) -> String {
    if es.is_empty() {
        return "没有记忆条目".into();
    }
    es.iter()
        .map(|e| format!("[{}] {}", e.id, e.content))
        .collect::<Vec<_>>()
        .join("\n")
}

async fn run(m: &Memory, a: In) -> Result<String, String> {
    let err = |e: ownmind_memory::api::MemoryError| e.to_string();
    match a.action.as_str() {
        "read_core" => {
            let c = m.core().await.map_err(err)?;
            Ok(if c.is_empty() {
                "核心记忆还是空的".into()
            } else {
                c
            })
        }
        "write_core" => {
            m.set_core(a.base.as_deref().unwrap_or(""), &need(a.text, "text")?)
                .await
                .map_err(err)?;
            Ok("核心记忆已更新".into())
        }
        "add" => {
            let e = m.add(&need(a.text, "text")?).await.map_err(err)?;
            Ok(format!("已记下，id={}", e.id))
        }
        "delete" => {
            let ok = m.delete(&need(a.id, "id")?).await.map_err(err)?;
            Ok(if ok {
                "已删除".into()
            } else {
                "没有这个 id".into()
            })
        }
        "search" => Ok(show(
            &m.search(&need(a.query, "query")?, 10).await.map_err(err)?,
        )),
        "recent" => Ok(show(&m.recent(10).await.map_err(err)?)),
        other => Err(format!("未知 action：{other}")),
    }
}
