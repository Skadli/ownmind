use std::path::PathBuf;

use ownmind_conversation::api::{Conversations, NewMessage, Role};
use ownmind_kernel::MasterKey;
use ownmind_runtime::api::{
    AgentEvent, ChatEvent, Runtime, RuntimeError, Tool, ToolContext, ToolFuture, ToolOutput,
    ToolSpec, Usage,
};
use ownmind_settings::api::{CredentialInput, DefaultModel, ProviderKind, Settings};
use sqlx::SqlitePool;

struct Env {
    _dir: tempfile::TempDir,
    convs: Conversations,
    settings: Settings,
}

async fn env(model_id: &str) -> Env {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}?mode=rwc", dir.path().join("t.db").display());
    let pool = SqlitePool::connect(&url).await.unwrap();
    let mut all: Vec<_> = ownmind_conversation::api::MIGRATOR
        .iter()
        .chain(ownmind_settings::api::MIGRATOR.iter())
        .cloned()
        .collect();
    all.sort_by_key(|m| m.version);
    sqlx::migrate::Migrator::with_migrations(all)
        .run(&pool)
        .await
        .unwrap();
    let settings = Settings::new(pool.clone(), MasterKey::random());
    let c = settings
        .create_credential(CredentialInput {
            kind: ProviderKind::Openai,
            label: "o".into(),
            base_url: None,
            api_key: Some("sk-x".into()),
        })
        .await
        .unwrap();
    settings
        .set_default_model(DefaultModel {
            credential_id: c.id,
            model_id: model_id.into(),
        })
        .await
        .unwrap();
    Env {
        _dir: dir,
        convs: Conversations::new(pool),
        settings,
    }
}

fn fake_cmd() -> Vec<String> {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fake_agent.mjs");
    vec!["node".into(), script.display().to_string()]
}

fn with_env(var: &str, val: &str) -> Vec<String> {
    let mut cmd = vec!["env".into(), format!("{var}={val}")];
    cmd.extend(fake_cmd());
    cmd
}

async fn collect(mut run: ownmind_runtime::api::Run) -> Vec<AgentEvent> {
    let mut out = vec![];
    while let Some(e) = run.events.recv().await {
        out.push(e);
    }
    out
}

#[tokio::test]
async fn run_streams_events_and_agent_pulls_context() {
    let e = env("gpt-test").await;
    let conv = e.convs.create(None).await.unwrap();
    e.convs
        .append(
            &conv.id,
            NewMessage {
                role: Role::User,
                body: serde_json::json!({ "role": "user", "content": "hi", "timestamp": 0 }),
                text: "hi".into(),
                input_tokens: None,
                output_tokens: None,
            },
        )
        .await
        .unwrap();
    let rt = Runtime::spawn(fake_cmd(), e.convs.clone(), e.settings.clone());

    let events = collect(rt.start_run(&conv.id).await.unwrap()).await;
    assert_eq!(
        events[..2],
        [
            AgentEvent::TextDelta {
                delta: "1 条消息，".into()
            },
            AgentEvent::TextDelta {
                delta: "模型 gpt-test".into()
            },
        ]
    );
    match &events[2] {
        AgentEvent::Done { text, usage, .. } => {
            assert_eq!(text, "1 条消息，模型 gpt-test");
            assert_eq!(
                usage,
                &Usage {
                    input: 10,
                    output: 5
                }
            );
        }
        other => panic!("期望 done，得到 {other:?}"),
    }
    assert_eq!(events.len(), 3);
}

#[tokio::test]
async fn cancel_ends_the_run_with_an_error() {
    let e = env("slow").await;
    let conv = e.convs.create(None).await.unwrap();
    let rt = Runtime::spawn(fake_cmd(), e.convs.clone(), e.settings.clone());
    let run = rt.start_run(&conv.id).await.unwrap();
    rt.cancel(&run.id).await.unwrap();
    let events = collect(run).await;
    assert!(matches!(&events[..], [AgentEvent::Error { .. }]));
}

#[tokio::test]
async fn crash_fails_the_run_and_the_agent_restarts() {
    let e = env("m").await;
    let conv = e.convs.create(None).await.unwrap();
    let rt = Runtime::spawn(
        with_env("FAKE_CRASH", "1"),
        e.convs.clone(),
        e.settings.clone(),
    );
    // 崩溃发生在 run.start 应答之前：请求以 AgentUnavailable 失败。
    assert!(matches!(
        rt.start_run(&conv.id).await,
        Err(RuntimeError::AgentUnavailable(_))
    ));
    // 重启后仍能握手并接收请求（再次崩溃，但说明进程被重新拉起且握手成功）。
    assert!(matches!(
        rt.start_run(&conv.id).await,
        Err(RuntimeError::AgentUnavailable(_))
    ));
}

#[tokio::test]
async fn protocol_version_mismatch_never_becomes_ready() {
    let e = env("m").await;
    let conv = e.convs.create(None).await.unwrap();
    let rt = Runtime::spawn(
        with_env("FAKE_VERSION", "99"),
        e.convs.clone(),
        e.settings.clone(),
    );
    let started = std::time::Instant::now();
    let r = rt.start_run(&conv.id).await;
    assert!(matches!(r, Err(RuntimeError::AgentUnavailable(m)) if m.contains("未就绪")));
    assert!(started.elapsed() >= std::time::Duration::from_secs(10));
}

#[tokio::test]
async fn missing_default_model_is_reported_before_contacting_agent() {
    let e = env("m").await;
    let creds = e.settings.list_credentials().await.unwrap();
    e.settings.delete_credential(&creds[0].id).await.unwrap();
    let rt = Runtime::spawn(fake_cmd(), e.convs.clone(), e.settings.clone());
    assert!(matches!(
        rt.start_run("x").await,
        Err(RuntimeError::Settings(_))
    ));
}

async fn drain(mut rx: tokio::sync::broadcast::Receiver<ChatEvent>) -> Vec<ChatEvent> {
    let mut out = vec![];
    while let Ok(e) = rx.recv().await {
        let fin = e.is_final();
        out.push(e);
        if fin {
            break;
        }
    }
    out
}

#[tokio::test]
async fn send_saves_both_messages_and_streams() {
    let e = env("m").await;
    let conv = e.convs.create(None).await.unwrap();
    let rt = Runtime::spawn(fake_cmd(), e.convs.clone(), e.settings.clone());
    let events = drain(rt.send(&conv.id, " 你好 ").await.unwrap()).await;
    assert!(matches!(&events[0], ChatEvent::User { message } if message.text == "你好"));
    assert!(matches!(&events[1], ChatEvent::Delta { .. }));
    let ChatEvent::Done { message } = events.last().unwrap() else {
        panic!("期望 done：{events:?}");
    };
    assert_eq!(message.text, "1 条消息，模型 m");
    assert_eq!(message.output_tokens, Some(5));
    assert_eq!(e.convs.messages(&conv.id).await.unwrap().len(), 2);
    // 运行结束后可以再发。
    while rt.is_running(&conv.id).await {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let events = drain(rt.send(&conv.id, "再来").await.unwrap()).await;
    assert!(
        matches!(events.last().unwrap(), ChatEvent::Done { message } if message.text.starts_with("3 条"))
    );
}

#[tokio::test]
async fn one_run_per_conversation_and_stop_works() {
    let e = env("slow").await;
    let conv = e.convs.create(None).await.unwrap();
    let rt = Runtime::spawn(fake_cmd(), e.convs.clone(), e.settings.clone());
    let rx = rt.send(&conv.id, "a").await.unwrap();
    assert!(matches!(
        rt.send(&conv.id, "b").await,
        Err(RuntimeError::Busy)
    ));
    rt.stop(&conv.id).await.unwrap();
    assert!(matches!(
        drain(rx).await.last(),
        Some(ChatEvent::Error { .. })
    ));
    assert!(matches!(
        rt.stop(&conv.id).await,
        Err(RuntimeError::NotRunning)
    ));
    // 只存了主人消息。
    assert_eq!(e.convs.messages(&conv.id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn send_rejects_empty_and_unconfigured() {
    let e = env("m").await;
    let conv = e.convs.create(None).await.unwrap();
    let rt = Runtime::spawn(fake_cmd(), e.convs.clone(), e.settings.clone());
    assert!(matches!(
        rt.send(&conv.id, "  ").await,
        Err(RuntimeError::EmptyMessage)
    ));
    let creds = e.settings.list_credentials().await.unwrap();
    e.settings.delete_credential(&creds[0].id).await.unwrap();
    assert!(matches!(
        rt.send(&conv.id, "hi").await,
        Err(RuntimeError::Settings(_))
    ));
    assert!(e.convs.messages(&conv.id).await.unwrap().is_empty());
}

#[tokio::test]
async fn long_conversations_are_compacted_keeping_six_turns() {
    let e = env("m").await;
    let conv = e.convs.create(None).await.unwrap();
    for i in 0..8 {
        let user = NewMessage {
            role: Role::User,
            body: serde_json::json!({ "role": "user", "content": format!("问题 {i}"), "timestamp": 0 }),
            text: format!("问题 {i} {}", "很长的内容".repeat(20)),
            input_tokens: None,
            output_tokens: None,
        };
        e.convs.append(&conv.id, user).await.unwrap();
        if i < 7 {
            let reply = NewMessage {
                role: Role::Assistant,
                body: serde_json::json!({ "role": "assistant", "content": [] }),
                text: "回答".repeat(30),
                input_tokens: None,
                output_tokens: None,
            };
            e.convs.append(&conv.id, reply).await.unwrap();
        }
    }
    let rt = Runtime::spawn(
        with_env("FAKE_WINDOW", "200"),
        e.convs.clone(),
        e.settings.clone(),
    );
    let events = collect(rt.start_run(&conv.id).await.unwrap()).await;
    // 8 轮里保留最近 6 轮：前 2 轮（seq 1–4）被摘要代替，发给模型 11 条。
    assert!(
        matches!(events.last().unwrap(), AgentEvent::Done { text, .. } if text.starts_with("11 条")),
        "{events:?}"
    );
    let c = e.convs.latest_compaction(&conv.id).await.unwrap().unwrap();
    assert_eq!((c.upto_seq, c.summary.as_str()), (4, "摘要至 4"));
    // 界面仍能拿到全部 15 条。
    assert_eq!(e.convs.messages(&conv.id).await.unwrap().len(), 15);
}

struct Echo;

impl Tool for Echo {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "echo".into(),
            description: "原样返回参数".into(),
            parameters: serde_json::json!({ "type": "object", "properties": {} }),
        }
    }

    fn call(&self, ctx: ToolContext, arguments: serde_json::Value) -> ToolFuture {
        Box::pin(async move {
            assert!(!ctx.conversation_id.is_empty());
            ToolOutput::ok(arguments.to_string())
        })
    }
}

static FOREVER_DROPPED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct Forever;

impl Tool for Forever {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "forever".into(),
            description: "永不返回".into(),
            parameters: serde_json::json!({ "type": "object" }),
        }
    }

    fn call(&self, _: ToolContext, _: serde_json::Value) -> ToolFuture {
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                FOREVER_DROPPED.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }
        Box::pin(async {
            let _g = Guard;
            std::future::pending::<ToolOutput>().await
        })
    }
}

fn tools() -> Vec<std::sync::Arc<dyn Tool>> {
    vec![std::sync::Arc::new(Echo), std::sync::Arc::new(Forever)]
}

#[tokio::test]
async fn tool_calls_run_and_every_message_is_saved_in_order() {
    let e = env("tool:echo").await;
    let conv = e.convs.create(None).await.unwrap();
    let rt = Runtime::spawn_with_tools(fake_cmd(), e.convs.clone(), e.settings.clone(), tools());
    let events = drain(rt.send(&conv.id, "用工具").await.unwrap()).await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ChatEvent::ToolStart { name, .. } if name == "echo"))
    );
    let ChatEvent::Done { message } = events.last().unwrap() else {
        panic!("{events:?}");
    };
    // 回放给模型时已包含 user、assistant(工具调用)、toolResult 三条。
    assert_eq!(message.text, r#"工具说：{"x":1}；工具 echo,forever；3 条"#);
    let roles: Vec<Role> = e
        .convs
        .messages(&conv.id)
        .await
        .unwrap()
        .iter()
        .map(|m| m.role)
        .collect();
    assert_eq!(
        roles,
        [Role::User, Role::Assistant, Role::Tool, Role::Assistant]
    );
}

#[tokio::test]
async fn unknown_tool_is_an_error_result_not_a_crash() {
    let e = env("tool:nope").await;
    let conv = e.convs.create(None).await.unwrap();
    let rt = Runtime::spawn_with_tools(fake_cmd(), e.convs.clone(), e.settings.clone(), tools());
    let events = drain(rt.send(&conv.id, "x").await.unwrap()).await;
    assert!(
        matches!(events.last().unwrap(), ChatEvent::Done { message } if message.text.starts_with("工具说：没有这个工具"))
    );
}

#[tokio::test]
async fn stopping_a_run_cancels_the_running_tool() {
    let e = env("tool:forever").await;
    let conv = e.convs.create(None).await.unwrap();
    let rt = Runtime::spawn_with_tools(fake_cmd(), e.convs.clone(), e.settings.clone(), tools());
    let mut rx = rt.send(&conv.id, "x").await.unwrap();
    loop {
        if let ChatEvent::ToolStart { .. } = rx.recv().await.unwrap() {
            break;
        }
    }
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    rt.stop(&conv.id).await.unwrap();
    let rest = drain(rx).await;
    assert!(
        matches!(rest.last(), Some(ChatEvent::Error { .. })),
        "{rest:?}"
    );
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(FOREVER_DROPPED.load(std::sync::atomic::Ordering::SeqCst));
}
