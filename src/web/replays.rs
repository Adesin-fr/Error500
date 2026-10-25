//! Replays de session : liste, page de lecture et flux des événements rrweb.

use super::projects::project_access;
use super::{render_page, CurrentUser, WebError, WebState};
use crate::queries::{self, ReplayRow};
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use inertia_rs::axum::InertiaRequest;
use futures::stream::{self, Stream, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::convert::Infallible;

pub fn routes() -> Router<WebState> {
    Router::new()
        .route("/projects/{project_id}/replays", get(index))
        .route("/projects/{project_id}/replays/{replay_id}", get(show))
        .route("/projects/{project_id}/replays/{replay_id}/events", get(events))
        .route("/projects/{project_id}/replays/{replay_id}/stream", get(stream))
}

fn json_list(raw: &Option<String>) -> Value {
    raw.as_deref().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_else(|| json!([]))
}

fn replay_json(r: &ReplayRow) -> Value {
    let mut v = json!(r);
    v["urls"] = json_list(&r.urls);
    v["error_ids"] = json_list(&r.error_ids);
    v["live"] = json!(r.live != 0);
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
    props["stream_url"] = json!(format!("/projects/{project_id}/replays/{replay_id}/stream"));
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

#[derive(Deserialize)]
struct StreamParams {
    /// Dernier segment déjà reçu par le client (il a chargé l'historique via `/events`).
    after: Option<i64>,
}

/// Flux SSE d'une session en cours : un événement `status` (`{"live": bool}`) à l'ouverture et à chaque
/// changement, puis un événement `segment` (`{"segment_id", "events"}`) pour chaque segment, y compris
/// ceux qui arrivent pendant que le flux est ouvert. La base est interrogée toutes les `live_poll`
/// secondes : pas d'état partagé entre instances.
async fn stream(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    Path((project_id, replay_id)): Path<(u64, String)>,
    Query(params): Query<StreamParams>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, WebError> {
    project_access(&state, &user, project_id).await?;
    queries::get_replay(&state.pool, project_id, &replay_id).await?.ok_or(WebError::NotFound)?;

    struct Live {
        state: WebState,
        project_id: u64,
        replay_id: String,
        after: i64,
        sent: HashSet<u64>,
        last_live: Option<bool>,
        first: bool,
    }
    let live = Live { state, project_id, replay_id, after: params.after.unwrap_or(-1), sent: HashSet::new(), last_live: None, first: true };

    // Chaque itération produit un lot d'événements non vide, ou met fin au flux.
    let batches = stream::unfold(live, |mut l| async move {
        loop {
            if !std::mem::take(&mut l.first) {
                tokio::time::sleep(l.state.live_poll).await;
            }
            let mut batch: Vec<Event> = Vec::new();

            let is_live = match queries::get_replay(&l.state.pool, l.project_id, &l.replay_id).await {
                Ok(Some(r)) => r.live != 0,
                Ok(None) => return None, // replay supprimé (rétention, suppression du projet)
                Err(e) => {
                    tracing::warn!("flux live : {e}");
                    return None;
                }
            };
            if l.last_live != Some(is_live) {
                l.last_live = Some(is_live);
                batch.push(Event::default().event("status").data(json!({"live": is_live}).to_string()));
            }

            let rows: Result<Vec<(u64, String)>, _> = sqlx::query_as(
                "SELECT segment_id, blob_key FROM replay_segments WHERE project_id = ? AND replay_id = ? ORDER BY segment_id",
            )
            .bind(l.project_id)
            .bind(&l.replay_id)
            .fetch_all(&l.state.pool)
            .await;
            let Ok(rows) = rows else { return None };
            for (segment_id, key) in rows {
                if l.sent.contains(&segment_id) || (segment_id as i64) <= l.after {
                    continue;
                }
                // Un segment illisible est ignoré (et retenté au cycle suivant) : il ne doit pas bloquer les autres.
                let Ok(Some(bytes)) = l.state.store.get(&key).await else { continue };
                let events = match serde_json::from_slice::<Value>(&bytes) {
                    Ok(Value::Array(items)) => items,
                    Ok(obj @ Value::Object(_)) => vec![obj],
                    _ => continue,
                };
                l.sent.insert(segment_id);
                let data = json!({"segment_id": segment_id, "events": events}).to_string();
                batch.push(Event::default().event("segment").data(data));
            }
            if !batch.is_empty() {
                return Some((batch, l));
            }
        }
    });
    let events = batches.flat_map(|batch| stream::iter(batch.into_iter().map(Ok)));
    Ok(Sse::new(events).keep_alive(KeepAlive::default()))
}
