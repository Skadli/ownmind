use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use ownmind_server::{Config, router};
use tower::ServiceExt;

fn config_with_web(dir: &std::path::Path) -> Config {
    Config {
        addr: "127.0.0.1:0".parse().unwrap(),
        web_dir: dir.to_path_buf(),
        data_dir: dir.join("data"),
    }
}

async fn get(config: Config, path: &str) -> (StatusCode, String) {
    let res = router(config)
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test]
async fn healthz_reports_ok() {
    let web = tempfile::tempdir().unwrap();
    let (status, body) = get(config_with_web(web.path()), "/healthz").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, r#"{"status":"ok"}"#);
}

#[tokio::test]
async fn serves_web_assets_and_falls_back_to_index() {
    let web = tempfile::tempdir().unwrap();
    std::fs::write(web.path().join("index.html"), "<p>ownmind</p>").unwrap();
    std::fs::write(web.path().join("app.js"), "console.log(1)").unwrap();

    let (status, body) = get(config_with_web(web.path()), "/app.js").await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "console.log(1)"));

    let (status, body) = get(config_with_web(web.path()), "/conversations/42").await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "<p>ownmind</p>"));
}
