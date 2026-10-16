//! M1 验收（票 14）：真实 agent 进程 + faux 模型连续聊 100 轮，压缩要触发、不能报错。
//! 需要先构建 agent：`pnpm -C agent build`（CI 在 cargo test 之前构建）。

use ownmind_runtime::api::ChatEvent;
use ownmind_server::{App, migrate, open_db};
use ownmind_settings::api::{CredentialInput, DefaultModel, ProviderKind};

fn real_agent_with_faux() -> Vec<String> {
    let entry = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../agent/dist/index.js");
    assert!(
        entry.exists(),
        "缺少 {}，先运行 pnpm -C agent build",
        entry.display()
    );
    vec![
        "env".into(),
        "OWNMIND_TEST_FAUX=1".into(),
        // 窗口很小，几轮就会触发压缩。
        "OWNMIND_TEST_FAUX_WINDOW=1500".into(),
        "node".into(),
        entry.display().to_string(),
    ]
}

#[tokio::test]
async fn hundred_turns_with_compaction() {
    let dir = tempfile::tempdir().unwrap();
    let pool = open_db(dir.path()).await.unwrap();
    migrate(&pool).await.unwrap();
    let app = App::new(
        pool,
        ownmind_kernel::MasterKey::random(),
        real_agent_with_faux(),
    );
    let cred = app
        .settings
        .create_credential(CredentialInput {
            kind: ProviderKind::Openai,
            label: "faux".into(),
            base_url: None,
            api_key: Some("unused".into()),
        })
        .await
        .unwrap();
    app.settings
        .set_default_model(DefaultModel {
            credential_id: cred.id,
            model_id: "faux".into(),
        })
        .await
        .unwrap();
    let conv = app.conversations.create(None).await.unwrap();

    for i in 0..100 {
        let text = format!("第 {i} 轮：{}", "说点什么。".repeat(10));
        let mut rx = app.runtime.send(&conv.id, &text).await.unwrap();
        let last = loop {
            let ev = rx.recv().await.unwrap();
            if ev.is_final() {
                break ev;
            }
        };
        match last {
            ChatEvent::Done { message } => {
                assert!(
                    message.text.starts_with(&format!("faux: 第 {i} 轮")),
                    "{}",
                    message.text
                );
            }
            other => panic!("第 {i} 轮失败：{other:?}"),
        }
        while app.runtime.is_running(&conv.id).await {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }

    assert_eq!(
        app.conversations.messages(&conv.id).await.unwrap().len(),
        200
    );
    let c = app
        .conversations
        .latest_compaction(&conv.id)
        .await
        .unwrap()
        .expect("应当触发过压缩");
    assert!(c.upto_seq > 100, "压缩应持续推进，实际到 {}", c.upto_seq);
}
