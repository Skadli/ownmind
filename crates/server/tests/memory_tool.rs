//! memory 工具：跨会话（A 记下，B 检索）、核心记忆读写与超限拒绝。

use std::sync::Arc;

use ownmind_memory::api::Memory;
use ownmind_runtime::api::{Tool, ToolContext, ToolOutput, memory_tool_set};
use serde_json::{Value, json};

async fn setup() -> (Vec<Arc<dyn Tool>>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}?mode=rwc", dir.path().join("t.db").display());
    let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
    ownmind_memory::api::MIGRATOR.run(&pool).await.unwrap();
    (memory_tool_set(Memory::new(pool)), dir)
}

async fn call(tools: &[Arc<dyn Tool>], conv: &str, args: Value) -> ToolOutput {
    tools[0]
        .call(
            ToolContext {
                run_id: "r".into(),
                conversation_id: conv.into(),
            },
            args,
        )
        .await
}

#[tokio::test]
async fn entries_are_shared_across_conversations() {
    let (t, _d) = setup().await;
    let r = call(&t, "A", json!({"action":"add","text":"主人对花生过敏"})).await;
    assert!(!r.is_error, "{}", r.content);
    let r = call(&t, "B", json!({"action":"search","query":"花生"})).await;
    assert!(!r.is_error);
    assert!(r.content.contains("主人对花生过敏"), "{}", r.content);
    let id = r.content[1..r.content.find(']').unwrap()].to_owned();
    assert!(
        !call(&t, "B", json!({"action":"delete","id":id}))
            .await
            .is_error
    );
    let r = call(&t, "A", json!({"action":"search","query":"花生"})).await;
    assert!(r.content.contains("没有"));
}

#[tokio::test]
async fn core_read_write_and_limit() {
    let (t, _d) = setup().await;
    assert!(
        call(&t, "A", json!({"action":"read_core"}))
            .await
            .content
            .contains("空")
    );
    assert!(
        !call(&t, "A", json!({"action":"write_core","text":"住在上海"}))
            .await
            .is_error
    );
    assert_eq!(
        call(&t, "B", json!({"action":"read_core"})).await.content,
        "住在上海"
    );
    let big = "字".repeat(2500);
    let r = call(&t, "A", json!({"action":"write_core","text":big})).await;
    assert!(r.is_error);
    assert_eq!(
        call(&t, "B", json!({"action":"read_core"})).await.content,
        "住在上海"
    );
    assert!(call(&t, "A", json!({"action":"nope"})).await.is_error);
    assert!(call(&t, "A", json!({"action":"add"})).await.is_error);
}
