use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use ownmind_server::{App, migrate, open_db, router};
use serde_json::{Value, json};
use tower::ServiceExt;

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
        let router = router(App::new(pool), &web);
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
