//! 联网工具走本地假服务器：搜索三种后端、抓取转文本、SSRF 拒绝、重定向校验。

use std::sync::Arc;

use axum::extract::Query;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::{Json, Router};
use ownmind_kernel::MasterKey;
use ownmind_runtime::api::{Tool, ToolContext, WebOptions, web_tool_set_with};
use ownmind_settings::api::{SearchInput, SearchProvider, Settings};
use serde_json::{Value, json};

async fn serve(app: Router) -> String {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    format!("http://{addr}")
}

async fn brave(
    headers: HeaderMap,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    assert_eq!(headers["x-subscription-token"], "bk");
    Json(json!({ "web": { "results": [
        { "title": format!("Brave <b>{}</b>", q["q"]), "url": "https://a.example/1", "description": "摘要 &amp; 一" }
    ]}}))
}

async fn tavily(headers: HeaderMap, Json(b): Json<Value>) -> Json<Value> {
    assert_eq!(headers["authorization"], "Bearer tk");
    Json(json!({ "results": [
        { "title": "Tavily", "url": "https://b.example", "content": b["query"] }
    ]}))
}

async fn searx(Query(q): Query<std::collections::HashMap<String, String>>) -> Json<Value> {
    assert_eq!(q["format"], "json");
    Json(json!({ "results": [ { "title": "Searx", "url": "https://c.example", "content": "c" } ] }))
}

async fn tools(allow_private: bool) -> (Vec<Arc<dyn Tool>>, Settings, String, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}?mode=rwc", dir.path().join("t.db").display());
    let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
    ownmind_settings::api::MIGRATOR.run(&pool).await.unwrap();
    let settings = Settings::new(pool, MasterKey::random());
    let base = serve(
        Router::new()
            .route("/brave", get(brave))
            .route("/tavily", post(tavily))
            .route("/search", get(searx))
            .route(
                "/page",
                get(|| async {
                    (
                        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                        "<html><script>x()</script><h1>你好</h1><p>正文 &amp; 内容</p></html>",
                    )
                }),
            )
            .route("/redir", get(|| async { Redirect::temporary("/page") }))
            .route(
                "/bin",
                get(|| async { ([(header::CONTENT_TYPE, "image/png")], "xx").into_response() }),
            )
            .route("/boom", get(|| async { StatusCode::INTERNAL_SERVER_ERROR })),
    )
    .await;
    let set = web_tool_set_with(
        settings.clone(),
        WebOptions {
            allow_private,
            brave_endpoint: Some(format!("{base}/brave")),
            tavily_endpoint: Some(format!("{base}/tavily")),
        },
    );
    (set, settings, base, dir)
}

async fn call(set: &[Arc<dyn Tool>], name: &str, args: Value) -> (bool, String) {
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
    (out.is_error, out.content)
}

fn input(provider: SearchProvider, base_url: Option<&str>, key: Option<&str>) -> SearchInput {
    SearchInput {
        provider,
        base_url: base_url.map(Into::into),
        api_key: key.map(Into::into),
    }
}

#[tokio::test]
async fn search_unconfigured_gives_clear_hint() {
    let (set, _s, _b, _d) = tools(true).await;
    let (err, text) = call(&set, "web_search", json!({ "query": "x" })).await;
    assert!(err);
    assert!(text.contains("还没有配置联网搜索"));
}

#[tokio::test]
async fn search_three_backends() {
    let (set, s, base, _d) = tools(true).await;
    s.set_search_config(input(SearchProvider::Brave, None, Some("bk")))
        .await
        .unwrap();
    let (err, text) = call(&set, "web_search", json!({ "query": "rust" })).await;
    assert!(!err, "{text}");
    assert!(text.contains("1. Brave rust"), "{text}");
    assert!(text.contains("https://a.example/1") && text.contains("摘要 & 一"));

    s.set_search_config(input(SearchProvider::Tavily, None, Some("tk")))
        .await
        .unwrap();
    let (err, text) = call(&set, "web_search", json!({ "query": "go" })).await;
    assert!(!err, "{text}");
    assert!(text.contains("Tavily") && text.contains("https://b.example"));

    s.set_search_config(input(SearchProvider::Searxng, Some(&base), None))
        .await
        .unwrap();
    let (err, text) = call(&set, "web_search", json!({ "query": "z" })).await;
    assert!(!err, "{text}");
    assert!(text.contains("Searx"));

    let (err, _) = call(&set, "web_search", json!({ "query": "  " })).await;
    assert!(err);
}

#[tokio::test]
async fn fetch_converts_html_and_follows_redirect() {
    let (set, _s, base, _d) = tools(true).await;
    for path in ["page", "redir"] {
        let (err, text) = call(
            &set,
            "web_fetch",
            json!({ "url": format!("{base}/{path}") }),
        )
        .await;
        assert!(!err, "{text}");
        assert!(
            text.contains("你好") && text.contains("正文 & 内容"),
            "{text}"
        );
        assert!(!text.contains("x()"));
    }
    let (err, text) = call(&set, "web_fetch", json!({ "url": format!("{base}/bin") })).await;
    assert!(err && text.contains("不支持的内容类型"), "{text}");
    let (err, text) = call(&set, "web_fetch", json!({ "url": format!("{base}/boom") })).await;
    assert!(err && text.contains("500"), "{text}");
    let (err, _) = call(&set, "web_fetch", json!({ "url": "ftp://x/y" })).await;
    assert!(err);
}

#[tokio::test]
async fn fetch_refuses_private_addresses_by_default() {
    let (set, _s, base, _d) = tools(false).await;
    for url in [
        format!("{base}/page"),
        "http://127.0.0.1:1/".to_owned(),
        "http://localhost/".to_owned(),
        "http://169.254.169.254/latest/meta-data".to_owned(),
        "http://[::1]/".to_owned(),
    ] {
        let (err, text) = call(&set, "web_fetch", json!({ "url": url })).await;
        assert!(err && text.contains("拒绝访问内网"), "{url}: {text}");
    }
}
