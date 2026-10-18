use ownmind_conversation::api::{ConversationError, Conversations, NewMessage, Role};
use sqlx::SqlitePool;

async fn pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}?mode=rwc", dir.path().join("t.db").display());
    let pool = SqlitePool::connect(&url).await.unwrap();
    ownmind_conversation::api::MIGRATOR
        .run(&pool)
        .await
        .unwrap();
    (dir, pool)
}

#[tokio::test]
async fn create_list_rename_delete() {
    let (_d, pool) = pool().await;
    let convs = Conversations::new(pool);

    let first = convs.create(None).await.unwrap();
    assert_eq!(first.title, "新会话");
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let second = convs.create(Some("周报")).await.unwrap();

    let titles: Vec<_> = convs
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|c| c.title)
        .collect();
    assert_eq!(titles, ["周报", "新会话"], "最近更新的排前面");

    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let renamed = convs.rename(&first.id, "旅行计划").await.unwrap();
    assert_eq!(renamed.title, "旅行计划");
    assert_eq!(convs.list().await.unwrap()[0].id, first.id, "改名算更新");

    convs.delete(&second.id).await.unwrap();
    let ids: Vec<_> = convs
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(ids, [first.id]);
}

#[tokio::test]
async fn missing_conversation_is_not_found() {
    let (_d, pool) = pool().await;
    let convs = Conversations::new(pool);
    assert!(matches!(
        convs.rename("nope", "x").await,
        Err(ConversationError::NotFound)
    ));
    assert!(matches!(
        convs.delete("nope").await,
        Err(ConversationError::NotFound)
    ));
}

#[tokio::test]
async fn blank_title_is_rejected() {
    let (_d, pool) = pool().await;
    let convs = Conversations::new(pool);
    let c = convs.create(None).await.unwrap();
    assert!(matches!(
        convs.rename(&c.id, "   ").await,
        Err(ConversationError::InvalidTitle)
    ));
}

fn text(role: Role, s: &str) -> NewMessage {
    NewMessage {
        role,
        body: serde_json::json!({ "role": "user", "content": s }),
        text: s.into(),
        input_tokens: None,
        output_tokens: Some(3),
    }
}

#[tokio::test]
async fn messages_are_ordered_and_deleted_with_the_conversation() {
    let (_d, pool) = pool().await;
    let convs = Conversations::new(pool.clone());
    let c = convs.create(None).await.unwrap();
    let other = convs.create(None).await.unwrap();

    let a = convs.append(&c.id, text(Role::User, "你好")).await.unwrap();
    let b = convs
        .append(&c.id, text(Role::Assistant, "你好！"))
        .await
        .unwrap();
    convs
        .append(&other.id, text(Role::User, "x"))
        .await
        .unwrap();
    assert_eq!((a.seq, b.seq), (1, 2));
    assert_eq!(b.output_tokens, Some(3));
    assert_eq!(a.body["content"], "你好");
    assert!(convs.get(&c.id).await.unwrap().updated_at >= b.created_at);

    let list = convs.messages(&c.id).await.unwrap();
    let roles: Vec<_> = list.iter().map(|m| (m.role, m.text.as_str())).collect();
    assert_eq!(roles, [(Role::User, "你好"), (Role::Assistant, "你好！")]);

    convs.delete(&c.id).await.unwrap();
    assert!(matches!(
        convs.messages(&c.id).await,
        Err(ConversationError::NotFound)
    ));
    assert!(matches!(
        convs.append(&c.id, text(Role::User, "x")).await,
        Err(ConversationError::NotFound)
    ));
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conv_message")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 1);
}
