//! Réglages d'un projet : DSN, jetons, membres, source maps.

use super::projects::{project_access, ProjectAccess};
use super::{redirect, render_page, CurrentUser, WebError, WebState};
use crate::auth::build_dsn;
use crate::projects::{self, RemoveMemberError, Role};
use crate::sourcemaps;
use crate::users::{self, User};
use axum::extract::{Path, State};
use axum::response::Response;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use inertia_rs::axum::InertiaRequest;
use serde::Deserialize;
use serde_json::{json, Value};

pub fn routes() -> Router<WebState> {
    Router::new()
        .route("/projects/{project_id}/settings", get(show))
        .route("/projects/{project_id}/members", post(add_member))
        .route("/projects/{project_id}/members/{user_id}", delete(remove_member))
        .route("/projects/{project_id}/notifications", post(set_notifications))
        .route("/projects/{project_id}/rotate-key", post(rotate_key))
        .route("/projects/{project_id}/rotate-upload-token", post(rotate_upload_token))
        .route("/projects/{project_id}/sourcemaps/{sourcemap_id}", delete(delete_sourcemap))
}

/// Accès en gestion (propriétaire ou admin) ; 404 si le projet est invisible, 403 si simple membre.
async fn manage_access(state: &WebState, user: &User, project_id: u64) -> Result<ProjectAccess, WebError> {
    let access = project_access(state, user, project_id).await?;
    if access.can_manage { Ok(access) } else { Err(WebError::Forbidden) }
}

fn back_to_settings(req: &InertiaRequest, project_id: u64) -> Result<Response, WebError> {
    redirect(req, &format!("/projects/{project_id}/settings"), None)
}

async fn page(
    state: &WebState,
    user: &User,
    req: &InertiaRequest,
    access: &ProjectAccess,
    errors: Value,
) -> Result<Response, WebError> {
    let project = projects::get(&state.pool, access.project.id).await?.ok_or(WebError::NotFound)?;
    let mut props = access.props();
    props["dsn"] = json!(build_dsn(&state.public_url, &project.public_key, project.id));
    props["upload_url"] = json!(format!("{}/api/{}/sourcemaps/", state.public_url.trim_end_matches('/'), project.id));
    props["upload_token"] = if access.can_manage {
        json!(projects::upload_token(&state.pool, project.id).await?)
    } else {
        Value::Null
    };
    props["members"] = json!(projects::list_members(&state.pool, project.id).await?);
    props["sourcemaps"] = json!(sourcemaps::list(&state.pool, project.id)
        .await?
        .into_iter()
        .map(|m| json!({"id": m.id, "release": m.release, "name": m.name, "debug_id": m.debug_id, "size_bytes": m.size_bytes}))
        .collect::<Vec<_>>());
    props["notifications"] = json!(projects::notifications_enabled(&state.pool, project.id, user.id).await?);
    props["errors"] = errors;
    render_page(state, req, Some(user), "Projects/Settings", props)
}

async fn show(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    Path(project_id): Path<u64>,
    req: InertiaRequest,
) -> Result<Response, WebError> {
    let access = project_access(&state, &user, project_id).await?;
    page(&state, &user, &req, &access, json!({})).await
}

#[derive(Deserialize)]
struct MemberForm {
    email: String,
    role: String,
}

async fn add_member(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    Path(project_id): Path<u64>,
    req: InertiaRequest,
    Json(form): Json<MemberForm>,
) -> Result<Response, WebError> {
    let access = manage_access(&state, &user, project_id).await?;
    let Some(role) = Role::parse(&form.role) else {
        return page(&state, &user, &req, &access, json!({"role": "Rôle inconnu."})).await;
    };
    let Some(member) = users::find_by_email(&state.pool, &form.email).await? else {
        return page(&state, &user, &req, &access, json!({"email": "Aucun utilisateur avec cette adresse."})).await;
    };
    projects::add_member(&state.pool, project_id, member.id, role).await?;
    back_to_settings(&req, project_id)
}

async fn remove_member(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    Path((project_id, member_id)): Path<(u64, u64)>,
    req: InertiaRequest,
) -> Result<Response, WebError> {
    manage_access(&state, &user, project_id).await?;
    match projects::remove_member(&state.pool, project_id, member_id).await {
        Ok(()) => back_to_settings(&req, project_id),
        Err(RemoveMemberError::LastOwner) => Err(WebError::Invalid(RemoveMemberError::LastOwner.to_string())),
        Err(RemoveMemberError::Db(e)) => Err(e.into()),
    }
}

async fn rotate_key(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    Path(project_id): Path<u64>,
    req: InertiaRequest,
) -> Result<Response, WebError> {
    manage_access(&state, &user, project_id).await?;
    projects::rotate_key(&state.pool, project_id).await?;
    back_to_settings(&req, project_id)
}

async fn rotate_upload_token(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    Path(project_id): Path<u64>,
    req: InertiaRequest,
) -> Result<Response, WebError> {
    manage_access(&state, &user, project_id).await?;
    projects::rotate_upload_token(&state.pool, project_id).await?;
    back_to_settings(&req, project_id)
}

async fn delete_sourcemap(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    Path((project_id, sourcemap_id)): Path<(u64, u64)>,
    req: InertiaRequest,
) -> Result<Response, WebError> {
    manage_access(&state, &user, project_id).await?;
    sourcemaps::delete(&state.pool, &state.store, project_id, sourcemap_id)
        .await
        .map_err(|e| WebError::Internal(e.to_string()))?;
    back_to_settings(&req, project_id)
}

#[derive(Deserialize)]
struct NotificationsForm {
    enabled: bool,
}

/// Préférence personnelle : chaque membre règle ses propres alertes.
async fn set_notifications(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    Path(project_id): Path<u64>,
    req: InertiaRequest,
    Json(form): Json<NotificationsForm>,
) -> Result<Response, WebError> {
    project_access(&state, &user, project_id).await?;
    if projects::notifications_enabled(&state.pool, project_id, user.id).await?.is_none() {
        return Err(WebError::NotFound);
    }
    projects::set_notifications(&state.pool, project_id, user.id, form.enabled).await?;
    back_to_settings(&req, project_id)
}
