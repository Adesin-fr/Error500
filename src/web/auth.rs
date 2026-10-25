//! Pages d'authentification : premier lancement, connexion, déconnexion.

use super::cookies::{clear_cookie, read, session_cookie, SESSION_COOKIE};
use super::{redirect, render_page, MaybeUser, WebError, WebState};
use super::account::unix_now;
use crate::users::{self, ChallengeOutcome, UserError};
use axum::extract::State;
use axum::http::header;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use inertia_rs::axum::InertiaRequest;
use serde::Deserialize;
use serde_json::json;

const SESSION_SECS: u64 = 30 * 24 * 3600;

pub fn routes() -> Router<WebState> {
    Router::new()
        .route("/login", get(login_page).post(login))
        .route("/login/two-factor", post(two_factor))
        .route("/logout", post(logout))
        .route("/setup", get(setup_page).post(setup))
}

#[derive(Deserialize)]
struct LoginForm {
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct SetupForm {
    name: String,
    email: String,
    password: String,
}

async fn login_page(
    State(state): State<WebState>,
    MaybeUser(user): MaybeUser,
    req: InertiaRequest,
) -> Result<Response, WebError> {
    if user.is_some() {
        return redirect(&req, "/projects", None);
    }
    if users::count(&state.pool).await? == 0 {
        return redirect(&req, "/setup", None);
    }
    render_page(&state, &req, None, "Auth/Login", json!({ "errors": {} }))
}

async fn login(
    State(state): State<WebState>,
    req: InertiaRequest,
    Json(form): Json<LoginForm>,
) -> Result<Response, WebError> {
    match users::authenticate(&state.pool, &form.email, &form.password).await? {
        Some(user) if users::two_factor_enabled(&state.pool, user.id).await? => {
            if state.secret_key.is_none() {
                return render_page(
                    &state,
                    &req,
                    None,
                    "Auth/Login",
                    json!({ "errors": { "email": "Double authentification indisponible : SECRET_KEY n’est pas configurée." } }),
                );
            }
            let challenge = users::create_login_challenge(&state.pool, user.id).await?;
            render_page(&state, &req, None, "Auth/TwoFactor", json!({ "challenge": challenge, "errors": {} }))
        }
        Some(user) => {
            let token = users::create_session(&state.pool, user.id).await?;
            redirect(&req, "/projects", Some(session_cookie(&token, SESSION_SECS, state.secure_cookies)))
        }
        None => render_page(
            &state,
            &req,
            None,
            "Auth/Login",
            json!({ "errors": { "email": "Adresse e-mail ou mot de passe incorrect." } }),
        ),
    }
}

#[derive(Deserialize)]
struct TwoFactorForm {
    challenge: String,
    code: String,
}

/// Seconde étape de la connexion : code de l'application ou code de secours.
async fn two_factor(
    State(state): State<WebState>,
    req: InertiaRequest,
    Json(form): Json<TwoFactorForm>,
) -> Result<Response, WebError> {
    let Some(key) = &state.secret_key else {
        return render_page(&state, &req, None, "Auth/Login", json!({ "errors": { "email": "SECRET_KEY n’est pas configurée." } }));
    };
    match users::answer_login_challenge(&state.pool, key, &form.challenge, &form.code, unix_now()).await? {
        ChallengeOutcome::Passed(user) => {
            let token = users::create_session(&state.pool, user.id).await?;
            redirect(&req, "/projects", Some(session_cookie(&token, SESSION_SECS, state.secure_cookies)))
        }
        ChallengeOutcome::Wrong => render_page(
            &state,
            &req,
            None,
            "Auth/TwoFactor",
            json!({ "challenge": form.challenge, "errors": { "code": "Code incorrect." } }),
        ),
        ChallengeOutcome::Invalid => render_page(
            &state,
            &req,
            None,
            "Auth/Login",
            json!({ "errors": { "email": "Connexion expirée, recommencez." } }),
        ),
    }
}

async fn logout(
    State(state): State<WebState>,
    req: InertiaRequest,
    headers: axum::http::HeaderMap,
) -> Result<Response, WebError> {
    let token = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|c| read(c, SESSION_COOKIE));
    if let Some(token) = token {
        users::delete_session(&state.pool, &token).await?;
    }
    redirect(&req, "/login", Some(clear_cookie(state.secure_cookies)))
}

async fn setup_page(State(state): State<WebState>, req: InertiaRequest) -> Result<Response, WebError> {
    if users::count(&state.pool).await? > 0 {
        return redirect(&req, "/login", None);
    }
    render_page(&state, &req, None, "Auth/Setup", json!({ "errors": {} }))
}

/// Création du premier compte (administrateur). Fermé dès qu'un utilisateur existe.
async fn setup(
    State(state): State<WebState>,
    req: InertiaRequest,
    Json(form): Json<SetupForm>,
) -> Result<Response, WebError> {
    if users::count(&state.pool).await? > 0 {
        return redirect(&req, "/login", None);
    }
    let mut errors = serde_json::Map::new();
    if form.name.trim().is_empty() {
        errors.insert("name".into(), "Le nom est requis.".into());
    }
    if !form.email.contains('@') {
        errors.insert("email".into(), "Adresse e-mail invalide.".into());
    }
    if errors.is_empty() {
        match users::create_user(&state.pool, &form.email, form.name.trim(), &form.password).await {
            Ok(user) => {
                let token = users::create_session(&state.pool, user.id).await?;
                return redirect(&req, "/projects", Some(session_cookie(&token, SESSION_SECS, state.secure_cookies)));
            }
            Err(UserError::WeakPassword) => {
                errors.insert("password".into(), "8 caractères minimum.".into());
            }
            Err(UserError::EmailTaken) => {
                errors.insert("email".into(), "Adresse e-mail déjà utilisée.".into());
            }
            Err(e) => return Err(e.into()),
        }
    }
    render_page(&state, &req, None, "Auth/Setup", json!({ "errors": errors }))
}
