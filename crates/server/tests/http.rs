use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use ownmind_server::{App, migrate, open_db, router};
use serde_json::{Value, json};
use tower::ServiceExt;

fn fake_agent() -> Vec<String> {
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../runtime/tests/fake_agent.mjs");
    vec!["node".into(), script.display().to_string()]
}

struct TestServer {
    _dir: tempfile::TempDir,
    router: Router,
    cookie: Option<String>,
}

impl TestServer {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let web = dir.path().join("web");
        std::fs::create_dir_all(&web).unwrap();
        std::fs::write(web.join("index.html"), "<p>ownmind</p>").unwrap();
        std::fs::write(web.join("app.js"), "console.log(1)").unwrap();
        let pool = open_db(&dir.path().join("data")).await.unwrap();
        migrate(&pool).await.unwrap();
        let router = router(
            App::new(pool, ownmind_kernel::MasterKey::random(), fake_agent()),
            &web,
        );
        Self {
            _dir: dir,
            router,
            cookie: None,
        }
    }

    /// 发请求；响应里的 Set-Cookie 会被记住，像浏览器一样。
    async fn call(
        &mut self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, String) {
        let mut req = Request::builder().method(method).uri(path);
        if let Some(c) = &self.cookie {
            req = req.header(header::COOKIE, c);
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let res = self.router.clone().oneshot(req).await.unwrap();
        if let Some(set) = res.headers().get(header::SET_COOKIE) {
            let pair = set.to_str().unwrap().split(';').next().unwrap().to_owned();
            self.cookie = if pair.ends_with('=') {
                None
            } else {
                Some(pair)
            };
        }
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    async fn json(
        &mut self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let (status, text) = self.call(method, path, body).await;
        (
            status,
            if text.is_empty() {
                Value::Null
            } else {
                serde_json::from_str(&text).unwrap()
            },
        )
    }
}

#[tokio::test]
async fn healthz_reports_ok() {
    let mut s = TestServer::start().await;
    assert_eq!(
        s.call(Method::GET, "/healthz", None).await,
        (StatusCode::OK, r#"{"status":"ok"}"#.into())
    );
}

#[tokio::test]
async fn serves_web_assets_and_falls_back_to_index() {
    let mut s = TestServer::start().await;
    assert_eq!(
        s.call(Method::GET, "/app.js", None).await,
        (StatusCode::OK, "console.log(1)".into())
    );
    assert_eq!(
        s.call(Method::GET, "/conversations/42", None).await,
        (StatusCode::OK, "<p>ownmind</p>".into())
    );
    assert_eq!(
        s.call(Method::GET, "/api/nope", None).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn first_visit_sets_up_owner_and_logs_in() {
    let mut s = TestServer::start().await;
    let (_, status) = s.json(Method::GET, "/api/auth/status", None).await;
    assert_eq!(status, json!({ "isSetUp": false, "isLoggedIn": false }));

    let pw = json!({ "password": "correct horse battery" });
    assert_eq!(
        s.call(Method::POST, "/api/auth/setup", Some(pw.clone()))
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let (_, status) = s.json(Method::GET, "/api/auth/status", None).await;
    assert_eq!(status, json!({ "isSetUp": true, "isLoggedIn": true }));

    let (code, body) = s.json(Method::POST, "/api/auth/setup", Some(pw)).await;
    assert_eq!(
        (code, body),
        (StatusCode::CONFLICT, json!({ "error": "already_set_up" }))
    );
}

#[tokio::test]
async fn conversations_require_login() {
    let mut s = TestServer::start().await;
    s.call(
        Method::POST,
        "/api/auth/setup",
        Some(json!({ "password": "correct horse battery" })),
    )
    .await;
    s.call(Method::POST, "/api/auth/logout", None).await;

    let (code, body) = s.json(Method::GET, "/api/conversations", None).await;
    assert_eq!(
        (code, body),
        (
            StatusCode::UNAUTHORIZED,
            json!({ "error": "unauthenticated" })
        )
    );

    let (code, body) = s
        .json(
            Method::POST,
            "/api/auth/login",
            Some(json!({ "password": "wrong password" })),
        )
        .await;
    assert_eq!(
        (code, body),
        (
            StatusCode::UNAUTHORIZED,
            json!({ "error": "invalid_password" })
        )
    );

    s.call(
        Method::POST,
        "/api/auth/login",
        Some(json!({ "password": "correct horse battery" })),
    )
    .await;
    assert_eq!(
        s.call(Method::GET, "/api/conversations", None).await.0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn conversation_crud_over_http() {
    let mut s = TestServer::start().await;
    s.call(
        Method::POST,
        "/api/auth/setup",
        Some(json!({ "password": "correct horse battery" })),
    )
    .await;

    let (code, created) = s
        .json(Method::POST, "/api/conversations", Some(json!({})))
        .await;
    assert_eq!(
        (code, created["title"].as_str()),
        (StatusCode::CREATED, Some("新会话"))
    );
    let id = created["id"].as_str().unwrap().to_owned();

    let path = format!("/api/conversations/{id}");
    let (code, renamed) = s
        .json(Method::PATCH, &path, Some(json!({ "title": "旅行计划" })))
        .await;
    assert_eq!(
        (code, renamed["title"].as_str()),
        (StatusCode::OK, Some("旅行计划"))
    );

    let (_, list) = s.json(Method::GET, "/api/conversations", None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    let (code, msgs) = s.json(Method::GET, &format!("{path}/messages"), None).await;
    assert_eq!((code, msgs), (StatusCode::OK, json!([])));
    assert_eq!(list[0]["title"], "旅行计划");

    assert_eq!(
        s.call(Method::DELETE, &path, None).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        s.call(Method::DELETE, &path, None).await.0,
        StatusCode::NOT_FOUND
    );
    let (_, list) = s.json(Method::GET, "/api/conversations", None).await;
    assert_eq!(list, json!([]));
}

#[tokio::test]
async fn account_credentials_over_http_never_return_the_key() {
    let mut s = TestServer::start().await;
    assert_eq!(
        s.call(Method::GET, "/api/settings/credentials", None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    s.call(
        Method::POST,
        "/api/auth/setup",
        Some(json!({ "password": "correct horse battery" })),
    )
    .await;

    let (code, body) = s
        .json(
            Method::POST,
            "/api/settings/credentials",
            Some(json!({ "kind": "openai-compatible", "label": "Ollama", "apiKey": "k" })),
        )
        .await;
    assert_eq!(
        (code, body),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({ "error": "invalid_base_url" })
        )
    );

    let (code, created) = s
        .json(
            Method::POST,
            "/api/settings/credentials",
            Some(json!({ "kind": "openai", "label": "OpenAI", "apiKey": "sk-abcdef9876" })),
        )
        .await;
    assert_eq!(code, StatusCode::CREATED);
    assert_eq!(created["keyHint"], "…9876");
    let id = created["id"].as_str().unwrap().to_owned();

    let (_, raw) = s.call(Method::GET, "/api/settings/credentials", None).await;
    assert!(!raw.contains("sk-abcdef"));

    let (_, model) = s
        .json(Method::GET, "/api/settings/default-model", None)
        .await;
    assert_eq!(model, Value::Null);
    let put = json!({ "credentialId": id, "modelId": "gpt-5" });
    assert_eq!(
        s.call(
            Method::PUT,
            "/api/settings/default-model",
            Some(put.clone())
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let (_, model) = s
        .json(Method::GET, "/api/settings/default-model", None)
        .await;
    assert_eq!(model, put);

    let path = format!("/api/settings/credentials/{id}");
    assert_eq!(
        s.call(Method::DELETE, &path, None).await.0,
        StatusCode::NO_CONTENT
    );
    let (_, model) = s
        .json(Method::GET, "/api/settings/default-model", None)
        .await;
    assert_eq!(model, Value::Null);
}

#[tokio::test]
async fn chat_streams_over_sse_and_persists() {
    let mut s = TestServer::start().await;
    s.call(
        Method::POST,
        "/api/auth/setup",
        Some(json!({ "password": "correct horse battery" })),
    )
    .await;
    let (_, conv) = s
        .json(Method::POST, "/api/conversations", Some(json!({})))
        .await;
    let path = format!(
        "/api/conversations/{}/messages",
        conv["id"].as_str().unwrap()
    );

    // 没配置默认模型：明确报错，不存消息。
    let (code, body) = s
        .json(Method::POST, &path, Some(json!({ "text": "你好" })))
        .await;
    assert_eq!(
        (code, body),
        (StatusCode::CONFLICT, json!({ "error": "no_default_model" }))
    );

    let (_, cred) = s
        .json(
            Method::POST,
            "/api/settings/credentials",
            Some(json!({ "kind": "openai", "label": "OpenAI", "apiKey": "sk-1234" })),
        )
        .await;
    s.call(
        Method::PUT,
        "/api/settings/default-model",
        Some(json!({ "credentialId": cred["id"], "modelId": "gpt-x" })),
    )
    .await;

    let (code, sse) = s
        .call(Method::POST, &path, Some(json!({ "text": "你好" })))
        .await;
    assert_eq!(code, StatusCode::OK);
    let names: Vec<&str> = sse
        .lines()
        .filter_map(|l| l.strip_prefix("event: "))
        .collect();
    assert_eq!(names, ["user", "delta", "delta", "done"]);
    assert!(sse.contains("1 条消息，模型 gpt-x"));

    let (_, msgs) = s.json(Method::GET, &path, None).await;
    let msgs = msgs.as_array().unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[1]["role"], "assistant");
    assert_eq!(msgs[1]["outputTokens"], 5);
}

#[tokio::test]
async fn totp_over_http_requires_code_after_enabling() {
    let mut s = TestServer::start().await;
    s.call(
        Method::POST,
        "/api/auth/setup",
        Some(json!({ "password": "correct horse battery" })),
    )
    .await;
    let (_, st) = s.json(Method::GET, "/api/auth/totp", None).await;
    assert_eq!(st, json!({ "enabled": false }));
    let (code, setup) = s.json(Method::POST, "/api/auth/totp/setup", None).await;
    assert_eq!(code, StatusCode::OK);
    assert!(
        setup["uri"]
            .as_str()
            .unwrap()
            .starts_with("otpauth://totp/")
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let otp = ownmind_auth::api::totp_code_at(setup["secret"].as_str().unwrap(), now).unwrap();
    let (code, _) = s
        .call(
            Method::POST,
            "/api/auth/totp/confirm",
            Some(json!({ "code": otp })),
        )
        .await;
    assert_eq!(code, StatusCode::NO_CONTENT);

    s.call(Method::POST, "/api/auth/logout", None).await;
    let pw = json!({ "password": "correct horse battery" });
    let (_, body) = s.json(Method::POST, "/api/auth/login", Some(pw)).await;
    assert_eq!(body, json!({ "error": "totp_required" }));
    let (_, body) = s
        .json(
            Method::POST,
            "/api/auth/login",
            Some(json!({ "password": "correct horse battery", "code": otp })),
        )
        .await;
    assert_eq!(body, json!({ "error": "invalid_totp" }));
}

#[tokio::test]
async fn login_is_throttled_with_429_and_retry_after() {
    let mut s = TestServer::start().await;
    s.call(
        Method::POST,
        "/api/auth/setup",
        Some(json!({ "password": "correct horse battery" })),
    )
    .await;
    s.call(Method::POST, "/api/auth/logout", None).await;
    for _ in 0..5 {
        s.call(
            Method::POST,
            "/api/auth/login",
            Some(json!({ "password": "wrong password" })),
        )
        .await;
    }
    let (code, body) = s
        .json(
            Method::POST,
            "/api/auth/login",
            Some(json!({ "password": "correct horse battery" })),
        )
        .await;
    assert_eq!(code, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["error"], "locked");
    assert!(body["retryAfter"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn profile_and_system_settings_over_http() {
    let mut s = TestServer::start().await;
    assert_eq!(
        s.call(Method::GET, "/api/settings/profile", None).await.0,
        StatusCode::UNAUTHORIZED
    );
    s.call(
        Method::POST,
        "/api/auth/setup",
        Some(json!({ "password": "correct horse battery" })),
    )
    .await;
    let (_, p) = s.json(Method::GET, "/api/settings/profile", None).await;
    assert_eq!(p, json!({ "name": "", "preferredName": "", "bio": "" }));
    let (code, p) = s
        .json(
            Method::PUT,
            "/api/settings/profile",
            Some(json!({ "name": "Ada", "preferredName": "A", "bio": "" })),
        )
        .await;
    assert_eq!((code, p["name"].clone()), (StatusCode::OK, json!("Ada")));
    let (code, body) = s
        .json(
            Method::PUT,
            "/api/settings/system",
            Some(json!({ "language": "en", "timezone": "bad zone!" })),
        )
        .await;
    assert_eq!(
        (code, body),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({ "error": "invalid_timezone" })
        )
    );
    let (_, sys) = s
        .json(
            Method::PUT,
            "/api/settings/system",
            Some(json!({ "language": "en", "timezone": "Europe/Paris" })),
        )
        .await;
    assert_eq!(sys, json!({ "language": "en", "timezone": "Europe/Paris" }));
}

#[tokio::test]
async fn schedule_tasks_crud_requires_login_and_validates() {
    let mut s = TestServer::start().await;
    s.call(
        Method::POST,
        "/api/auth/setup",
        Some(json!({ "password": "correct horse battery" })),
    )
    .await;
    let task = json!({
        "name": "日报", "prompt": "总结新闻",
        "schedule": { "kind": "interval", "everySecs": 3600 }
    });
    let (code, created) = s
        .json(Method::POST, "/api/schedule/tasks", Some(task.clone()))
        .await;
    assert_eq!(code, StatusCode::CREATED);
    assert_eq!(created["schedule"]["everySecs"], 3600);
    assert_eq!(created["enabled"], true);
    let id = created["id"].as_str().unwrap().to_owned();

    let (code, list) = s.json(Method::GET, "/api/schedule/tasks", None).await;
    assert_eq!((code, list.as_array().unwrap().len()), (StatusCode::OK, 1));

    let (code, upd) = s
        .json(
            Method::PATCH,
            &format!("/api/schedule/tasks/{id}"),
            Some(json!({ "enabled": false, "name": "周报" })),
        )
        .await;
    assert_eq!(
        (code, upd["enabled"].clone(), upd["name"].clone()),
        (StatusCode::OK, json!(false), json!("周报"))
    );

    let (code, fires) = s
        .json(
            Method::GET,
            &format!("/api/schedule/tasks/{id}/fires"),
            None,
        )
        .await;
    assert_eq!((code, fires), (StatusCode::OK, json!([])));

    let bad =
        json!({ "name": "x", "prompt": "y", "schedule": { "kind": "interval", "everySecs": 5 } });
    let (code, body) = s.json(Method::POST, "/api/schedule/tasks", Some(bad)).await;
    assert_eq!(
        (code, body),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            json!({ "error": "invalid_interval" })
        )
    );

    let (code, _) = s
        .json(Method::DELETE, &format!("/api/schedule/tasks/{id}"), None)
        .await;
    assert_eq!(code, StatusCode::NO_CONTENT);
    let (code, _) = s
        .json(Method::GET, &format!("/api/schedule/tasks/{id}"), None)
        .await;
    assert_eq!(code, StatusCode::NOT_FOUND);

    s.call(Method::POST, "/api/auth/logout", None).await;
    let (code, _) = s.json(Method::GET, "/api/schedule/tasks", None).await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn memory_management_over_http() {
    let mut s = TestServer::start().await;
    let (code, _) = s.json(Method::GET, "/api/memory", None).await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
    s.call(
        Method::POST,
        "/api/auth/setup",
        Some(json!({ "password": "correct horse battery" })),
    )
    .await;
    let (code, m) = s.json(Method::GET, "/api/memory", None).await;
    assert_eq!((code, m["core"].clone()), (StatusCode::OK, json!("")));

    let (code, _) = s
        .json(
            Method::PUT,
            "/api/memory/core",
            Some(json!({ "base": "", "content": "喜欢咖啡" })),
        )
        .await;
    assert_eq!(code, StatusCode::NO_CONTENT);
    // 基线过期 → 409
    let (code, body) = s
        .json(
            Method::PUT,
            "/api/memory/core",
            Some(json!({ "base": "", "content": "别的" })),
        )
        .await;
    assert_eq!(
        (code, body["error"].clone()),
        (StatusCode::CONFLICT, json!("memory_conflict"))
    );

    let (code, e) = s
        .json(
            Method::POST,
            "/api/memory/entries",
            Some(json!({ "content": "生日 3 月 1 日" })),
        )
        .await;
    assert_eq!(code, StatusCode::CREATED);
    let id = e["id"].as_str().unwrap().to_owned();
    let (_, m) = s.json(Method::GET, "/api/memory", None).await;
    assert_eq!(m["entries"].as_array().unwrap().len(), 1);
    let (code, _) = s
        .json(Method::DELETE, &format!("/api/memory/entries/{id}"), None)
        .await;
    assert_eq!(code, StatusCode::NO_CONTENT);
    let (code, _) = s
        .json(Method::DELETE, &format!("/api/memory/entries/{id}"), None)
        .await;
    assert_eq!(code, StatusCode::NOT_FOUND);

    let (code, sk) = s.json(Method::GET, "/api/skills", None).await;
    assert_eq!((code, sk), (StatusCode::OK, json!([])));
}
