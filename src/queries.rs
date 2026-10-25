//! Requêtes de lecture pour l'interface : issues, événements, logs, replays.

use crate::storage::BlobStore;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::{FromRow, MySqlPool};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueStatus {
    Unresolved,
    Resolved,
    Ignored,
}

impl IssueStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            IssueStatus::Unresolved => "unresolved",
            IssueStatus::Resolved => "resolved",
            IssueStatus::Ignored => "ignored",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "unresolved" => Some(IssueStatus::Unresolved),
            "resolved" => Some(IssueStatus::Resolved),
            "ignored" => Some(IssueStatus::Ignored),
            _ => None,
        }
    }
}

impl TryFrom<String> for IssueStatus {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        IssueStatus::parse(&s).ok_or_else(|| format!("statut inconnu : {s}"))
    }
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct IssueRow {
    pub id: u64,
    pub title: String,
    pub level: String,
    #[sqlx(try_from = "String")]
    pub status: IssueStatus,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub event_count: u64,
}

const ISSUE_COLS: &str = "id, title, level, status, first_seen, last_seen, event_count";

pub async fn list_issues(
    pool: &MySqlPool,
    project_id: u64,
    status: Option<IssueStatus>,
    limit: u32,
    offset: u32,
) -> Result<Vec<IssueRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {ISSUE_COLS} FROM issues WHERE project_id = ? AND (? IS NULL OR status = ?) \
         ORDER BY last_seen DESC, id DESC LIMIT ? OFFSET ?"
    )))
    .bind(project_id)
    .bind(status.map(IssueStatus::as_str))
    .bind(status.map(IssueStatus::as_str))
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

pub async fn count_issues(
    pool: &MySqlPool,
    project_id: u64,
    status: Option<IssueStatus>,
) -> Result<u64, sqlx::Error> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues WHERE project_id = ? AND (? IS NULL OR status = ?)")
        .bind(project_id)
        .bind(status.map(IssueStatus::as_str))
        .bind(status.map(IssueStatus::as_str))
        .fetch_one(pool)
        .await?;
    Ok(n as u64)
}

pub async fn get_issue(pool: &MySqlPool, project_id: u64, issue_id: u64) -> Result<Option<IssueRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT {ISSUE_COLS} FROM issues WHERE project_id = ? AND id = ?")))
        .bind(project_id)
        .bind(issue_id)
        .fetch_optional(pool)
        .await
}

pub async fn set_issue_status(
    pool: &MySqlPool,
    project_id: u64,
    issue_id: u64,
    status: IssueStatus,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE issues SET status = ? WHERE project_id = ? AND id = ?")
        .bind(status.as_str())
        .bind(project_id)
        .bind(issue_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct EventSummary {
    pub event_id: String,
    pub occurred_at: DateTime<Utc>,
    pub release: Option<String>,
    pub environment: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventRow {
    pub event_id: String,
    pub occurred_at: DateTime<Utc>,
    pub release: Option<String>,
    pub environment: Option<String>,
    pub payload: Value,
}

type EventTuple = (String, DateTime<Utc>, Option<String>, Option<String>, String);

fn event_row((event_id, occurred_at, release, environment, payload): EventTuple) -> EventRow {
    EventRow { event_id, occurred_at, release, environment, payload: serde_json::from_str(&payload).unwrap_or(Value::Null) }
}

const EVENT_COLS: &str = "event_id, occurred_at, release_name, environment, payload";

pub async fn list_events(
    pool: &MySqlPool,
    project_id: u64,
    issue_id: u64,
    limit: u32,
) -> Result<Vec<EventSummary>, sqlx::Error> {
    let rows: Vec<(String, DateTime<Utc>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT event_id, occurred_at, release_name, environment FROM events \
         WHERE project_id = ? AND issue_id = ? ORDER BY occurred_at DESC, id DESC LIMIT ?",
    )
    .bind(project_id)
    .bind(issue_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(event_id, occurred_at, release, environment)| EventSummary { event_id, occurred_at, release, environment })
        .collect())
}

pub async fn latest_event(pool: &MySqlPool, project_id: u64, issue_id: u64) -> Result<Option<EventRow>, sqlx::Error> {
    let row: Option<EventTuple> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {EVENT_COLS} FROM events WHERE project_id = ? AND issue_id = ? ORDER BY occurred_at DESC, id DESC LIMIT 1"
    )))
    .bind(project_id)
    .bind(issue_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(event_row))
}

pub async fn get_event(
    pool: &MySqlPool,
    project_id: u64,
    issue_id: u64,
    event_id: &str,
) -> Result<Option<EventRow>, sqlx::Error> {
    let row: Option<EventTuple> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {EVENT_COLS} FROM events WHERE project_id = ? AND issue_id = ? AND event_id = ?"
    )))
    .bind(project_id)
    .bind(issue_id)
    .bind(event_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(event_row))
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct LogRow {
    pub id: u64,
    pub occurred_at: DateTime<Utc>,
    pub level: String,
    pub body: String,
    pub trace_id: Option<String>,
    pub attributes: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct LogFilter {
    pub level: Option<String>,
    pub search: Option<String>,
}

/// Échappe `%`, `_` et `\` pour une recherche LIKE littérale.
fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

pub async fn list_logs(
    pool: &MySqlPool,
    project_id: u64,
    filter: &LogFilter,
    limit: u32,
) -> Result<Vec<LogRow>, sqlx::Error> {
    let pattern = filter.search.as_deref().filter(|s| !s.is_empty()).map(|s| format!("%{}%", like_escape(s)));
    sqlx::query_as(
        "SELECT id, occurred_at, level, body, trace_id, attributes FROM logs \
         WHERE project_id = ? AND (? IS NULL OR level = ?) AND (? IS NULL OR body LIKE ?) \
         ORDER BY occurred_at DESC, id DESC LIMIT ?",
    )
    .bind(project_id)
    .bind(&filter.level)
    .bind(&filter.level)
    .bind(&pattern)
    .bind(&pattern)
    .bind(limit)
    .fetch_all(pool)
    .await
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ReplayRow {
    pub replay_id: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub duration_seconds: i64,
    pub segment_count: i64,
    pub urls: Option<String>,
    pub user_label: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
    pub release_name: Option<String>,
    pub environment: Option<String>,
    pub error_ids: Option<String>,
    /// 1 si un segment a été reçu dans les `LIVE_WINDOW_SECS` dernières secondes (session en cours).
    pub live: i64,
}

/// Une session est « en direct » tant qu'elle a reçu un segment depuis moins de ce délai : le SDK Sentry
/// en envoie toutes les 5 à 15 secondes tant que l'utilisateur est actif.
pub const LIVE_WINDOW_SECS: u32 = 60;

const REPLAY_SELECT: &str = "SELECT r.replay_id, r.first_seen, r.last_seen, \
    TIMESTAMPDIFF(SECOND, r.first_seen, r.last_seen) AS duration_seconds, \
    (SELECT COUNT(*) FROM replay_segments s WHERE s.project_id = r.project_id AND s.replay_id = r.replay_id) AS segment_count, \
    r.urls, r.user_label, r.browser, r.os, r.release_name, r.environment, r.error_ids, \
    CAST(EXISTS(SELECT 1 FROM replay_segments s WHERE s.project_id = r.project_id AND s.replay_id = r.replay_id \
      AND s.received_at > CURRENT_TIMESTAMP(3) - INTERVAL 60 SECOND) AS SIGNED) AS live \
    FROM replays r WHERE r.project_id = ?";

pub async fn list_replays(pool: &MySqlPool, project_id: u64, limit: u32) -> Result<Vec<ReplayRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!("{REPLAY_SELECT} ORDER BY r.last_seen DESC, r.id DESC LIMIT ?")))
        .bind(project_id)
        .bind(limit)
        .fetch_all(pool)
        .await
}

pub async fn get_replay(pool: &MySqlPool, project_id: u64, replay_id: &str) -> Result<Option<ReplayRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!("{REPLAY_SELECT} AND r.replay_id = ?")))
        .bind(project_id)
        .bind(replay_id)
        .fetch_optional(pool)
        .await
}

/// Événements rrweb d'un replay : tous les segments, dans l'ordre. `None` si le replay n'existe pas.
/// Un segment absent ou illisible est ignoré plutôt que de rendre tout le replay inutilisable.
pub async fn replay_events(
    pool: &MySqlPool,
    store: &BlobStore,
    project_id: u64,
    replay_id: &str,
) -> Result<Option<Vec<Value>>, sqlx::Error> {
    if get_replay(pool, project_id, replay_id).await?.is_none() {
        return Ok(None);
    }
    let segments: Vec<(String,)> = sqlx::query_as(
        "SELECT blob_key FROM replay_segments WHERE project_id = ? AND replay_id = ? ORDER BY segment_id",
    )
    .bind(project_id)
    .bind(replay_id)
    .fetch_all(pool)
    .await?;

    let mut events = Vec::new();
    for (key,) in segments {
        let Ok(Some(bytes)) = store.get(&key).await else { continue };
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(Value::Array(items)) => events.extend(items),
            Ok(obj @ Value::Object(_)) => events.push(obj),
            _ => tracing::warn!("segment de replay illisible : {key}"),
        }
    }
    Ok(Some(events))
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct LinkedError {
    pub event_id: String,
    pub issue_id: u64,
    pub title: String,
}

/// Erreurs du projet correspondant à ces identifiants d'événement (ceux inconnus sont ignorés).
pub async fn linked_errors(pool: &MySqlPool, project_id: u64, event_ids: &[String]) -> Result<Vec<LinkedError>, sqlx::Error> {
    if event_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; event_ids.len()].join(", ");
    let sql = format!(
        "SELECT e.event_id, e.issue_id, e.title FROM events e WHERE e.project_id = ? AND e.event_id IN ({placeholders}) \
         ORDER BY e.occurred_at"
    );
    let mut q = sqlx::query_as(sqlx::AssertSqlSafe(sql)).bind(project_id);
    for id in event_ids {
        q = q.bind(id);
    }
    q.fetch_all(pool).await
}
