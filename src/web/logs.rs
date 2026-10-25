//! Logs applicatifs d'un projet.

use super::projects::project_access;
use super::{render_page, CurrentUser, WebError, WebState};
use crate::queries::{self, LogFilter};
use axum::extract::{Path, Query, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use inertia_rs::axum::InertiaRequest;
use serde::Deserialize;
use serde_json::{json, Value};

const LIMIT: u32 = 200;

pub fn routes() -> Router<WebState> {
    Router::new().route("/projects/{project_id}/logs", get(index))
}

#[derive(Deserialize)]
struct LogsQuery {
    level: Option<String>,
    q: Option<String>,
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

async fn index(
    State(state): State<WebState>,
    CurrentUser(user): CurrentUser,
    Path(project_id): Path<u64>,
    Query(q): Query<LogsQuery>,
    req: InertiaRequest,
) -> Result<Response, WebError> {
    let access = project_access(&state, &user, project_id).await?;
    let filter = LogFilter { level: non_empty(q.level), search: non_empty(q.q) };
    let logs: Vec<Value> = queries::list_logs(&state.pool, project_id, &filter, LIMIT)
        .await?
        .into_iter()
        .map(|l| {
            let attributes = l.attributes.as_deref().and_then(|a| serde_json::from_str::<Value>(a).ok());
            let mut v = json!(l);
            v["attributes"] = attributes.unwrap_or(Value::Null);
            v
        })
        .collect();

    let mut props = access.props();
    props["logs"] = json!(logs);
    props["filters"] = json!({"level": filter.level, "q": filter.search});
    render_page(&state, &req, Some(&user), "Logs/Index", props)
}
