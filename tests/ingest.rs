mod common;

use bytes::Bytes;
use common::TestDb;
use error500::envelope::Envelope;
use error500::ingest::{self, IngestSummary};
use error500::projects::{self, Project};
use error500::storage::BlobStore;
use error500::users;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use serde_json::json;
use std::io::Write;

const EVENT_ID: &str = "9ec79c33ec9942ab8353589fcb2e04dc";

async fn setup() -> (TestDb, BlobStore, Project) {
    let db = TestDb::new().await;
    let u = users::create_user(&db.pool, "a@example.com", "A", "correct horse").await.unwrap();
    let p = projects::create_project(&db.pool, u.id, "App").await.unwrap();
    (db, BlobStore::memory(), p)
}

/// Construit une enveloppe : `items` = (type, payload).
fn envelope(header: serde_json::Value, items: &[(&str, &[u8])]) -> Envelope {
    let mut raw = format!("{header}\n").into_bytes();
    for (kind, payload) in items {
        raw.extend(format!("{{\"type\":\"{kind}\",\"length\":{}}}\n", payload.len()).bytes());
        raw.extend(*payload);
        raw.push(b'\n');
    }
    Envelope::parse(&raw).unwrap()
}

fn error_event(event_id: &str, value: &str, function: &str) -> Vec<u8> {
    json!({
        "event_id": event_id, "level": "error", "platform": "javascript",
        "timestamp": 1700000000.0,
        "exception": {"values": [{"type": "TypeError", "value": value,
            "stacktrace": {"frames": [{"filename": "app.js", "function": function, "in_app": true}]}}]}
    })
    .to_string()
    .into_bytes()
}

async fn count(db: &TestDb, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
        .fetch_one(&db.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn error_event_creates_issue_and_event() {
    let (db, store, p) = setup().await;
    let env = envelope(json!({"event_id": EVENT_ID}), &[("event", &error_event(EVENT_ID, "x is undefined", "pay"))]);
    let s = ingest::ingest_envelope(&db.pool, &store, &p, &env).await.unwrap();
    assert_eq!(s, IngestSummary { events: 1, ..Default::default() });

    let (title, n): (String, u64) = sqlx::query_as("SELECT title, event_count FROM issues").fetch_one(&db.pool).await.unwrap();
    assert_eq!(title, "TypeError: x is undefined");
    assert_eq!(n, 1);
    assert_eq!(count(&db, "events").await, 1);
    db.cleanup().await;
}

#[tokio::test]
async fn same_error_site_groups_into_one_issue() {
    let (db, store, p) = setup().await;
    for (id, msg) in [("a".repeat(32), "user 1"), ("b".repeat(32), "user 2")] {
        let env = envelope(json!({}), &[("event", &error_event(&id, msg, "pay"))]);
        ingest::ingest_envelope(&db.pool, &store, &p, &env).await.unwrap();
    }
    assert_eq!(count(&db, "issues").await, 1);
    assert_eq!(count(&db, "events").await, 2);
    let n: u64 = sqlx::query_scalar("SELECT event_count FROM issues").fetch_one(&db.pool).await.unwrap();
    assert_eq!(n, 2);
    db.cleanup().await;
}

#[tokio::test]
async fn different_error_site_creates_new_issue() {
    let (db, store, p) = setup().await;
    for (id, f) in [("a".repeat(32), "pay"), ("b".repeat(32), "refund")] {
        let env = envelope(json!({}), &[("event", &error_event(&id, "m", f))]);
        ingest::ingest_envelope(&db.pool, &store, &p, &env).await.unwrap();
    }
    assert_eq!(count(&db, "issues").await, 2);
    db.cleanup().await;
}

#[tokio::test]
async fn retried_event_is_deduplicated() {
    let (db, store, p) = setup().await;
    let env = envelope(json!({}), &[("event", &error_event(EVENT_ID, "m", "pay"))]);
    ingest::ingest_envelope(&db.pool, &store, &p, &env).await.unwrap();
    let s = ingest::ingest_envelope(&db.pool, &store, &p, &env).await.unwrap();
    assert_eq!(s.events, 0);
    assert_eq!(count(&db, "events").await, 1);
    let n: u64 = sqlx::query_scalar("SELECT event_count FROM issues").fetch_one(&db.pool).await.unwrap();
    assert_eq!(n, 1);
    db.cleanup().await;
}

#[tokio::test]
async fn same_event_id_in_two_projects_is_kept_in_both() {
    let (db, store, p1) = setup().await;
    let p2 = projects::create_project(&db.pool, 1, "Other").await.unwrap();
    let env = envelope(json!({}), &[("event", &error_event(EVENT_ID, "m", "pay"))]);
    ingest::ingest_envelope(&db.pool, &store, &p1, &env).await.unwrap();
    ingest::ingest_envelope(&db.pool, &store, &p2, &env).await.unwrap();
    assert_eq!(count(&db, "events").await, 2);
    assert_eq!(count(&db, "issues").await, 2);
    db.cleanup().await;
}

#[tokio::test]
async fn transactions_and_other_items_are_ignored_without_error() {
    let (db, store, p) = setup().await;
    let tx = json!({"type": "transaction", "event_id": EVENT_ID}).to_string();
    let env = envelope(json!({}), &[("transaction", tx.as_bytes()), ("session", b"{}"), ("client_report", b"{}")]);
    let s = ingest::ingest_envelope(&db.pool, &store, &p, &env).await.unwrap();
    assert_eq!(s, IngestSummary { ignored: 3, ..Default::default() });
    assert_eq!(count(&db, "events").await, 0);
    db.cleanup().await;
}

#[tokio::test]
async fn log_items_are_stored_one_row_per_entry() {
    let (db, store, p) = setup().await;
    let payload = json!({"items": [
        {"timestamp": 1700000000.25, "level": "info", "body": "started", "trace_id": "0af7651916cd43dd8448eb211c80319c",
         "attributes": {"user.id": {"value": "42", "type": "string"}}},
        {"timestamp": 1700000001.0, "level": "error", "body": "failed"}
    ]})
    .to_string();
    let env = envelope(json!({}), &[("log", payload.as_bytes())]);
    let s = ingest::ingest_envelope(&db.pool, &store, &p, &env).await.unwrap();
    assert_eq!(s.logs, 2);

    let rows: Vec<(String, String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT level, body, trace_id, attributes FROM logs ORDER BY occurred_at")
            .fetch_all(&db.pool).await.unwrap();
    assert_eq!(rows[0].0, "info");
    assert_eq!(rows[0].1, "started");
    assert_eq!(rows[0].2.as_deref(), Some("0af7651916cd43dd8448eb211c80319c"));
    assert!(rows[0].3.as_deref().unwrap().contains("user.id"));
    assert_eq!(rows[1].1, "failed");
    assert_eq!(rows[1].2, None);
    db.cleanup().await;
}

fn recording(segment_id: u32, body: &[u8]) -> Vec<u8> {
    let mut v = format!("{{\"segment_id\":{segment_id}}}\n").into_bytes();
    v.extend(body);
    v
}

fn replay_event(replay_id: &str, segment_id: u32, ts: f64) -> Vec<u8> {
    json!({"type": "replay_event", "replay_id": replay_id, "segment_id": segment_id,
           "timestamp": ts, "replay_start_timestamp": 1700000000.0,
           "urls": ["https://app.example.com/checkout"], "release": "1.0", "environment": "prod"})
    .to_string().into_bytes()
}

const REPLAY_ID: &str = "11111111111111111111111111111111";

#[tokio::test]
async fn replay_segment_blob_and_metadata_are_stored() {
    let (db, store, p) = setup().await;
    let rrweb = br#"[{"type":4,"data":{"href":"https://app.example.com"},"timestamp":1700000000000}]"#;
    let env = envelope(
        json!({"event_id": REPLAY_ID}),
        &[("replay_event", &replay_event(REPLAY_ID, 0, 1700000010.0)), ("replay_recording", &recording(0, rrweb))],
    );
    let s = ingest::ingest_envelope(&db.pool, &store, &p, &env).await.unwrap();
    assert_eq!(s.replay_segments, 1);

    let (key, size): (String, u64) = sqlx::query_as("SELECT blob_key, size_bytes FROM replay_segments").fetch_one(&db.pool).await.unwrap();
    assert_eq!(key, format!("replays/{}/{REPLAY_ID}/0", p.id));
    assert_eq!(size, rrweb.len() as u64);
    assert_eq!(store.get(&key).await.unwrap(), Some(Bytes::from_static(rrweb)));

    let (urls, env_name): (String, String) = sqlx::query_as("SELECT urls, environment FROM replays").fetch_one(&db.pool).await.unwrap();
    assert!(urls.contains("app.example.com/checkout"));
    assert_eq!(env_name, "prod");
    db.cleanup().await;
}

#[tokio::test]
async fn zlib_compressed_recording_is_stored_decompressed() {
    let (db, store, p) = setup().await;
    let rrweb = br#"[{"type":2,"data":{},"timestamp":1700000000000}]"#;
    let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
    enc.write_all(rrweb).unwrap();
    let compressed = enc.finish().unwrap();
    let env = envelope(json!({"event_id": REPLAY_ID}), &[("replay_recording", &recording(0, &compressed))]);
    ingest::ingest_envelope(&db.pool, &store, &p, &env).await.unwrap();
    let key = format!("replays/{}/{REPLAY_ID}/0", p.id);
    assert_eq!(store.get(&key).await.unwrap(), Some(Bytes::from_static(rrweb)));
    db.cleanup().await;
}

#[tokio::test]
async fn several_segments_belong_to_one_replay() {
    let (db, store, p) = setup().await;
    for (seg, ts) in [(0u32, 1700000010.0), (1, 1700000020.0)] {
        let env = envelope(
            json!({"event_id": REPLAY_ID}),
            &[("replay_event", &replay_event(REPLAY_ID, seg, ts)), ("replay_recording", &recording(seg, b"[]"))],
        );
        ingest::ingest_envelope(&db.pool, &store, &p, &env).await.unwrap();
    }
    assert_eq!(count(&db, "replays").await, 1);
    assert_eq!(count(&db, "replay_segments").await, 2);
    let secs: i64 = sqlx::query_scalar("SELECT TIMESTAMPDIFF(SECOND, first_seen, last_seen) FROM replays").fetch_one(&db.pool).await.unwrap();
    assert_eq!(secs, 20);
    db.cleanup().await;
}

#[tokio::test]
async fn store_endpoint_payload_is_ingested_like_an_event() {
    let (db, _store, p) = setup().await;
    let payload: serde_json::Value = serde_json::from_slice(&error_event(EVENT_ID, "m", "pay")).unwrap();
    let s = ingest::ingest_event_payload(&db.pool, &p, &payload).await.unwrap();
    assert!(s);
    assert_eq!(count(&db, "events").await, 1);
    db.cleanup().await;
}

// --- Compatibilité du format de replay du SDK Sentry (navigateur) ---

fn raw_envelope(parts: &[&[u8]]) -> Envelope {
    Envelope::parse(&parts.concat()).unwrap()
}

#[tokio::test]
async fn raw_deflate_recording_is_decompressed() {
    use flate2::write::DeflateEncoder;
    let (db, store, p) = setup().await;
    let rrweb = br#"[{"type":3,"data":{"source":1},"timestamp":1700000000000}]"#;
    let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
    enc.write_all(rrweb).unwrap();
    let raw = enc.finish().unwrap();
    let env = envelope(json!({"event_id": REPLAY_ID}), &[("replay_recording", &recording(0, &raw))]);
    ingest::ingest_envelope(&db.pool, &store, &p, &env).await.unwrap();
    let key = format!("replays/{}/{REPLAY_ID}/0", p.id);
    assert_eq!(store.get(&key).await.unwrap(), Some(Bytes::from_static(rrweb)));
    db.cleanup().await;
}

#[tokio::test]
async fn sdk_shaped_envelope_without_item_length_on_replay_event() {
    // Le SDK JS écrit `{"type":"replay_event"}` sans length : le payload s'arrête à la fin de ligne.
    let (db, store, p) = setup().await;
    let re = replay_event(REPLAY_ID, 0, 1700000010.0);
    let rec = recording(0, b"[]");
    let env = raw_envelope(&[
        format!("{{\"event_id\":\"{REPLAY_ID}\",\"sent_at\":\"2023-11-14T22:13:20.000Z\",\"sdk\":{{\"name\":\"sentry.javascript.browser\"}}}}\n").as_bytes(),
        b"{\"type\":\"replay_event\"}\n",
        &re,
        b"\n",
        format!("{{\"type\":\"replay_recording\",\"length\":{}}}\n", rec.len()).as_bytes(),
        &rec,
        b"\n",
    ]);
    let s = ingest::ingest_envelope(&db.pool, &store, &p, &env).await.unwrap();
    assert_eq!(s.replay_segments, 1);
    assert_eq!(count(&db, "replays").await, 1);
    db.cleanup().await;
}

#[tokio::test]
async fn recording_listed_before_replay_event_still_resolves_replay_id() {
    let (db, store, p) = setup().await;
    // L'identifiant de l'enveloppe diffère du replay_id : seul le replay_event le donne.
    let env = envelope(
        json!({"event_id": "ffffffffffffffffffffffffffffffff"}),
        &[("replay_recording", &recording(0, b"[]")), ("replay_event", &replay_event(REPLAY_ID, 0, 1700000010.0))],
    );
    ingest::ingest_envelope(&db.pool, &store, &p, &env).await.unwrap();
    assert!(store.get(&format!("replays/{}/{REPLAY_ID}/0", p.id)).await.unwrap().is_some());
    db.cleanup().await;
}

#[tokio::test]
async fn replay_event_metadata_is_extracted() {
    let (db, store, p) = setup().await;
    let re = json!({
        "type": "replay_event", "replay_id": REPLAY_ID, "segment_id": 0, "replay_type": "session",
        "timestamp": 1700000010.0, "replay_start_timestamp": 1700000000.0,
        "urls": ["https://app.example.com/"], "error_ids": ["e1", "e2"],
        "user": {"email": "jane@example.com", "id": "42"},
        "contexts": {"browser": {"name": "Chrome", "version": "120.0"}, "os": {"name": "macOS", "version": "14"}}
    })
    .to_string();
    let env = envelope(json!({"event_id": REPLAY_ID}), &[("replay_event", re.as_bytes())]);
    ingest::ingest_envelope(&db.pool, &store, &p, &env).await.unwrap();
    let (user, browser, os, ty, errs): (String, String, String, String, String) =
        sqlx::query_as("SELECT user_label, browser, os, replay_type, error_ids FROM replays")
            .fetch_one(&db.pool).await.unwrap();
    assert_eq!(user, "jane@example.com");
    assert_eq!(browser, "Chrome 120.0");
    assert_eq!(os, "macOS 14");
    assert_eq!(ty, "session");
    assert!(errs.contains("e1") && errs.contains("e2"));
    db.cleanup().await;
}
