//! Liste et création des projets, contrôle d'accès par projet.

use super::{redirect, render_page, CurrentUser, WebError, WebState};
use crate::projects::{self, Project, Role};
use crate::queries::{self, IssueStatus};
use crate::users::User;
use axum::extract::{Path, State};
use axum::response::{Redirect, Response};
use axum::routing::get;
use axum::{Json, Router};
use inertia_rs::axum::InertiaRequest;
use serde::Deserialize;
use serde_json::{json, Value};

pub fn routes() -> Router<WebState> {
    Router::new()
        .route("/projects", get(index).post(create))
        .route("/projects/{project_id}", get(root))
}

/// Projet auquel l'utilisateur a accès (membre, ou administrateur).
pub struct ProjectAccess {
    pub project: Project,
    pub can_manage: bool,
}

impl ProjectAccess {
    /// Props communes à toutes les pages d'un projet.
    pub fn props(&self) -> Value {
        json!({
            "project": {"id": self.project.id, "name": self.project.name, "slug": self.project.slug},
            "can_manage": self.can_manage,
        })
    }
}

/// 404 (et non 403) si le projet n'existe pas ou n'est pas accessible, pour ne pas en révéler l'existence.
pub async fn project_access(state: &WebState, user: &User, project_id: u64) -> Result<ProjectAccess, WebError> {
    let project = projects::get(&state.pool, project_id).await?.ok_or(WebError::NotFound)?;
    let role = projects::user_role(&state.pool, project_id, user.id).await?;
    if !user.is_admin && role.is_none() {
        return Err(WebError::NotFound);
    }
    Ok(ProjectAccess { project, can_manage: user.is_admin || role == Some(Role::Owner) })
}

async fn index(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    req: InertiaRequest,
) -> Result<Response, WebError> {
    index_page(&state, &user, &req, json!({})).await
}

async fn index_page(
    state: &WebState,
    user: &User,
    req: &InertiaRequest,
    errors: Value,
) -> Result<Response, WebError> {
    let mut list = Vec::new();
    for p in projects::list_for_user(&state.pool, user).await? {
        let unresolved = queries::count_issues(&state.pool, p.id, Some(IssueStatus::Unresolved)).await?;
        list.push(json!({"id": p.id, "name": p.name, "slug": p.slug, "unresolved_count": unresolved}));
    }
    render_page(state, req, Some(user), "Projects/Index", json!({ "projects": list, "errors": errors }))
}

#[derive(Deserialize)]
struct CreateForm {
    name: String,
}

async fn create(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    req: InertiaRequest,
    Json(form): Json<CreateForm>,
) -> Result<Response, WebError> {
    let name = form.name.trim();
    if name.is_empty() {
        return index_page(&state, &user, &req, json!({"name": "Le nom est requis."})).await;
    }
    let project = projects::create_project(&state.pool, user.id, name).await?;
    redirect(&req, &format!("/projects/{}/issues", project.id), None)
}

async fn root(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    Path(project_id): Path<u64>,
) -> Result<Redirect, WebError> {
    project_access(&state, &user, project_id).await?;
    Ok(Redirect::to(&format!("/projects/{project_id}/issues")))
}
