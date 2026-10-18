//! HTTP 层：路由、Cookie、请求校验与错误到状态码的映射。

use std::path::Path;

use axum::extract::{FromRequestParts, Path as UrlPath, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use ownmind_auth::api::{AuthError, SESSION_TTL_MS};
use ownmind_conversation::api::{Conversation, ConversationError, Message};
use ownmind_runtime::api::{ChatEvent, RuntimeError};
use ownmind_settings::api::{
    Credential, CredentialInput, DefaultModel, Profile, SettingsError, SystemPrefs,
};
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
        .route("/auth/totp", get(totp_status))
        .route("/auth/totp/setup", post(totp_setup))
        .route("/auth/totp/confirm", post(totp_confirm))
        .route("/auth/totp/disable", post(totp_disable))
        .route(
            "/conversations",
            get(list_conversations).post(create_conversation),
        )
        .route(
            "/conversations/{id}",
            patch(rename_conversation).delete(delete_conversation),
        )
        .route(
            "/conversations/{id}/messages",
            get(list_messages).post(send_message),
        )
        .route("/conversations/{id}/run", get(run_status).delete(stop_run))
        .route(
            "/settings/credentials",
            get(list_credentials).post(create_credential),
        )
        .route(
            "/settings/credentials/{id}",
            patch(update_credential).delete(delete_credential),
        )
        .route("/settings/profile", get(get_profile).put(put_profile))
        .route("/settings/system", get(get_system).put(put_system))
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
            AuthError::Locked { .. } => Self(StatusCode::TOO_MANY_REQUESTS, "locked"),
            AuthError::TotpRequired => Self(StatusCode::UNAUTHORIZED, "totp_required"),
            AuthError::InvalidTotp => Self(StatusCode::UNAUTHORIZED, "invalid_totp"),
            AuthError::TotpNotPending => Self(StatusCode::CONFLICT, "totp_not_pending"),
            AuthError::Secret(e) => internal(e),
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
            SettingsError::InvalidProfile => unprocessable("invalid_profile"),
            SettingsError::InvalidTimezone => unprocessable("invalid_timezone"),
            SettingsError::NoDefaultModel => Self(StatusCode::CONFLICT, "no_default_model"),
            SettingsError::Secret(e) => internal(e),
            SettingsError::Db(e) => internal(e),
        }
    }
}

impl From<RuntimeError> for ApiError {
    fn from(e: RuntimeError) -> Self {
        match e {
            RuntimeError::Settings(e) => e.into(),
            RuntimeError::Conversation(e) => e.into(),
            RuntimeError::Busy => Self(StatusCode::CONFLICT, "busy"),
            RuntimeError::NotRunning => Self(StatusCode::CONFLICT, "not_running"),
            RuntimeError::EmptyMessage => Self(StatusCode::UNPROCESSABLE_ENTITY, "empty_message"),
            RuntimeError::AgentUnavailable(m) => {
                tracing::error!(error = %m, "agent unavailable");
                Self(StatusCode::SERVICE_UNAVAILABLE, "agent_unavailable")
            }
            RuntimeError::Agent(m) => internal(m),
        }
    }
}

/// 登录类接口的错误：锁定时带上剩余秒数（响应体 retryAfter 与 Retry-After 头）。
struct AuthFailure(AuthError);

impl IntoResponse for AuthFailure {
    fn into_response(self) -> Response {
        if let AuthError::Locked { retry_after_secs } = self.0 {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, retry_after_secs.to_string())],
                Json(json!({ "error": "locked", "retryAfter": retry_after_secs })),
            )
                .into_response();
        }
        ApiError::from(self.0).into_response()
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

#[derive(Deserialize)]
struct LoginBody {
    password: String,
    code: Option<String>,
}

#[derive(Deserialize)]
struct CodeBody {
    code: String,
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
    Json(body): Json<LoginBody>,
) -> Result<(CookieJar, StatusCode), AuthFailure> {
    let token = app
        .auth
        .log_in(&body.password, body.code.as_deref(), user_agent(&headers))
        .await
        .map_err(AuthFailure)?;
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

// ---------- 两步验证 ----------

async fn totp_status(_: Owner, State(app): State<App>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({ "enabled": app.auth.totp_enabled().await? })))
}

async fn totp_setup(
    _: Owner,
    State(app): State<App>,
) -> Result<Json<ownmind_auth::api::TotpSetup>, ApiError> {
    Ok(Json(app.auth.totp_begin().await?))
}

async fn totp_confirm(
    _: Owner,
    State(app): State<App>,
    Json(body): Json<CodeBody>,
) -> Result<StatusCode, ApiError> {
    app.auth.totp_confirm(&body.code).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn totp_disable(
    _: Owner,
    State(app): State<App>,
    Json(body): Json<PasswordBody>,
) -> Result<StatusCode, AuthFailure> {
    app.auth
        .totp_disable(&body.password)
        .await
        .map_err(AuthFailure)?;
    Ok(StatusCode::NO_CONTENT)
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

#[derive(Deserialize)]
struct SendBody {
    text: String,
}

/// 发消息：返回 SSE。事件名即 ChatEvent 的 type，data 为整个事件 JSON。
async fn send_message(
    _: Owner,
    State(app): State<App>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<SendBody>,
) -> Result<Response, ApiError> {
    let rx = app.runtime.send(&id, &body.text).await?;
    let stream = futures_util::stream::unfold((rx, false), |(mut rx, done)| async move {
        if done {
            return None;
        }
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let fin = ev.is_final();
                    let name = match &ev {
                        ChatEvent::User { .. } => "user",
                        ChatEvent::Delta { .. } => "delta",
                        ChatEvent::Thinking { .. } => "thinking",
                        ChatEvent::Saved { .. } => "saved",
                        ChatEvent::ToolStart { .. } => "tool_start",
                        ChatEvent::Done { .. } => "done",
                        ChatEvent::Error { .. } => "error",
                    };
                    let event = Event::default()
                        .event(name)
                        .json_data(&ev)
                        .unwrap_or_default();
                    return Some((Ok::<_, std::convert::Infallible>(event), (rx, fin)));
                }
                // 消费太慢丢了一些增量：继续读，最终 done 里有完整文本。
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response())
}

async fn run_status(_: Owner, State(app): State<App>, UrlPath(id): UrlPath<String>) -> Json<Value> {
    Json(json!({ "running": app.runtime.is_running(&id).await }))
}

async fn stop_run(
    _: Owner,
    State(app): State<App>,
    UrlPath(id): UrlPath<String>,
) -> Result<StatusCode, ApiError> {
    app.runtime.stop(&id).await?;
    Ok(StatusCode::NO_CONTENT)
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

// ---------- 设置：个人资料与系统偏好 ----------

async fn get_profile(_: Owner, State(app): State<App>) -> Result<Json<Profile>, ApiError> {
    Ok(Json(app.settings.profile().await?))
}

async fn put_profile(
    _: Owner,
    State(app): State<App>,
    Json(body): Json<Profile>,
) -> Result<Json<Profile>, ApiError> {
    Ok(Json(app.settings.set_profile(body).await?))
}

async fn get_system(_: Owner, State(app): State<App>) -> Result<Json<SystemPrefs>, ApiError> {
    Ok(Json(app.settings.system().await?))
}

async fn put_system(
    _: Owner,
    State(app): State<App>,
    Json(body): Json<SystemPrefs>,
) -> Result<Json<SystemPrefs>, ApiError> {
    Ok(Json(app.settings.set_system(body).await?))
}
