//! HTTP 层：路由、Cookie、请求校验与错误到状态码的映射。

use std::path::Path;

use axum::extract::{FromRequestParts, Path as UrlPath, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use ownmind_auth::api::{AuthError, SESSION_TTL_MS};
use ownmind_conversation::api::{Conversation, ConversationError, Message};
use ownmind_settings::api::{Credential, CredentialInput, DefaultModel, SettingsError};
use serde::Deserialize;
use serde_json::{Value, json};
use tower_http::services::{ServeDir, ServeFile};

use crate::App;

const SESSION_COOKIE: &str = "ownmind_session";

pub fn router(app: App, web_dir: &Path) -> Router {
    let web = ServeDir::new(web_dir).fallback(ServeFile::new(web_dir.join("index.html")));
    let api = Router::new()
        .route("/auth/status", get(auth_status))
        .route("/auth/setup", post(set_up))
        .route("/auth/login", post(log_in))
        .route("/auth/logout", post(log_out))
        .route(
            "/conversations",
            get(list_conversations).post(create_conversation),
        )
        .route(
            "/conversations/{id}",
            patch(rename_conversation).delete(delete_conversation),
        )
        .route("/conversations/{id}/messages", get(list_messages))
        .route(
            "/settings/credentials",
            get(list_credentials).post(create_credential),
        )
        .route(
            "/settings/credentials/{id}",
            patch(update_credential).delete(delete_credential),
        )
        .route(
            "/settings/default-model",
            get(get_default_model).put(put_default_model),
        )
        .fallback(|| async { ApiError(StatusCode::NOT_FOUND, "not_found") });
    Router::new()
        .route(
            "/healthz",
            get(|| async { Json(json!({ "status": "ok" })) }),
        )
        .nest("/api", api)
        .fallback_service(web)
        .with_state(app)
}

// ---------- 错误 ----------

/// 统一错误体：`{"error": "<code>"}`，code 供前端翻译。
struct ApiError(StatusCode, &'static str);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<AuthError> for ApiError {
    fn from(e: AuthError) -> Self {
        match e {
            AuthError::AlreadySetUp => Self(StatusCode::CONFLICT, "already_set_up"),
            AuthError::NotSetUp => Self(StatusCode::CONFLICT, "not_set_up"),
            AuthError::WeakPassword => Self(StatusCode::UNPROCESSABLE_ENTITY, "weak_password"),
            AuthError::InvalidPassword => Self(StatusCode::UNAUTHORIZED, "invalid_password"),
            AuthError::Unauthenticated => Self(StatusCode::UNAUTHORIZED, "unauthenticated"),
            AuthError::Db(e) => internal(e),
        }
    }
}

impl From<ConversationError> for ApiError {
    fn from(e: ConversationError) -> Self {
        match e {
            ConversationError::NotFound => Self(StatusCode::NOT_FOUND, "not_found"),
            ConversationError::InvalidTitle => {
                Self(StatusCode::UNPROCESSABLE_ENTITY, "invalid_title")
            }
            ConversationError::Db(e) => internal(e),
        }
    }
}

impl From<SettingsError> for ApiError {
    fn from(e: SettingsError) -> Self {
        let unprocessable = |code| Self(StatusCode::UNPROCESSABLE_ENTITY, code);
        match e {
            SettingsError::NotFound => Self(StatusCode::NOT_FOUND, "not_found"),
            SettingsError::InvalidLabel => unprocessable("invalid_label"),
            SettingsError::InvalidBaseUrl => unprocessable("invalid_base_url"),
            SettingsError::MissingKey => unprocessable("missing_key"),
            SettingsError::InvalidModel => unprocessable("invalid_model"),
            SettingsError::NoDefaultModel => Self(StatusCode::CONFLICT, "no_default_model"),
            SettingsError::Secret(e) => internal(e),
            SettingsError::Db(e) => internal(e),
        }
    }
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    tracing::error!(error = %e, "internal error");
    ApiError(StatusCode::INTERNAL_SERVER_ERROR, "internal")
}

// ---------- 登录 ----------

/// 已登录主人。放在 handler 参数里即要求登录。
struct Owner;

impl FromRequestParts<App> for Owner {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, app: &App) -> Result<Self, ApiError> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(SESSION_COOKIE)
            .ok_or(ApiError(StatusCode::UNAUTHORIZED, "unauthenticated"))?;
        app.auth.authenticate(token.value()).await?;
        Ok(Owner)
    }
}

fn session_cookie(token: String) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, token))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Strict)
        .max_age(time_ms(SESSION_TTL_MS))
        .build()
}

fn time_ms(ms: i64) -> time::Duration {
    time::Duration::milliseconds(ms)
}

fn user_agent(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
}

#[derive(Deserialize)]
struct PasswordBody {
    password: String,
}

async fn auth_status(State(app): State<App>, jar: CookieJar) -> Result<Json<Value>, ApiError> {
    let is_set_up = app.auth.is_set_up().await?;
    let is_logged_in = match jar.get(SESSION_COOKIE) {
        Some(c) => app.auth.authenticate(c.value()).await.is_ok(),
        None => false,
    };
    Ok(Json(
        json!({ "isSetUp": is_set_up, "isLoggedIn": is_logged_in }),
    ))
}

async fn set_up(
    State(app): State<App>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(body): Json<PasswordBody>,
) -> Result<(CookieJar, StatusCode), ApiError> {
    let token = app
        .auth
        .set_up(&body.password, user_agent(&headers))
        .await?;
    Ok((jar.add(session_cookie(token)), StatusCode::NO_CONTENT))
}

async fn log_in(
    State(app): State<App>,
    headers: HeaderMap,
    jar: CookieJar,
    Json(body): Json<PasswordBody>,
) -> Result<(CookieJar, StatusCode), ApiError> {
    let token = app
        .auth
        .log_in(&body.password, user_agent(&headers))
        .await?;
    Ok((jar.add(session_cookie(token)), StatusCode::NO_CONTENT))
}

async fn log_out(
    State(app): State<App>,
    jar: CookieJar,
) -> Result<(CookieJar, StatusCode), ApiError> {
    if let Some(c) = jar.get(SESSION_COOKIE) {
        app.auth.log_out(c.value()).await?;
    }
    Ok((
        jar.remove(Cookie::build(SESSION_COOKIE).path("/")),
        StatusCode::NO_CONTENT,
    ))
}

// ---------- 会话 ----------

#[derive(Deserialize)]
struct TitleBody {
    title: Option<String>,
}

#[derive(Deserialize)]
struct RenameBody {
    title: String,
}

async fn list_conversations(
    _: Owner,
    State(app): State<App>,
) -> Result<Json<Vec<Conversation>>, ApiError> {
    Ok(Json(app.conversations.list().await?))
}

async fn create_conversation(
    _: Owner,
    State(app): State<App>,
    Json(body): Json<TitleBody>,
) -> Result<(StatusCode, Json<Conversation>), ApiError> {
    let conv = app.conversations.create(body.title.as_deref()).await?;
    Ok((StatusCode::CREATED, Json(conv)))
}

async fn rename_conversation(
    _: Owner,
    State(app): State<App>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<RenameBody>,
) -> Result<Json<Conversation>, ApiError> {
    Ok(Json(app.conversations.rename(&id, &body.title).await?))
}

async fn delete_conversation(
    _: Owner,
    State(app): State<App>,
    UrlPath(id): UrlPath<String>,
) -> Result<StatusCode, ApiError> {
    app.conversations.delete(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_messages(
    _: Owner,
    State(app): State<App>,
    UrlPath(id): UrlPath<String>,
) -> Result<Json<Vec<Message>>, ApiError> {
    Ok(Json(app.conversations.messages(&id).await?))
}

// ---------- 设置：账户 ----------

async fn list_credentials(
    _: Owner,
    State(app): State<App>,
) -> Result<Json<Vec<Credential>>, ApiError> {
    Ok(Json(app.settings.list_credentials().await?))
}

async fn create_credential(
    _: Owner,
    State(app): State<App>,
    Json(body): Json<CredentialInput>,
) -> Result<(StatusCode, Json<Credential>), ApiError> {
    let c = app.settings.create_credential(body).await?;
    Ok((StatusCode::CREATED, Json(c)))
}

async fn update_credential(
    _: Owner,
    State(app): State<App>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<CredentialInput>,
) -> Result<Json<Credential>, ApiError> {
    Ok(Json(app.settings.update_credential(&id, body).await?))
}

async fn delete_credential(
    _: Owner,
    State(app): State<App>,
    UrlPath(id): UrlPath<String>,
) -> Result<StatusCode, ApiError> {
    app.settings.delete_credential(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_default_model(
    _: Owner,
    State(app): State<App>,
) -> Result<Json<Option<DefaultModel>>, ApiError> {
    Ok(Json(app.settings.default_model().await?))
}

async fn put_default_model(
    _: Owner,
    State(app): State<App>,
    Json(body): Json<DefaultModel>,
) -> Result<StatusCode, ApiError> {
    app.settings.set_default_model(body).await?;
    Ok(StatusCode::NO_CONTENT)
}
