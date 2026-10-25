//! Liste des issues d'un projet.

use super::projects::project_access;
use super::{redirect, render_page, CurrentUser, WebError, WebState};
use crate::event::context_view;
use crate::queries::{self, IssueStatus};
use crate::sourcemaps::{self, MapSet};
use crate::stacktrace::build_view;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use inertia_rs::axum::InertiaRequest;
use serde::Deserialize;
use serde_json::{json, Value};

const PER_PAGE: u32 = 25;

pub fn routes() -> Router<WebState> {
    Router::new()
        .route("/projects/{project_id}/issues", get(index))
        .route("/projects/{project_id}/issues/{issue_id}", get(show))
        .route("/projects/{project_id}/issues/{issue_id}/status", post(set_status))
}

#[derive(Deserialize)]
struct IssuesQuery {
    status: Option<String>,
    page: Option<u32>,
}

async fn index(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    Path(project_id): Path<u64>,
    Query(q): Query<IssuesQuery>,
    req: InertiaRequest,
) -> Result<Response, WebError> {
    let access = project_access(&state, &user, project_id).await?;
    let status = q.status.as_deref().and_then(IssueStatus::parse).unwrap_or(IssueStatus::Unresolved);

    let total = queries::count_issues(&state.pool, project_id, Some(status)).await?;
    let pages = total.div_ceil(u64::from(PER_PAGE)).max(1) as u32;
    let page = q.page.unwrap_or(1).clamp(1, pages);
    let issues =
        queries::list_issues(&state.pool, project_id, Some(status), PER_PAGE, (page - 1) * PER_PAGE).await?;

    let mut counts = serde_json::Map::new();
    for s in [IssueStatus::Unresolved, IssueStatus::Resolved, IssueStatus::Ignored] {
        counts.insert(s.as_str().into(), queries::count_issues(&state.pool, project_id, Some(s)).await?.into());
    }

    let mut props = access.props();
    props["issues"] = json!(issues);
    props["status"] = json!(status);
    props["page"] = json!(page);
    props["pages"] = json!(pages);
    props["total"] = json!(total);
    props["counts"] = Value::Object(counts);
    render_page(&state, &req, Some(&user), "Issues/Index", props)
}

#[derive(Deserialize)]
struct ShowQuery {
    event: Option<String>,
}

async fn show(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    Path((project_id, issue_id)): Path<(u64, u64)>,
    Query(q): Query<ShowQuery>,
    req: InertiaRequest,
) -> Result<Response, WebError> {
    let access = project_access(&state, &user, project_id).await?;
    let issue = queries::get_issue(&state.pool, project_id, issue_id).await?.ok_or(WebError::NotFound)?;
    let event = match q.event.as_deref() {
        Some(id) => queries::get_event(&state.pool, project_id, issue_id, id).await?,
        None => queries::latest_event(&state.pool, project_id, issue_id).await?,
    }
    .ok_or(WebError::NotFound)?;

    // Une source map illisible ne doit pas empêcher d'afficher l'erreur : on retombe sur les frames bruts.
    let maps = sourcemaps::load_for_event(&state.pool, &state.store, project_id, &event.payload)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("chargement des source maps impossible : {e}");
            MapSet::default()
        });
    let exceptions = build_view(&event.payload, &maps);
    let ctx = context_view(&event.payload);

    // Le replay n'est proposé que s'il a bien été reçu.
    let mut replay_id = Value::Null;
    let mut replay_timeline = Value::Null;
    if let Some(id) = ctx["replay_id"].as_str() {
        if let Some(replay) = queries::get_replay(&state.pool, project_id, id).await? {
            replay_id = json!(id);
            // Repères : l'erreur affichée plus les autres erreurs de la même session, dans l'ordre du temps.
            let mut ids: Vec<String> = replay
                .error_ids
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            if !ids.contains(&event.event_id) {
                ids.push(event.event_id.clone());
            }
            let markers: Vec<Value> = queries::linked_errors(&state.pool, project_id, &ids)
                .await?
                .into_iter()
                .map(|e| {
                    json!({
                        "event_id": e.event_id,
                        "issue_id": e.issue_id,
                        "title": e.title,
                        "occurred_at": e.occurred_at,
                        "current": e.event_id == event.event_id,
                    })
                })
                .collect();
            replay_timeline = json!({
                "events_url": format!("/projects/{project_id}/replays/{id}/events"),
                "markers": markers,
            });
        }
    }

    let mut event_props = ctx;
    event_props["event_id"] = json!(event.event_id);
    event_props["occurred_at"] = json!(event.occurred_at);
    event_props["release"] = json!(event.release);
    event_props["environment"] = json!(event.environment);
    event_props["platform"] = event.payload.get("platform").cloned().unwrap_or(Value::Null);
    event_props.as_object_mut().map(|m| m.remove("replay_id"));

    let mut props = access.props();
    props["issue"] = json!(issue);
    props["event"] = event_props;
    props["exceptions"] = json!(exceptions);
    props["events"] = json!(queries::list_events(&state.pool, project_id, issue_id, 20).await?);
    props["replay_id"] = replay_id;
    props["replay_timeline"] = replay_timeline;
    render_page(&state, &req, Some(&user), "Issues/Show", props)
}

#[derive(Deserialize)]
struct StatusForm {
    status: String,
}

async fn set_status(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    Path((project_id, issue_id)): Path<(u64, u64)>,
    req: InertiaRequest,
    Json(form): Json<StatusForm>,
) -> Result<Response, WebError> {
    project_access(&state, &user, project_id).await?;
    queries::get_issue(&state.pool, project_id, issue_id).await?.ok_or(WebError::NotFound)?;
    let status = IssueStatus::parse(&form.status).ok_or_else(|| WebError::Invalid("statut inconnu".into()))?;
    queries::set_issue_status(&state.pool, project_id, issue_id, status).await?;
    redirect(&req, &format!("/projects/{project_id}/issues/{issue_id}"), None)
}
