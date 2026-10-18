//! sandboxd 接口测试：鉴权、超时、输出截断、路径逃逸、读写编辑。

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use ownmind_sandboxd::{MAX_OUTPUT, Sandbox, router};
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "test-token";

fn app(dir: &tempfile::TempDir) -> axum::Router {
    router(Sandbox::new(dir.path().to_path_buf(), TOKEN.into()).unwrap())
}

async fn call(
    app: &axum::Router,
    path: &str,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut req = Request::post(path).header("content-type", "application/json");
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let res = app
        .clone()
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn ok(app: &axum::Router, path: &str, body: Value) -> Value {
    let (s, v) = call(app, path, Some(TOKEN), body).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v
}

#[tokio::test]
async fn rejects_missing_or_wrong_token() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(&dir);
    for token in [None, Some("wrong"), Some("")] {
        let (s, _) = call(&app, "/exec", token, json!({"command": "echo hi"})).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        let (s, _) = call(&app, "/read", token, json!({"path": "a"})).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
    }
    let res = app
        .clone()
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn exec_runs_in_workdir_and_returns_exit_code() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(&dir);
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    let v = ok(
        &app,
        "/exec",
        json!({"command": "pwd; echo err >&2; exit 3", "cwd": "sub"}),
    )
    .await;
    assert_eq!(v["exitCode"], 3);
    assert!(v["stdout"].as_str().unwrap().trim_end().ends_with("/sub"));
    assert_eq!(v["stderr"], "err\n");
    assert_eq!(v["timedOut"], false);
    let (s, _) = call(
        &app,
        "/exec",
        Some(TOKEN),
        json!({"command": "ls", "cwd": "../"}),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn exec_timeout_kills_process_group() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(&dir);
    let started = std::time::Instant::now();
    // 后台子进程也要被杀：否则它写的标记文件会在 3 秒后出现
    let v = ok(
        &app,
        "/exec",
        json!({"command": "(sleep 3; touch leaked) & sleep 30", "timeoutSecs": 1}),
    )
    .await;
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    assert_eq!(v["timedOut"], true);
    assert!(v["exitCode"].is_null());
    tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    assert!(!dir.path().join("leaked").exists());
}

#[tokio::test]
async fn exec_output_is_capped() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(&dir);
    let v = ok(
        &app,
        "/exec",
        json!({"command": "head -c 3000000 /dev/zero | tr '\\0' a; echo short >&2"}),
    )
    .await;
    assert_eq!(v["exitCode"], 0);
    assert_eq!(v["stdout"].as_str().unwrap().len(), MAX_OUTPUT);
    assert_eq!(v["stdoutTruncated"], true);
    assert_eq!(v["stderr"], "short\n");
    assert_eq!(v["stderrTruncated"], false);
}

#[tokio::test]
async fn write_read_edit_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let app = app(&dir);
    ok(
        &app,
        "/write",
        json!({"path": "notes/a.txt", "content": "hello world"}),
    )
    .await;
    let v = ok(&app, "/read", json!({"path": "notes/a.txt"})).await;
    assert_eq!(v["content"], "hello world");
    assert_eq!(v["size"], 11);
    ok(
        &app,
        "/edit",
        json!({"path": "notes/a.txt", "oldString": "world", "newString": "ownmind"}),
    )
    .await;
    assert_eq!(
        std::fs::read_to_string(dir.path().join("notes/a.txt")).unwrap(),
        "hello ownmind"
    );

    ok(&app, "/write", json!({"path": "dup.txt", "content": "x x"})).await;
    let (s, v) = call(
        &app,
        "/edit",
        Some(TOKEN),
        json!({"path": "dup.txt", "oldString": "x", "newString": "y"}),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{v}");
    let (s, _) = call(
        &app,
        "/edit",
        Some(TOKEN),
        json!({"path": "dup.txt", "oldString": "z", "newString": "y"}),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(&app, "/read", Some(TOKEN), json!({"path": "missing.txt"})).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rejects_path_escape() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), "s3cret").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("secret"), root.path().join("file-link"))
        .unwrap();
    std::os::unix::fs::symlink("/nonexistent/x", root.path().join("dangling")).unwrap();
    let app = app(&root);

    for path in [
        "../x",
        "a/../../x",
        "/etc/passwd",
        "link/secret",
        "file-link",
        "link/new.txt",
        "dangling",
    ] {
        let (s, v) = call(&app, "/read", Some(TOKEN), json!({"path": path})).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "read {path}: {v}");
        let (s, v) = call(
            &app,
            "/write",
            Some(TOKEN),
            json!({"path": path, "content": "pwn"}),
        )
        .await;
        assert_eq!(s, StatusCode::FORBIDDEN, "write {path}: {v}");
        let (s, _) = call(
            &app,
            "/edit",
            Some(TOKEN),
            json!({"path": path, "oldString": "s3cret", "newString": "pwn"}),
        )
        .await;
        assert_eq!(s, StatusCode::FORBIDDEN, "edit {path}");
    }
    assert_eq!(
        std::fs::read_to_string(outside.path().join("secret")).unwrap(),
        "s3cret"
    );
    assert!(!outside.path().join("new.txt").exists());
    assert!(!std::path::Path::new("/nonexistent/x").exists());
}
