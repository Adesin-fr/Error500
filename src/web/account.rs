//! Compte de l'utilisateur connecté : authentification à deux facteurs (TOTP).

use super::{redirect, render_page, CurrentUser, WebError, WebState};
use crate::users::{self, User};
use crate::totp;
use axum::extract::State;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use inertia_rs::axum::InertiaRequest;
use serde::Deserialize;
use serde_json::{json, Value};

const ISSUER: &str = "error500";

pub fn routes() -> Router<WebState> {
    Router::new()
        .route("/account", get(show))
        .route("/account/2fa/setup", post(setup))
        .route("/account/2fa/enable", post(enable))
        .route("/account/2fa/disable", post(disable))
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

async fn page(
    state: &WebState,
    req: &InertiaRequest,
    user: &User,
    errors: Value,
    recovery_codes: Option<Vec<String>>,
) -> Result<Response, WebError> {
    let Some(key) = &state.secret_key else {
        let status = users::two_factor_status_without_secret(&state.pool, user.id).await?;
        let two_factor = json!({"available": false, "enabled": status.enabled, "recovery_remaining": status.recovery_remaining});
        return render_page(state, req, Some(user), "Account/Show", json!({"two_factor": two_factor, "recovery_codes": recovery_codes, "errors": errors}));
    };
    let status = users::two_factor_status(&state.pool, key, user.id).await?;
    let mut two_factor = json!({"available": true, "enabled": status.enabled, "recovery_remaining": status.recovery_remaining});
    if let Some(secret) = &status.pending_secret {
        let uri = totp::otpauth_uri(ISSUER, &user.email, secret);
        two_factor["secret"] = json!(secret);
        two_factor["qr_svg"] = json!(totp::qr_svg(&uri));
        two_factor["otpauth_uri"] = json!(uri);
    }
    render_page(
        state,
        req,
        Some(user),
        "Account/Show",
        json!({"two_factor": two_factor, "recovery_codes": recovery_codes, "errors": errors}),
    )
}

async fn show(State(state): State<WebState>, CurrentUser(user): CurrentUser, req: InertiaRequest) -> Result<Response, WebError> {
    page(&state, &req, &user, json!({}), None).await
}

async fn setup(State(state): State<WebState>, CurrentUser(user): CurrentUser, req: InertiaRequest) -> Result<Response, WebError> {
    let Some(key) = &state.secret_key else {
        return page(&state, &req, &user, json!({"setup": "SECRET_KEY n’est pas configurée sur le serveur."}), None).await;
    };
    users::begin_two_factor_setup(&state.pool, key, user.id).await?;
    page(&state, &req, &user, json!({}), None).await
}

#[derive(Deserialize)]
struct CodeForm {
    code: String,
}

async fn enable(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    req: InertiaRequest,
    Json(form): Json<CodeForm>,
) -> Result<Response, WebError> {
    let Some(key) = &state.secret_key else {
        return page(&state, &req, &user, json!({"setup": "SECRET_KEY n’est pas configurée sur le serveur."}), None).await;
    };
    match users::enable_two_factor(&state.pool, key, user.id, &form.code, unix_now()).await? {
        // Réponse directe, sans redirection : les codes de secours ne sont montrés qu'une fois.
        Some(codes) => page(&state, &req, &user, json!({}), Some(codes)).await,
        None => page(&state, &req, &user, json!({"code": "Code incorrect."}), None).await,
    }
}

#[derive(Deserialize)]
struct PasswordForm {
    password: String,
}

async fn disable(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    req: InertiaRequest,
    Json(form): Json<PasswordForm>,
) -> Result<Response, WebError> {
    if !users::check_password(&state.pool, user.id, &form.password).await? {
        return page(&state, &req, &user, json!({"password": "Mot de passe incorrect."}), None).await;
    }
    users::disable_two_factor(&state.pool, user.id).await?;
    redirect(&req, "/account", None)
}
