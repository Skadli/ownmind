use ownmind_conversation::api::{ConversationError, Conversations};
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
    let second = convs.create(Some("周报")).await.unwrap();

    let titles: Vec<_> = convs
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|c| c.title)
        .collect();
    assert_eq!(titles, ["周报", "新会话"], "最近更新的排前面");

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
