//! 沙盒工具：exec、read_file、write_file、edit_file（经 sandbox 客户端调用 sandboxd）。
//! 单次结果超过 8k token 时截断，全文写进沙盒 `.outputs/<id>`（决策 005.3）。

use std::sync::Arc;

use ownmind_sandbox::api::{ExecArgs, Sandbox, SandboxError};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{Tool, ToolContext, ToolFuture, ToolOutput, ToolSpec};
use crate::internal::context::estimate_chars;

/// 单次工具结果的 token 上限。
pub(crate) const MAX_RESULT_TOKENS: i64 = 8000;

pub(crate) fn sandbox_tools(sb: Sandbox) -> Vec<Arc<dyn Tool>> {
    ["exec", "read_file", "write_file", "edit_file"]
        .into_iter()
        .map(|name| {
            Arc::new(SandboxTool {
                sb: sb.clone(),
                name,
            }) as Arc<dyn Tool>
        })
        .collect()
}

struct SandboxTool {
    sb: Sandbox,
    name: &'static str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExecIn {
    command: String,
    cwd: Option<String>,
    timeout_secs: Option<u64>,
}

#[derive(Deserialize)]
struct PathIn {
    path: String,
}

#[derive(Deserialize)]
struct WriteIn {
    path: String,
    content: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EditIn {
    path: String,
    old_string: String,
    new_string: String,
}

impl Tool for SandboxTool {
    fn spec(&self) -> ToolSpec {
        let (description, parameters) = match self.name {
            "exec" => (
                "在沙盒工作目录里用 sh -c 执行命令，返回退出码、stdout、stderr。默认超时 60 秒，最长 600 秒。",
                json!({ "type": "object", "properties": {
                    "command": { "type": "string", "description": "要执行的 shell 命令" },
                    "cwd": { "type": "string", "description": "相对工作目录的子目录，可省略" },
                    "timeoutSecs": { "type": "integer", "description": "超时秒数，1–600" }
                }, "required": ["command"] }),
            ),
            "read_file" => (
                "读取沙盒里的文本文件（路径相对工作目录）。",
                json!({ "type": "object", "properties": {
                    "path": { "type": "string" }
                }, "required": ["path"] }),
            ),
            "write_file" => (
                "写入（覆盖）沙盒里的文件，必要时创建目录。",
                json!({ "type": "object", "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                }, "required": ["path", "content"] }),
            ),
            _ => (
                "把文件里唯一出现的 oldString 替换为 newString；出现零次或多次会报错。",
                json!({ "type": "object", "properties": {
                    "path": { "type": "string" },
                    "oldString": { "type": "string" },
                    "newString": { "type": "string" }
                }, "required": ["path", "oldString", "newString"] }),
            ),
        };
        ToolSpec {
            name: self.name.to_owned(),
            description: description.to_owned(),
            parameters,
        }
    }

    fn call(&self, ctx: ToolContext, arguments: Value) -> ToolFuture {
        let sb = self.sb.clone();
        let name = self.name;
        Box::pin(async move {
            match run(&sb, name, arguments).await {
                Ok(text) => limit_output(&sb, &ctx, text).await,
                Err(msg) => ToolOutput::error(msg),
            }
        })
    }
}

fn args<T: for<'de> Deserialize<'de>>(v: Value) -> Result<T, String> {
    serde_json::from_value(v).map_err(|e| format!("参数不合法：{e}"))
}

fn fail(e: SandboxError) -> String {
    e.to_string()
}

async fn run(sb: &Sandbox, name: &str, arguments: Value) -> Result<String, String> {
    match name {
        "exec" => {
            let a: ExecIn = args(arguments)?;
            let r = sb
                .exec(&ExecArgs {
                    command: a.command,
                    cwd: a.cwd,
                    timeout_secs: a.timeout_secs,
                })
                .await
                .map_err(fail)?;
            let mut out = match (r.timed_out, r.exit_code) {
                (true, _) => "命令超时被终止\n".to_owned(),
                (_, Some(c)) => format!("退出码：{c}\n"),
                (_, None) => "命令被信号终止\n".to_owned(),
            };
            if !r.stdout.is_empty() {
                out.push_str(&format!("--- stdout ---\n{}\n", r.stdout));
            }
            if !r.stderr.is_empty() {
                out.push_str(&format!("--- stderr ---\n{}\n", r.stderr));
            }
            if r.stdout_truncated || r.stderr_truncated {
                out.push_str("（输出超过 1 MB，已被沙盒截断）\n");
            }
            Ok(out)
        }
        "read_file" => {
            let a: PathIn = args(arguments)?;
            let r = sb.read(&a.path).await.map_err(fail)?;
            Ok(if r.truncated {
                format!(
                    "{}\n（文件共 {} 字节，只读到前面一部分）",
                    r.content, r.size
                )
            } else {
                r.content
            })
        }
        "write_file" => {
            let a: WriteIn = args(arguments)?;
            let n = sb.write(&a.path, &a.content).await.map_err(fail)?;
            Ok(format!("已写入 {} 字节到 {}", n, a.path))
        }
        _ => {
            let a: EditIn = args(arguments)?;
            sb.edit(&a.path, &a.old_string, &a.new_string)
                .await
                .map_err(fail)?;
            Ok(format!("已修改 {}", a.path))
        }
    }
}

/// 结果超限就截断；全文存到沙盒 `.outputs/<id>`，结果末尾给出路径。
async fn limit_output(sb: &Sandbox, ctx: &ToolContext, text: String) -> ToolOutput {
    if estimate_chars(&text) <= MAX_RESULT_TOKENS {
        return ToolOutput::ok(text);
    }
    let head = head_within(&text, MAX_RESULT_TOKENS);
    let rel = format!(".outputs/{}", ownmind_kernel::new_id());
    match sb.write(&rel, &text).await {
        Ok(_) => ToolOutput::ok(format!(
            "{head}\n\n…（结果过长已截断，全文在沙盒 /workspace/{rel}，可用 read_file 或 exec 分段读取）"
        )),
        Err(e) => {
            tracing::warn!(error = %e, run_id = %ctx.run_id, "保存完整工具输出失败");
            ToolOutput::ok(format!("{head}\n\n…（结果过长已截断，完整输出保存失败）"))
        }
    }
}

/// 取开头尽量多、估算不超过 `tokens` 的部分（ASCII 约 4 字符一个 token，其他 1 字符一个）。
fn head_within(text: &str, tokens: i64) -> &str {
    let mut cost_x4 = 0i64;
    let budget = tokens * 4;
    for (i, c) in text.char_indices() {
        cost_x4 += if c.is_ascii() { 1 } else { 4 };
        if cost_x4 > budget {
            return &text[..i];
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_respects_token_budget() {
        assert_eq!(head_within("abcdefgh", 1), "abcd");
        assert_eq!(head_within("你好吗", 2), "你好");
        assert_eq!(head_within("短", 10), "短");
    }

    #[test]
    fn four_tools_with_object_schemas() {
        let tools = sandbox_tools(Sandbox::new("http://127.0.0.1:1", "t"));
        let names: Vec<_> = tools.iter().map(|t| t.spec().name).collect();
        assert_eq!(names, ["exec", "read_file", "write_file", "edit_file"]);
        assert!(
            tools
                .iter()
                .all(|t| t.spec().parameters["type"] == "object")
        );
    }
}
