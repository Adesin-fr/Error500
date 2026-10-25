//! Interface web : pages Inertia (Vue) servies par le même binaire que l'ingestion.

pub mod assets;
pub mod admin;
pub mod auth;
pub mod cookies;
pub mod issues;
pub mod logs;
pub mod projects;
pub mod replays;
pub mod settings;

use crate::storage::BlobStore;
use crate::users::{self, User};
use assets::{render_shell, Assets};
use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::Router;
use inertia_rs::axum::{InertiaError, InertiaRequest, VersionLayer};
use inertia_rs::Inertia;
use serde_json::{json, Value};
use sqlx::MySqlPool;
use std::convert::Infallible;
use std::sync::Arc;

#[derive(Clone)]
pub struct WebState {
    pub pool: MySqlPool,
    pub store: BlobStore,
    /// URL publique de l'application, pour construire les DSN affichés dans l'interface.
    pub public_url: String,
    pub assets: Arc<Assets>,
    /// Serveur de développement Vite (rechargement à chaud) ; `None` en production.
    pub dev_url: Option<String>,
    pub secure_cookies: bool,
}

pub fn router(state: WebState) -> Router {
    let version = state.assets.version.clone();
    Router::new()
        .route("/", get(|_: CurrentUser| async { Redirect::to("/projects") }))
        .route("/assets/{*path}", get(asset))
        .merge(auth::routes())
        .merge(projects::routes())
        .merge(issues::routes())
        .merge(replays::routes())
        .merge(logs::routes())
        .merge(settings::routes())
        .merge(admin::routes())
        .layer(VersionLayer::new(version))
        .with_state(state)
}

#[derive(Debug)]
pub enum WebError {
    NotFound,
    Forbidden,
    Invalid(String),
    Internal(String),
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        match self {
            WebError::NotFound => (StatusCode::NOT_FOUND, "Introuvable").into_response(),
            WebError::Forbidden => (StatusCode::FORBIDDEN, "Accès refusé").into_response(),
            WebError::Invalid(d) => (StatusCode::UNPROCESSABLE_ENTITY, d).into_response(),
            WebError::Internal(e) => {
                tracing::error!("erreur interne : {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Erreur interne").into_response()
            }
        }
    }
}

impl From<sqlx::Error> for WebError {
    fn from(e: sqlx::Error) -> Self {
        WebError::Internal(e.to_string())
    }
}

impl From<InertiaError> for WebError {
    fn from(e: InertiaError) -> Self {
        WebError::Internal(e.to_string())
    }
}

impl From<users::UserError> for WebError {
    fn from(e: users::UserError) -> Self {
        WebError::Internal(e.to_string())
    }
}

/// Utilisateur de la session ; redirige vers `/login` s'il n'y en a pas.
pub struct CurrentUser(pub User);

/// Utilisateur de la session, s'il y en a un.
pub struct MaybeUser(pub Option<User>);

async fn session_user(parts: &Parts, state: &WebState) -> Option<User> {
    let header = parts.headers.get(header::COOKIE)?.to_str().ok()?;
    let token = cookies::read(header, cookies::SESSION_COOKIE)?;
    users::user_for_session(&state.pool, &token).await.ok().flatten()
}

impl FromRequestParts<WebState> for CurrentUser {
    type Rejection = Redirect;

    async fn from_request_parts(parts: &mut Parts, state: &WebState) -> Result<Self, Redirect> {
        session_user(parts, state).await.map(CurrentUser).ok_or_else(|| Redirect::to("/login"))
    }
}

impl FromRequestParts<WebState> for MaybeUser {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &WebState) -> Result<Self, Infallible> {
        Ok(MaybeUser(session_user(parts, state).await))
    }
}

/// Rend une page Inertia : JSON pour une visite Inertia, HTML complet pour un premier chargement.
/// Les props reçoivent toujours `auth.user` (partagé par toutes les pages).
pub fn render_page(
    state: &WebState,
    req: &InertiaRequest,
    user: Option<&User>,
    component: &str,
    props: Value,
) -> Result<Response, WebError> {
    let mut props = match props {
        Value::Object(m) => m,
        _ => serde_json::Map::new(),
    };
    props.insert(
        "auth".into(),
        json!({ "user": user.map(|u| json!({"id": u.id, "name": u.name, "email": u.email, "is_admin": u.is_admin})) }),
    );
    Ok(req.render(Inertia::response(component, Value::Object(props)), |ctx| {
        Html(render_shell(ctx.data_page(), &state.assets, state.dev_url.as_deref(), "error500"))
    })?)
}

/// Redirection (303 après une écriture) avec, éventuellement, un `Set-Cookie`.
pub fn redirect(req: &InertiaRequest, to: &str, set_cookie: Option<String>) -> Result<Response, WebError> {
    let mut resp = req.redirect(Inertia::redirect(to))?;
    if let Some(c) = set_cookie {
        resp.headers_mut().append(header::SET_COOKIE, HeaderValue::from_str(&c).map_err(|e| WebError::Internal(e.to_string()))?);
    }
    Ok(resp)
}

async fn asset(State(state): State<WebState>, Path(path): Path<String>) -> Response {
    match state.assets.get(&format!("assets/{path}")) {
        Some(a) => (
            [
                (header::CONTENT_TYPE, a.content_type.clone()),
                (header::CACHE_CONTROL, "public, max-age=31536000, immutable".to_string()),
            ],
            a.bytes.clone(),
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
