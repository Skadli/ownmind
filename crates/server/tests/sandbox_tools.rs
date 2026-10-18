//! 沙盒工具经真实 sandboxd（进程内）跑通：执行、读写、编辑，以及超长输出落盘。

use ownmind_runtime::api::{ToolContext, sandbox_tool_set};
use ownmind_sandbox::api::Sandbox;
use serde_json::json;

async fn start() -> (
    Vec<std::sync::Arc<dyn ownmind_runtime::api::Tool>>,
    Sandbox,
    tempfile::TempDir,
) {
    let dir = tempfile::tempdir().unwrap();
    let sandboxd = ownmind_sandboxd::Sandbox::new(dir.path().to_path_buf(), "tok".into()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, ownmind_sandboxd::router(sandboxd))
            .await
            .unwrap();
    });
    let client = Sandbox::new(&format!("http://{addr}"), "tok");
    (sandbox_tool_set(client.clone()), client, dir)
}

fn ctx() -> ToolContext {
    ToolContext {
        run_id: "r".into(),
        conversation_id: "c".into(),
    }
}

async fn call(
    tools: &[std::sync::Arc<dyn ownmind_runtime::api::Tool>],
    name: &str,
    args: serde_json::Value,
) -> ownmind_runtime::api::ToolOutput {
    let tool = tools.iter().find(|t| t.spec().name == name).unwrap();
    tool.call(ctx(), args).await
}

#[tokio::test]
async fn write_read_edit_exec() {
    let (tools, _, _dir) = start().await;
    let w = call(
        &tools,
        "write_file",
        json!({ "path": "a/hello.txt", "content": "你好 world" }),
    )
    .await;
    assert!(!w.is_error, "{}", w.content);
    let e = call(
        &tools,
        "edit_file",
        json!({ "path": "a/hello.txt", "oldString": "world", "newString": "sandbox" }),
    )
    .await;
    assert!(!e.is_error, "{}", e.content);
    let r = call(&tools, "read_file", json!({ "path": "a/hello.txt" })).await;
    assert_eq!(r.content, "你好 sandbox");
    let x = call(
        &tools,
        "exec",
        json!({ "command": "cat a/hello.txt; echo err >&2; exit 3" }),
    )
    .await;
    assert!(!x.is_error);
    assert!(
        x.content.contains("退出码：3")
            && x.content.contains("你好 sandbox")
            && x.content.contains("err")
    );
}

#[tokio::test]
async fn errors_are_tool_errors() {
    let (tools, _, _dir) = start().await;
    let r = call(&tools, "read_file", json!({ "path": "../etc/passwd" })).await;
    assert!(r.is_error);
    let r = call(&tools, "read_file", json!({ "path": "nope.txt" })).await;
    assert!(r.is_error);
    let r = call(&tools, "exec", json!({})).await;
    assert!(r.is_error && r.content.contains("参数不合法"));
}

#[tokio::test]
async fn long_output_is_truncated_and_saved() {
    let (tools, client, _dir) = start().await;
    // 约 12 万个字符 ≈ 3 万 token，超过 8k。
    let out = call(
        &tools,
        "exec",
        json!({ "command": "yes abcdefgh | head -n 13000" }),
    )
    .await;
    assert!(!out.is_error);
    assert!(
        out.content.chars().count() < 40_000,
        "{}",
        out.content.len()
    );
    let marker = "/workspace/.outputs/";
    let at = out.content.find(marker).expect("结果里要有全文路径");
    let rel: String = out.content[at + "/workspace/".len()..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '/' || *c == '-')
        .collect();
    let full = client.read(&rel).await.unwrap();
    assert!(full.content.matches("abcdefgh").count() >= 13000);
}
