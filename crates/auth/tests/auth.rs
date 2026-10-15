use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use ownmind_auth::api::{Auth, AuthError, SESSION_TTL_MS};
use sqlx::SqlitePool;

const PASSWORD: &str = "correct horse battery";

async fn pool() -> (tempfile::TempDir, SqlitePool) {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}?mode=rwc", dir.path().join("t.db").display());
    let pool = SqlitePool::connect(&url).await.unwrap();
    ownmind_auth::api::MIGRATOR.run(&pool).await.unwrap();
    (dir, pool)
}

#[tokio::test]
async fn owner_is_set_up_exactly_once() {
    let (_d, pool) = pool().await;
    let auth = Auth::new(pool);
    assert!(!auth.is_set_up().await.unwrap());

    auth.set_up(PASSWORD, None).await.unwrap();
    assert!(auth.is_set_up().await.unwrap());
    assert!(matches!(
        auth.set_up("another password", None).await,
        Err(AuthError::AlreadySetUp)
    ));
}

#[tokio::test]
async fn short_password_is_rejected() {
    let (_d, pool) = pool().await;
    let auth = Auth::new(pool);
    assert!(matches!(
        auth.set_up("short", None).await,
        Err(AuthError::WeakPassword)
    ));
    assert!(!auth.is_set_up().await.unwrap());
}

#[tokio::test]
async fn login_checks_password_and_issues_working_token() {
    let (_d, pool) = pool().await;
    let auth = Auth::new(pool);
    assert!(matches!(
        auth.log_in(PASSWORD, None).await,
        Err(AuthError::NotSetUp)
    ));

    auth.set_up(PASSWORD, None).await.unwrap();
    assert!(matches!(
        auth.log_in("wrong password", None).await,
        Err(AuthError::InvalidPassword)
    ));

    let token = auth.log_in(PASSWORD, Some("test-agent")).await.unwrap();
    auth.authenticate(&token).await.unwrap();
    assert!(matches!(
        auth.authenticate("forged").await,
        Err(AuthError::Unauthenticated)
    ));
}

#[tokio::test]
async fn logout_revokes_token() {
    let (_d, pool) = pool().await;
    let auth = Auth::new(pool);
    let token = auth.set_up(PASSWORD, None).await.unwrap();
    auth.log_out(&token).await.unwrap();
    assert!(matches!(
        auth.authenticate(&token).await,
        Err(AuthError::Unauthenticated)
    ));
}

#[tokio::test]
async fn session_expires_unless_used() {
    let (_d, pool) = pool().await;
    let now = Arc::new(AtomicI64::new(1_000_000));
    let clock = now.clone();
    let auth = Auth::with_clock(pool, Arc::new(move || clock.load(Ordering::SeqCst)));
    let token = auth.set_up(PASSWORD, None).await.unwrap();

    // 用过一次就续期：从使用时刻起再算 30 天
    now.fetch_add(SESSION_TTL_MS - 1, Ordering::SeqCst);
    auth.authenticate(&token).await.unwrap();
    now.fetch_add(SESSION_TTL_MS - 1, Ordering::SeqCst);
    auth.authenticate(&token).await.unwrap();

    now.fetch_add(SESSION_TTL_MS + 1, Ordering::SeqCst);
    assert!(matches!(
        auth.authenticate(&token).await,
        Err(AuthError::Unauthenticated)
    ));
}
