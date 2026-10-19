//! 技能：预设索引进上下文、经 Git 链接安装（沙盒内 clone）、校验与卸载。

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

use ownmind_runtime::api::{Tool, ToolContext, ToolOutput, skill_tool_set};
use ownmind_sandbox::api::Sandbox;
use ownmind_skill::api::Skills;
use serde_json::{Value, json};

struct Env {
    tools: Vec<Arc<dyn Tool>>,
    skills: Skills,
    pool: sqlx::SqlitePool,
    _dirs: Vec<tempfile::TempDir>,
}

async fn setup() -> Env {
    let work = tempfile::tempdir().unwrap();
    let builtin = tempfile::tempdir().unwrap();
    let b = builtin.path().join("demo");
    std::fs::create_dir_all(&b).unwrap();
    std::fs::write(
        b.join("SKILL.md"),
        "---\nname: demo\ndescription: 演示技能\n---\n正文\n",
    )
    .unwrap();
    let sandboxd = ownmind_sandboxd::Sandbox::new(work.path().to_path_buf(), "tok".into()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, ownmind_sandboxd::router(sandboxd))
            .await
            .unwrap();
    });
    let db = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}?mode=rwc", db.path().join("t.db").display());
    let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
    ownmind_server::migrate(&pool).await.unwrap();
    let sb = Sandbox::new(&format!("http://{addr}"), "tok");
    let skills = Skills::new(pool.clone(), sb, builtin.path().to_str().unwrap());
    Env {
        tools: skill_tool_set(skills.clone()),
        skills,
        pool,
        _dirs: vec![work, builtin, db],
    }
}

async fn call(e: &Env, args: Value) -> ToolOutput {
    e.tools[0]
        .call(
            ToolContext {
                run_id: "r".into(),
                conversation_id: "c".into(),
            },
            args,
        )
        .await
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// 用 python 的静态 HTTP 服务提供一个 dumb-http Git 仓库。
struct Served {
    child: Child,
    url: String,
    _dir: tempfile::TempDir,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn serve_repo(skill_md: Option<&str>) -> Served {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("r.git-src");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("README.md"), "x").unwrap();
    if let Some(md) = skill_md {
        std::fs::write(repo.join("SKILL.md"), md).unwrap();
    }
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    let bare = dir.path().join("srv/r.git");
    std::fs::create_dir_all(dir.path().join("srv")).unwrap();
    git(
        dir.path(),
        &[
            "clone",
            "-q",
            "--bare",
            repo.to_str().unwrap(),
            bare.to_str().unwrap(),
        ],
    );
    git(&bare, &["update-server-info"]);
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let child = Command::new("python3")
        .args([
            "-m",
            "http.server",
            &port.to_string(),
            "--bind",
            "127.0.0.1",
        ])
        .current_dir(dir.path().join("srv"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(800));
    Served {
        child,
        url: format!("http://127.0.0.1:{port}/r.git"),
        _dir: dir,
    }
}

#[tokio::test]
async fn index_lists_builtin_and_rejects_bad_urls() {
    let e = setup().await;
    let idx = e.skills.index().await.unwrap();
    assert_eq!(idx.len(), 1);
    assert_eq!(
        (idx[0].name.as_str(), idx[0].source.as_str()),
        ("demo", "builtin")
    );
    assert!(
        call(&e, json!({"action":"list"}))
            .await
            .content
            .contains("演示技能")
    );
    for bad in ["file:///etc", "https://x.com/a;id", "ssh://git@x/y"] {
        let r = call(&e, json!({"action":"install","url":bad})).await;
        assert!(r.is_error, "{bad}");
    }
    assert!(call(&e, json!({"action":"install"})).await.is_error);
    assert!(call(&e, json!({"action":"nope"})).await.is_error);
    // 预设技能不能卸载。
    assert!(
        call(&e, json!({"action":"remove","name":"demo"}))
            .await
            .content
            .contains("没有")
    );
}

#[tokio::test]
async fn install_from_git_then_index_and_remove() {
    let e = setup().await;
    let s = serve_repo(Some("---\nname: hello-skill\ndescription: 打招呼\n---\n"));
    let r = call(&e, json!({"action":"install","url":s.url})).await;
    assert!(!r.is_error, "{}", r.content);
    let names: Vec<_> = e
        .skills
        .index()
        .await
        .unwrap()
        .into_iter()
        .map(|i| (i.name, i.source))
        .collect();
    assert!(
        names.contains(&("hello-skill".into(), "installed".into())),
        "{names:?}"
    );
    assert_eq!(e.skills.list_installed().await.unwrap().len(), 1);
    // 重装即升级，不产生重复登记。
    assert!(
        !call(&e, json!({"action":"install","url":s.url}))
            .await
            .is_error
    );
    let n: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM skill_installed")
        .fetch_one(&e.pool)
        .await
        .unwrap();
    assert_eq!(n.0, 1);
    assert!(
        call(&e, json!({"action":"remove","name":"hello-skill"}))
            .await
            .content
            .contains("已卸载")
    );
    assert_eq!(e.skills.index().await.unwrap().len(), 1);
}

#[tokio::test]
async fn install_requires_valid_skill_md() {
    let e = setup().await;
    let none = serve_repo(None);
    let r = call(&e, json!({"action":"install","url":none.url})).await;
    assert!(
        r.is_error && r.content.contains("SKILL.md"),
        "{}",
        r.content
    );
    let bad = serve_repo(Some("---\nname: Bad Name\ndescription: x\n---\n"));
    let r = call(&e, json!({"action":"install","url":bad.url})).await;
    assert!(r.is_error && r.content.contains("不合法"), "{}", r.content);
    assert!(e.skills.list_installed().await.unwrap().is_empty());
    // 失败不留临时目录。
    assert_eq!(e.skills.index().await.unwrap().len(), 1);
}
