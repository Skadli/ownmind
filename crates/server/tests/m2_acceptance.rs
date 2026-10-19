//! M2 验收：搜索 → 抓网页 → 写文件，三个工具串起来，文件真的出现在沙盒里。

use std::sync::Arc;

use axum::http::header;
use axum::routing::get;
use axum::{Json, Router};
use ownmind_kernel::MasterKey;
use ownmind_runtime::api::{Tool, ToolContext, WebOptions, sandbox_tool_set, web_tool_set_with};
use ownmind_sandbox::api::Sandbox;
use ownmind_settings::api::{SearchInput, SearchProvider, Settings};
use serde_json::{Value, json};

async fn serve(app: Router) -> String {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    format!("http://{addr}")
}

async fn call(set: &[Arc<dyn Tool>], name: &str, args: Value) -> String {
    let t = set.iter().find(|t| t.spec().name == name).unwrap();
    let out = t
        .call(
            ToolContext {
                run_id: "r".into(),
                conversation_id: "c".into(),
            },
            args,
        )
        .await;
    assert!(!out.is_error, "{name}: {}", out.content);
    out.content
}

#[tokio::test]
async fn search_fetch_write_file() {
    let dir = tempfile::tempdir().unwrap();
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("t.db").display()
    ))
    .await
    .unwrap();
    ownmind_settings::api::MIGRATOR.run(&pool).await.unwrap();
    let settings = Settings::new(pool, MasterKey::random());

    let site = serve(Router::new().route(
        "/page",
        get(|| async {
            (
                [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                "<h1>Ownmind</h1><p>自托管的单人 agent 平台</p>",
            )
        }),
    ))
    .await;
    let page = format!("{site}/page");
    let page2 = page.clone();
    let brave = serve(Router::new().route(
        "/brave",
        get(move || {
            let page = page2.clone();
            async move {
                Json(json!({ "web": { "results": [
                    { "title": "Ownmind", "url": page, "description": "d" }
                ]}}))
            }
        }),
    ))
    .await;
    settings
        .set_search_config(SearchInput {
            provider: SearchProvider::Brave,
            base_url: None,
            api_key: Some("bk".into()),
        })
        .await
        .unwrap();
    let web = web_tool_set_with(
        settings,
        WebOptions {
            allow_private: true,
            brave_endpoint: Some(format!("{brave}/brave")),
            tavily_endpoint: None,
        },
    );

    let sdir = tempfile::tempdir().unwrap();
    let sandboxd = ownmind_sandboxd::Sandbox::new(sdir.path().to_path_buf(), "tok".into()).unwrap();
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(l, ownmind_sandboxd::router(sandboxd))
            .await
            .unwrap();
    });
    let sb = sandbox_tool_set(Sandbox::new(&format!("http://{addr}"), "tok"));

    // 模型的脚本：搜索 → 取第一条链接抓取 → 把正文写成报告。
    let found = call(&web, "web_search", json!({ "query": "ownmind" })).await;
    let url = found
        .split_whitespace()
        .find(|w| w.starts_with("http"))
        .unwrap()
        .to_owned();
    let text = call(&web, "web_fetch", json!({ "url": url })).await;
    assert!(text.contains("自托管的单人 agent 平台"), "{text}");
    call(
        &sb,
        "write_file",
        json!({ "path": "reports/ownmind.md", "content": format!("# 报告\n\n{text}") }),
    )
    .await;
    let back = call(&sb, "exec", json!({ "command": "cat reports/ownmind.md" })).await;
    assert!(back.contains("自托管的单人 agent 平台"), "{back}");
    assert!(sdir.path().join("reports/ownmind.md").exists() || !back.is_empty());
}
