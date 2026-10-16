use ownmind_kernel::MasterKey;
use ownmind_settings::api::{
    CredentialInput, DefaultModel, Language, Profile, ProviderKind, Settings, SettingsError,
    SystemPrefs,
};
use sqlx::SqlitePool;

async fn settings() -> (tempfile::TempDir, SqlitePool, Settings) {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}?mode=rwc", dir.path().join("t.db").display());
    let pool = SqlitePool::connect(&url).await.unwrap();
    ownmind_settings::api::MIGRATOR.run(&pool).await.unwrap();
    let s = Settings::new(pool.clone(), MasterKey::random());
    (dir, pool, s)
}

fn input(kind: ProviderKind, base_url: Option<&str>, key: Option<&str>) -> CredentialInput {
    CredentialInput {
        kind,
        label: "我的".into(),
        base_url: base_url.map(Into::into),
        api_key: key.map(Into::into),
    }
}

#[tokio::test]
async fn key_is_encrypted_and_only_hint_is_public() {
    let (_d, pool, s) = settings().await;
    let c = s
        .create_credential(input(ProviderKind::Openai, None, Some("sk-secret-1234")))
        .await
        .unwrap();
    assert_eq!(c.key_hint, "…1234");
    let blob: Vec<u8> = sqlx::query_scalar("SELECT key_ciphertext FROM settings_credential")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!blob.windows(9).any(|w| w == b"sk-secret"));

    s.set_default_model(DefaultModel {
        credential_id: c.id.clone(),
        model_id: " gpt-5 ".into(),
    })
    .await
    .unwrap();
    let r = s.resolve_default_model().await.unwrap();
    assert_eq!(r.api_key, "sk-secret-1234");
    assert_eq!(r.model_id, "gpt-5");
    assert!(!format!("{r:?}").contains("sk-secret"));
}

#[tokio::test]
async fn compatible_kinds_need_a_url_and_new_credentials_need_a_key() {
    let (_d, _p, s) = settings().await;
    let no_url = s
        .create_credential(input(ProviderKind::OpenaiCompatible, None, Some("k")))
        .await;
    assert!(matches!(no_url, Err(SettingsError::InvalidBaseUrl)));
    let bad_url = s
        .create_credential(input(ProviderKind::Openai, Some("ftp://x"), Some("k")))
        .await;
    assert!(matches!(bad_url, Err(SettingsError::InvalidBaseUrl)));
    let no_key = s
        .create_credential(input(ProviderKind::Anthropic, None, Some("  ")))
        .await;
    assert!(matches!(no_key, Err(SettingsError::MissingKey)));
    let ok = s
        .create_credential(input(
            ProviderKind::OpenaiCompatible,
            Some("http://localhost:11434/v1/"),
            Some("ollama"),
        ))
        .await
        .unwrap();
    assert_eq!(ok.base_url.as_deref(), Some("http://localhost:11434/v1"));
}

#[tokio::test]
async fn update_keeps_key_when_blank_and_delete_clears_default() {
    let (_d, _p, s) = settings().await;
    let c = s
        .create_credential(input(ProviderKind::Deepseek, None, Some("key-aaaa")))
        .await
        .unwrap();
    let u = s
        .update_credential(&c.id, input(ProviderKind::Deepseek, None, None))
        .await
        .unwrap();
    assert_eq!(u.key_hint, "…aaaa");
    s.set_default_model(DefaultModel {
        credential_id: c.id.clone(),
        model_id: "deepseek-chat".into(),
    })
    .await
    .unwrap();
    assert!(s.default_model().await.unwrap().is_some());

    s.delete_credential(&c.id).await.unwrap();
    assert!(s.default_model().await.unwrap().is_none());
    assert!(matches!(
        s.resolve_default_model().await,
        Err(SettingsError::NoDefaultModel)
    ));
    assert!(matches!(
        s.delete_credential(&c.id).await,
        Err(SettingsError::NotFound)
    ));
}

#[tokio::test]
async fn default_model_must_point_at_an_existing_credential() {
    let (_d, _p, s) = settings().await;
    let r = s
        .set_default_model(DefaultModel {
            credential_id: "nope".into(),
            model_id: "m".into(),
        })
        .await;
    assert!(matches!(r, Err(SettingsError::NotFound)));
}

#[tokio::test]
async fn profile_and_system_prefs_round_trip_with_validation() {
    let (_d, _pool, s) = settings().await;
    assert_eq!(s.profile().await.unwrap(), Profile::default());
    assert_eq!(s.system().await.unwrap().timezone, "UTC");

    let saved = s
        .set_profile(Profile {
            name: " 张三 ".into(),
            preferred_name: "老张".into(),
            bio: "做后端".into(),
        })
        .await
        .unwrap();
    assert_eq!(saved.name, "张三");
    assert_eq!(s.profile().await.unwrap(), saved);
    assert!(matches!(
        s.set_profile(Profile {
            bio: "x".repeat(2001),
            ..Profile::default()
        })
        .await,
        Err(SettingsError::InvalidProfile)
    ));

    let sys = SystemPrefs {
        language: Language::Zh,
        timezone: "Asia/Shanghai".into(),
    };
    assert_eq!(s.set_system(sys.clone()).await.unwrap(), sys);
    assert_eq!(s.system().await.unwrap(), sys);
    assert!(matches!(
        s.set_system(SystemPrefs {
            timezone: "Asia/Shanghai; DROP".into(),
            ..sys
        })
        .await,
        Err(SettingsError::InvalidTimezone)
    ));
}
