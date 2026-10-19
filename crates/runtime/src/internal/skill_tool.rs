//! skill 工具：列出、通过 Git 链接安装、卸载技能。

use std::sync::Arc;

use ownmind_skill::api::Skills;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{Tool, ToolContext, ToolFuture, ToolOutput, ToolSpec};

pub(crate) fn skill_tools(skills: Skills) -> Vec<Arc<dyn Tool>> {
    vec![Arc::new(SkillTool { skills })]
}

struct SkillTool {
    skills: Skills,
}

#[derive(Deserialize)]
struct In {
    action: String,
    url: Option<String>,
    name: Option<String>,
}

impl Tool for SkillTool {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "skill".into(),
            description:
                "管理技能。action：list 列出全部技能；install 用 Git 链接（url，http/https）\
安装技能，仓库根目录须有带 name、description front matter 的 SKILL.md，同名会覆盖升级；\
remove 卸载已装技能（name）。预设技能不能卸载。"
                    .into(),
            parameters: json!({ "type": "object", "properties": {
                "action": { "type": "string", "enum": ["list", "install", "remove"] },
                "url": { "type": "string" },
                "name": { "type": "string" }
            }, "required": ["action"] }),
        }
    }

    fn call(&self, _ctx: ToolContext, arguments: Value) -> ToolFuture {
        let s = self.skills.clone();
        Box::pin(async move {
            let a = match serde_json::from_value::<In>(arguments) {
                Ok(a) => a,
                Err(e) => return ToolOutput::error(format!("参数不合法：{e}")),
            };
            match run(&s, a).await {
                Ok(t) => ToolOutput::ok(t),
                Err(e) => ToolOutput::error(e),
            }
        })
    }
}

async fn run(s: &Skills, a: In) -> Result<String, String> {
    let err = |e: ownmind_skill::api::SkillError| e.to_string();
    match a.action.as_str() {
        "list" => {
            let v = s.index().await.map_err(err)?;
            if v.is_empty() {
                return Ok("还没有技能".into());
            }
            Ok(v.iter()
                .map(|i| format!("{}（{}）：{}  {}", i.name, i.source, i.description, i.path))
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "install" => {
            let url = a.url.ok_or("缺少参数 url")?;
            let i = s.install(&url).await.map_err(err)?;
            Ok(format!(
                "已安装技能 {}，需要时读 .skills/installed/{}/SKILL.md",
                i.name, i.name
            ))
        }
        "remove" => {
            let name = a.name.ok_or("缺少参数 name")?;
            Ok(if s.remove(&name).await.map_err(err)? {
                "已卸载".into()
            } else {
                "没有这个已装技能（预设技能不能卸载）".into()
            })
        }
        other => Err(format!("未知 action：{other}")),
    }
}
