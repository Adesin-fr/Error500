//! Ingestion : transforme les items d'une enveloppe Sentry en lignes SQL et blobs.

use crate::envelope::Envelope;
use crate::event::{normalize, NormalizedEvent};
use crate::projects::Project;
use crate::storage::BlobStore;
use bytes::Bytes;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use flate2::read::{GzDecoder, ZlibDecoder};
use serde_json::Value;
use sqlx::MySqlPool;
use std::io::Read;

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("payload invalide : {0}")]
    BadPayload(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Storage(#[from] object_store::Error),
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IngestSummary {
    pub events: usize,
    pub logs: usize,
    pub replay_segments: usize,
    pub ignored: usize,
}

fn trunc(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn trunc_opt(s: &Option<String>, max: usize) -> Option<String> {
    s.as_deref().map(|s| trunc(s, max))
}

fn naive(dt: DateTime<Utc>) -> NaiveDateTime {
    dt.naive_utc()
}

fn secs_to_dt(secs: f64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt((secs * 1000.0) as i64).single().unwrap_or_else(Utc::now)
}

pub async fn ingest_envelope(
    pool: &MySqlPool,
    store: &BlobStore,
    project: &Project,
    envelope: &Envelope,
) -> Result<IngestSummary, IngestError> {
    let mut summary = IngestSummary::default();
    let mut replay_id = envelope.event_id.clone();
    for item in envelope.items.iter().filter(|i| i.kind == "replay_event") {
        let payload: Value = serde_json::from_slice(&item.payload)
            .map_err(|e| IngestError::BadPayload(format!("replay_event : {e}")))?;
        if let Some(id) = payload.get("replay_id").and_then(Value::as_str) {
            replay_id = Some(id.to_string());
        }
        upsert_replay(pool, project, &payload).await?;
    }

    for item in &envelope.items {
        match item.kind.as_str() {
            "event" => {
                let payload: Value = serde_json::from_slice(&item.payload)
                    .map_err(|e| IngestError::BadPayload(format!("event : {e}")))?;
                if payload.get("type").and_then(Value::as_str) == Some("transaction") {
                    summary.ignored += 1;
                } else if ingest_event_payload(pool, project, &payload).await? {
                    summary.events += 1;
                }
            }
            "log" => {
                let payload: Value = serde_json::from_slice(&item.payload)
                    .map_err(|e| IngestError::BadPayload(format!("log : {e}")))?;
                summary.logs += ingest_logs(pool, project, &payload).await?;
            }
            "replay_recording" => {
                let id = replay_id
                    .as_deref()
                    .ok_or_else(|| IngestError::BadPayload("replay_recording sans replay_id".into()))?;
                ingest_recording(pool, store, project, id, &item.payload).await?;
                summary.replay_segments += 1;
            }
            "replay_event" => {}
            _ => summary.ignored += 1,
        }
    }
    Ok(summary)
}

/// Stocke un événement d'erreur (aussi utilisé par l'endpoint historique `/store/`).
/// Renvoie `false` si l'événement existait déjà (réémission du SDK).
pub async fn ingest_event_payload(
    pool: &MySqlPool,
    project: &Project,
    payload: &Value,
) -> Result<bool, IngestError> {
    let ev: NormalizedEvent = normalize(payload);
    let mut tx = pool.begin().await?;

    let exists: Option<(u64,)> = sqlx::query_as("SELECT id FROM events WHERE project_id = ? AND event_id = ?")
        .bind(project.id)
        .bind(&ev.event_id)
        .fetch_optional(&mut *tx)
        .await?;
    if exists.is_some() {
        return Ok(false);
    }

    let ts = naive(ev.timestamp);
    let title = trunc(&ev.title, 512);
    let level = trunc(&ev.level, 16);
    let issue_id = sqlx::query(
        "INSERT INTO issues (project_id, fingerprint, title, level, first_seen, last_seen, event_count) \
         VALUES (?, ?, ?, ?, ?, ?, 1) \
         ON DUPLICATE KEY UPDATE id = LAST_INSERT_ID(id), event_count = event_count + 1, \
           first_seen = LEAST(first_seen, VALUES(first_seen)), \
           last_seen = GREATEST(last_seen, VALUES(last_seen)), level = VALUES(level)",
    )
    .bind(project.id)
    .bind(&ev.fingerprint)
    .bind(&title)
    .bind(&level)
    .bind(ts)
    .bind(ts)
    .execute(&mut *tx)
    .await?
    .last_insert_id();

    let res = sqlx::query(
        "INSERT INTO events (project_id, issue_id, event_id, level, title, message, platform, release_name, \
         environment, transaction_name, exception_type, exception_value, occurred_at, payload) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(project.id)
    .bind(issue_id)
    .bind(&ev.event_id)
    .bind(&level)
    .bind(&title)
    .bind(&ev.message)
    .bind(trunc_opt(&ev.platform, 64))
    .bind(trunc_opt(&ev.release, 255))
    .bind(trunc_opt(&ev.environment, 255))
    .bind(trunc_opt(&ev.transaction, 512))
    .bind(trunc_opt(&ev.exception_type, 255))
    .bind(&ev.exception_value)
    .bind(ts)
    .bind(payload.to_string())
    .execute(&mut *tx)
    .await;
    match res {
        Ok(_) => {}
        // Course avec une réémission concurrente : on abandonne la transaction (issue incluse).
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => return Ok(false),
        Err(e) => return Err(e.into()),
    }
    tx.commit().await?;
    Ok(true)
}

async fn ingest_logs(pool: &MySqlPool, project: &Project, payload: &Value) -> Result<usize, IngestError> {
    let Some(items) = payload.get("items").and_then(Value::as_array) else {
        return Ok(0);
    };
    let mut stored = 0;
    for item in items {
        let Some(body) = item.get("body").and_then(Value::as_str) else { continue };
        let ts = item.get("timestamp").and_then(Value::as_f64).map(secs_to_dt).unwrap_or_else(Utc::now);
        let level = item.get("level").and_then(Value::as_str).unwrap_or("info");
        let trace_id = item.get("trace_id").and_then(Value::as_str).map(|t| trunc(t, 32));
        let attributes = item.get("attributes").filter(|a| !a.is_null()).map(Value::to_string);
        sqlx::query("INSERT INTO logs (project_id, occurred_at, level, body, trace_id, attributes) VALUES (?, ?, ?, ?, ?, ?)")
            .bind(project.id)
            .bind(naive(ts))
            .bind(trunc(level, 16))
            .bind(body)
            .bind(trace_id)
            .bind(attributes)
            .execute(pool)
            .await?;
        stored += 1;
    }
    Ok(stored)
}

async fn upsert_replay(pool: &MySqlPool, project: &Project, payload: &Value) -> Result<(), IngestError> {
    let replay_id = payload
        .get("replay_id")
        .and_then(Value::as_str)
        .ok_or_else(|| IngestError::BadPayload("replay_event sans replay_id".into()))?;
    let ts = payload.get("timestamp").and_then(Value::as_f64).map(secs_to_dt).unwrap_or_else(Utc::now);
    let start = payload.get("replay_start_timestamp").and_then(Value::as_f64).map(secs_to_dt).unwrap_or(ts);
    let urls = payload.get("urls").filter(|u| !u.is_null()).map(Value::to_string);
    let field = |k: &str| payload.get(k).and_then(Value::as_str).map(|s| trunc(s, 255));

    sqlx::query(
        "INSERT INTO replays (project_id, replay_id, first_seen, last_seen, urls, release_name, environment) \
         VALUES (?, ?, ?, ?, ?, ?, ?) \
         ON DUPLICATE KEY UPDATE first_seen = LEAST(first_seen, VALUES(first_seen)), \
           last_seen = GREATEST(last_seen, VALUES(last_seen)), urls = COALESCE(VALUES(urls), urls), \
           release_name = COALESCE(VALUES(release_name), release_name), \
           environment = COALESCE(VALUES(environment), environment)",
    )
    .bind(project.id)
    .bind(replay_id)
    .bind(naive(start))
    .bind(naive(ts))
    .bind(urls)
    .bind(field("release"))
    .bind(field("environment"))
    .execute(pool)
    .await?;
    Ok(())
}

/// Décompresse un corps de segment (zlib ou gzip) ; renvoie le corps tel quel s'il est en clair.
fn decompress(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let ok = match body {
        [0x78, ..] => ZlibDecoder::new(body).read_to_end(&mut out).is_ok(),
        [0x1f, 0x8b, ..] => GzDecoder::new(body).read_to_end(&mut out).is_ok(),
        _ => false,
    };
    if ok { out } else { body.to_vec() }
}

async fn ingest_recording(
    pool: &MySqlPool,
    store: &BlobStore,
    project: &Project,
    replay_id: &str,
    payload: &[u8],
) -> Result<(), IngestError> {
    let nl = payload
        .iter()
        .position(|&b| b == b'\n')
        .ok_or_else(|| IngestError::BadPayload("replay_recording sans en-tête".into()))?;
    let header: Value = serde_json::from_slice(&payload[..nl])
        .map_err(|e| IngestError::BadPayload(format!("en-tête replay_recording : {e}")))?;
    let segment_id = header
        .get("segment_id")
        .and_then(Value::as_u64)
        .ok_or_else(|| IngestError::BadPayload("segment_id manquant".into()))?;

    let body = decompress(&payload[nl + 1..]);
    let key = format!("replays/{}/{replay_id}/{segment_id}", project.id);
    let size = body.len() as u64;
    store.put(&key, Bytes::from(body)).await?;

    // Garantit l'existence de la ligne `replays` même si le `replay_event` est arrivé séparément.
    sqlx::query("INSERT IGNORE INTO replays (project_id, replay_id, first_seen, last_seen) VALUES (?, ?, NOW(3), NOW(3))")
        .bind(project.id)
        .bind(replay_id)
        .execute(pool)
        .await?;
    sqlx::query(
        "INSERT INTO replay_segments (project_id, replay_id, segment_id, blob_key, size_bytes) \
         VALUES (?, ?, ?, ?, ?) ON DUPLICATE KEY UPDATE blob_key = VALUES(blob_key), size_bytes = VALUES(size_bytes)",
    )
    .bind(project.id)
    .bind(replay_id)
    .bind(segment_id)
    .bind(&key)
    .bind(size)
    .execute(pool)
    .await?;
    Ok(())
}
