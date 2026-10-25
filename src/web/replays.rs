//! Replays de session : liste, page de lecture et flux des événements rrweb.

use super::projects::project_access;
use super::{render_page, CurrentUser, WebError, WebState};
use crate::queries::{self, ReplayRow};
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use inertia_rs::axum::InertiaRequest;
use serde_json::{json, Value};

pub fn routes() -> Router<WebState> {
    Router::new()
        .route("/projects/{project_id}/replays", get(index))
        .route("/projects/{project_id}/replays/{replay_id}", get(show))
        .route("/projects/{project_id}/replays/{replay_id}/events", get(events))
}

fn json_list(raw: &Option<String>) -> Value {
    raw.as_deref().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_else(|| json!([]))
}

fn replay_json(r: &ReplayRow) -> Value {
    let mut v = json!(r);
    v["urls"] = json_list(&r.urls);
    v["error_ids"] = json_list(&r.error_ids);
    v
}

async fn index(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    Path(project_id): Path<u64>,
    req: InertiaRequest,
) -> Result<Response, WebError> {
    let access = project_access(&state, &user, project_id).await?;
    let replays: Vec<Value> =
        queries::list_replays(&state.pool, project_id, 100).await?.iter().map(replay_json).collect();
    let mut props = access.props();
    props["replays"] = json!(replays);
    render_page(&state, &req, Some(&user), "Replays/Index", props)
}

async fn show(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    Path((project_id, replay_id)): Path<(u64, String)>,
    req: InertiaRequest,
) -> Result<Response, WebError> {
    let access = project_access(&state, &user, project_id).await?;
    let replay = queries::get_replay(&state.pool, project_id, &replay_id).await?.ok_or(WebError::NotFound)?;
    let error_ids: Vec<String> = json_list(&replay.error_ids)
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default();

    let mut props = access.props();
    props["replay"] = replay_json(&replay);
    props["events_url"] = json!(format!("/projects/{project_id}/replays/{replay_id}/events"));
    props["errors"] = json!(queries::linked_errors(&state.pool, project_id, &error_ids).await?);
    render_page(&state, &req, Some(&user), "Replays/Show", props)
}

/// Événements rrweb du replay, consommés par le lecteur (chargés à part : ils peuvent être volumineux).
async fn events(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    Path((project_id, replay_id)): Path<(u64, String)>,
) -> Result<Response, WebError> {
    project_access(&state, &user, project_id).await?;
    let events = queries::replay_events(&state.pool, &state.store, project_id, &replay_id)
        .await?
        .ok_or(WebError::NotFound)?;
    let body = serde_json::to_vec(&events).map_err(|e| WebError::Internal(e.to_string()))?;
    Ok(([(header::CONTENT_TYPE, "application/json"), (header::CACHE_CONTROL, "private, max-age=300")], body).into_response())
}
