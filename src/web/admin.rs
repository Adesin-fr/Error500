//! Administration des utilisateurs (réservée aux administrateurs).

use super::{redirect, render_page, CurrentUser, WebError, WebState};
use crate::users::{self, User, UserError};
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use inertia_rs::axum::InertiaRequest;
use serde::Deserialize;
use serde_json::{json, Value};

pub fn routes() -> Router<WebState> {
    Router::new().route("/admin/users", get(index).post(create))
}

/// 404 pour un non-administrateur : la section n'a pas à révéler son existence.
fn require_admin(user: &User) -> Result<(), WebError> {
    if user.is_admin { Ok(()) } else { Err(WebError::NotFound) }
}

async fn page(state: &WebState, user: &User, req: &InertiaRequest, errors: Value) -> Result<Response, WebError> {
    let list = users::list(&state.pool).await?;
    render_page(state, req, Some(user), "Admin/Users", json!({ "users": list, "errors": errors }))
}

async fn index(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    req: InertiaRequest,
) -> Result<Response, WebError> {
    require_admin(&user)?;
    page(&state, &user, &req, json!({})).await
}

#[derive(Deserialize)]
struct NewUser {
    name: String,
    email: String,
    password: String,
    #[serde(default)]
    is_admin: bool,
}

async fn create(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    req: InertiaRequest,
    Json(form): Json<NewUser>,
) -> Result<Response, WebError> {
    require_admin(&user)?;
    let mut errors = serde_json::Map::new();
    if form.name.trim().is_empty() {
        errors.insert("name".into(), "Le nom est requis.".into());
    }
    if !form.email.contains('@') {
        errors.insert("email".into(), "Adresse e-mail invalide.".into());
    }
    if errors.is_empty() {
        match users::create_user(&state.pool, &form.email, form.name.trim(), &form.password).await {
            Ok(created) => {
                if form.is_admin {
                    users::set_admin(&state.pool, created.id, true).await?;
                }
                return redirect(&req, "/admin/users", None);
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
    page(&state, &user, &req, Value::Object(errors)).await
}
