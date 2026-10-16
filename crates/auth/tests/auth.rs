use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use ownmind_auth::api::{Auth, AuthError, SESSION_TTL_MS, totp_code_at};
use ownmind_kernel::MasterKey;
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
    let auth = Auth::new(pool, MasterKey::random());
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
    let auth = Auth::new(pool, MasterKey::random());
    assert!(matches!(
        auth.set_up("short", None).await,
        Err(AuthError::WeakPassword)
    ));
    assert!(!auth.is_set_up().await.unwrap());
}

#[tokio::test]
async fn login_checks_password_and_issues_working_token() {
    let (_d, pool) = pool().await;
    let auth = Auth::new(pool, MasterKey::random());
    assert!(matches!(
        auth.log_in(PASSWORD, None, None).await,
        Err(AuthError::NotSetUp)
    ));

    auth.set_up(PASSWORD, None).await.unwrap();
    assert!(matches!(
        auth.log_in("wrong password", None, None).await,
        Err(AuthError::InvalidPassword)
    ));

    let token = auth
        .log_in(PASSWORD, None, Some("test-agent"))
        .await
        .unwrap();
    auth.authenticate(&token).await.unwrap();
    assert!(matches!(
        auth.authenticate("forged").await,
        Err(AuthError::Unauthenticated)
    ));
}

#[tokio::test]
async fn logout_revokes_token() {
    let (_d, pool) = pool().await;
    let auth = Auth::new(pool, MasterKey::random());
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
    let auth = Auth::with_clock(
        pool,
        MasterKey::random(),
        Arc::new(move || clock.load(Ordering::SeqCst)),
    );
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

#[tokio::test]
async fn totp_gates_login_once_confirmed_and_codes_are_single_use() {
    let (_d, pool) = pool().await;
    let now = Arc::new(AtomicI64::new(1_800_000_000_000));
    let clock = now.clone();
    let auth = Auth::with_clock(
        pool,
        MasterKey::random(),
        Arc::new(move || clock.load(Ordering::SeqCst)),
    );
    auth.set_up(PASSWORD, None).await.unwrap();
    assert!(matches!(
        auth.totp_confirm("123456").await,
        Err(AuthError::TotpNotPending)
    ));

    let setup = auth.totp_begin().await.unwrap();
    assert!(setup.uri.contains(&setup.secret));
    // 未确认前不生效。
    assert!(!auth.totp_enabled().await.unwrap());
    auth.log_in(PASSWORD, None, None).await.unwrap();

    let t = now.load(Ordering::SeqCst);
    assert!(matches!(
        auth.totp_confirm("000000").await,
        Err(AuthError::InvalidTotp)
    ));
    let code = totp_code_at(&setup.secret, t).unwrap();
    auth.totp_confirm(&code).await.unwrap();
    assert!(auth.totp_enabled().await.unwrap());

    assert!(matches!(
        auth.log_in(PASSWORD, None, None).await,
        Err(AuthError::TotpRequired)
    ));
    // 确认用过的码不能再用来登录。
    assert!(matches!(
        auth.log_in(PASSWORD, Some(&code), None).await,
        Err(AuthError::InvalidTotp)
    ));
    // 下一步的码可以，用过一次后就不行了。
    now.fetch_add(30_000, Ordering::SeqCst);
    let next = totp_code_at(&setup.secret, t + 30_000).unwrap();
    assert!(matches!(
        auth.log_in("wrong password", Some(&next), None).await,
        Err(AuthError::InvalidPassword)
    ));
    auth.log_in(PASSWORD, Some(&next), None).await.unwrap();
    assert!(matches!(
        auth.log_in(PASSWORD, Some(&next), None).await,
        Err(AuthError::InvalidTotp)
    ));

    assert!(matches!(
        auth.totp_disable("wrong password").await,
        Err(AuthError::InvalidPassword)
    ));
    auth.totp_disable(PASSWORD).await.unwrap();
    auth.log_in(PASSWORD, None, None).await.unwrap();
}
