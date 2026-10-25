mod common;

use bytes::Bytes;
use common::TestDb;
use error500::envelope::Envelope;
use error500::ingest;
use error500::projects::{self, Project};
use error500::queries::{self, IssueStatus, LogFilter};
use error500::storage::BlobStore;
use error500::users;
use serde_json::json;

async fn setup() -> (TestDb, BlobStore, Project) {
    let db = TestDb::new().await;
    let u = users::create_user(&db.pool, "a@example.com", "A", "correct horse").await.unwrap();
    let p = projects::create_project(&db.pool, u.id, "App").await.unwrap();
    (db, BlobStore::memory(), p)
}

async fn send_event(db: &TestDb, p: &Project, id: &str, ty: &str, ts: f64) {
    let ev = json!({"event_id": id, "timestamp": ts, "release": "1.0", "environment": "prod",
        "exception": {"values": [{"type": ty, "value": "v"}]}}).to_string();
    let raw = format!("{{}}\n{{\"type\":\"event\",\"length\":{}}}\n{ev}\n", ev.len());
    ingest::ingest_envelope(&db.pool, &BlobStore::memory(), p, &Envelope::parse(raw.as_bytes()).unwrap()).await.unwrap();
}

fn id(n: u8) -> String {
    format!("{n:02x}").repeat(16)
}

#[tokio::test]
async fn issues_are_listed_newest_activity_first_with_counts() {
    let (db, _s, p) = setup().await;
    send_event(&db, &p, &id(1), "OldError", 1_700_000_000.0).await;
    send_event(&db, &p, &id(2), "NewError", 1_700_000_100.0).await;
    send_event(&db, &p, &id(3), "OldError", 1_700_000_200.0).await; // réactive OldError
    let issues = queries::list_issues(&db.pool, p.id, None, 50, 0).await.unwrap();
    assert_eq!(issues.len(), 2);
    assert!(issues[0].title.starts_with("OldError"));
    assert_eq!(issues[0].event_count, 2);
    assert_eq!(issues[0].status, IssueStatus::Unresolved);
    db.cleanup().await;
}

#[tokio::test]
async fn issues_are_scoped_to_their_project_and_filterable_by_status() {
    let (db, _s, p) = setup().await;
    let other = projects::create_project(&db.pool, 1, "Other").await.unwrap();
    send_event(&db, &p, &id(1), "A", 1_700_000_000.0).await;
    send_event(&db, &other, &id(2), "B", 1_700_000_000.0).await;
    let mine = queries::list_issues(&db.pool, p.id, None, 50, 0).await.unwrap();
    assert_eq!(mine.len(), 1);
    assert!(queries::get_issue(&db.pool, other.id, mine[0].id).await.unwrap().is_none(), "pas d'accès croisé");

    queries::set_issue_status(&db.pool, p.id, mine[0].id, IssueStatus::Resolved).await.unwrap();
    assert!(queries::list_issues(&db.pool, p.id, Some(IssueStatus::Unresolved), 50, 0).await.unwrap().is_empty());
    assert_eq!(queries::list_issues(&db.pool, p.id, Some(IssueStatus::Resolved), 50, 0).await.unwrap().len(), 1);
    db.cleanup().await;
}

#[tokio::test]
async fn pagination_limits_and_offsets() {
    let (db, _s, p) = setup().await;
    for n in 0..5u8 {
        send_event(&db, &p, &id(n + 1), &format!("E{n}"), 1_700_000_000.0 + f64::from(n)).await;
    }
    assert_eq!(queries::list_issues(&db.pool, p.id, None, 2, 0).await.unwrap().len(), 2);
    assert_eq!(queries::list_issues(&db.pool, p.id, None, 2, 4).await.unwrap().len(), 1);
    assert_eq!(queries::count_issues(&db.pool, p.id, None).await.unwrap(), 5);
    db.cleanup().await;
}

#[tokio::test]
async fn new_event_on_resolved_issue_reopens_it_but_not_ignored_ones() {
    let (db, _s, p) = setup().await;
    send_event(&db, &p, &id(1), "A", 1_700_000_000.0).await;
    send_event(&db, &p, &id(2), "B", 1_700_000_000.0).await;
    let issues = queries::list_issues(&db.pool, p.id, None, 50, 0).await.unwrap();
    let a = issues.iter().find(|i| i.title.starts_with('A')).unwrap().id;
    let b = issues.iter().find(|i| i.title.starts_with('B')).unwrap().id;
    queries::set_issue_status(&db.pool, p.id, a, IssueStatus::Resolved).await.unwrap();
    queries::set_issue_status(&db.pool, p.id, b, IssueStatus::Ignored).await.unwrap();
    send_event(&db, &p, &id(3), "A", 1_700_000_300.0).await;
    send_event(&db, &p, &id(4), "B", 1_700_000_300.0).await;
    assert_eq!(queries::get_issue(&db.pool, p.id, a).await.unwrap().unwrap().status, IssueStatus::Unresolved);
    assert_eq!(queries::get_issue(&db.pool, p.id, b).await.unwrap().unwrap().status, IssueStatus::Ignored);
    db.cleanup().await;
}

#[tokio::test]
async fn issue_events_latest_first_and_payload_is_returned() {
    let (db, _s, p) = setup().await;
    send_event(&db, &p, &id(1), "A", 1_700_000_000.0).await;
    send_event(&db, &p, &id(2), "A", 1_700_000_100.0).await;
    let issue = queries::list_issues(&db.pool, p.id, None, 1, 0).await.unwrap().remove(0);
    let events = queries::list_events(&db.pool, p.id, issue.id, 10).await.unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_id, id(2));
    let latest = queries::latest_event(&db.pool, p.id, issue.id).await.unwrap().unwrap();
    assert_eq!(latest.event_id, id(2));
    assert_eq!(latest.payload["exception"]["values"][0]["type"], "A");
    let older = queries::get_event(&db.pool, p.id, issue.id, &id(1)).await.unwrap().unwrap();
    assert_eq!(older.release.as_deref(), Some("1.0"));
    assert!(queries::get_event(&db.pool, p.id, issue.id, "nope").await.unwrap().is_none());
    db.cleanup().await;
}

async fn send_logs(db: &TestDb, p: &Project, entries: serde_json::Value) {
    let payload = json!({"items": entries}).to_string();
    let raw = format!("{{}}\n{{\"type\":\"log\",\"length\":{}}}\n{payload}\n", payload.len());
    ingest::ingest_envelope(&db.pool, &BlobStore::memory(), p, &Envelope::parse(raw.as_bytes()).unwrap()).await.unwrap();
}

#[tokio::test]
async fn logs_are_listed_newest_first_and_filtered() {
    let (db, _s, p) = setup().await;
    send_logs(&db, &p, json!([
        {"timestamp": 1700000000.0, "level": "info", "body": "server started"},
        {"timestamp": 1700000001.0, "level": "error", "body": "db connection lost"},
        {"timestamp": 1700000002.0, "level": "info", "body": "request handled"},
    ])).await;
    let all = queries::list_logs(&db.pool, p.id, &LogFilter::default(), 50).await.unwrap();
    assert_eq!(all.iter().map(|l| l.body.as_str()).collect::<Vec<_>>(), ["request handled", "db connection lost", "server started"]);

    let errors = queries::list_logs(&db.pool, p.id, &LogFilter { level: Some("error".into()), ..Default::default() }, 50).await.unwrap();
    assert_eq!(errors.len(), 1);
    let found = queries::list_logs(&db.pool, p.id, &LogFilter { search: Some("started".into()), ..Default::default() }, 50).await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].body, "server started");
    db.cleanup().await;
}

#[tokio::test]
async fn log_search_treats_percent_and_underscore_literally() {
    let (db, _s, p) = setup().await;
    send_logs(&db, &p, json!([
        {"timestamp": 1700000000.0, "level": "info", "body": "disk at 100%"},
        {"timestamp": 1700000001.0, "level": "info", "body": "disk at 100 percent"},
    ])).await;
    let found = queries::list_logs(&db.pool, p.id, &LogFilter { search: Some("100%".into()), ..Default::default() }, 50).await.unwrap();
    assert_eq!(found.len(), 1);
    db.cleanup().await;
}

fn recording(segment_id: u32, body: &str) -> Vec<u8> {
    let mut v = format!("{{\"segment_id\":{segment_id}}}\n").into_bytes();
    v.extend(body.as_bytes());
    v
}

async fn send_replay(db: &TestDb, store: &BlobStore, p: &Project, rid: &str, seg: u32, ts: f64, events: &str) {
    let re = json!({"type": "replay_event", "replay_id": rid, "segment_id": seg, "timestamp": ts,
        "replay_start_timestamp": 1_700_000_000.0, "urls": ["https://app/"], "user": {"email": "u@x.io"}}).to_string();
    let rec = recording(seg, events);
    let mut raw = format!("{{\"event_id\":\"{rid}\"}}\n{{\"type\":\"replay_event\",\"length\":{}}}\n{re}\n", re.len()).into_bytes();
    raw.extend(format!("{{\"type\":\"replay_recording\",\"length\":{}}}\n", rec.len()).bytes());
    raw.extend(rec);
    raw.push(b'\n');
    ingest::ingest_envelope(&db.pool, store, p, &Envelope::parse(&raw).unwrap()).await.unwrap();
}

#[tokio::test]
async fn replays_are_listed_with_segments_and_duration() {
    let (db, store, p) = setup().await;
    let r1 = "1".repeat(32);
    send_replay(&db, &store, &p, &r1, 0, 1_700_000_010.0, "[]").await;
    send_replay(&db, &store, &p, &r1, 1, 1_700_000_030.0, "[]").await;
    send_replay(&db, &store, &p, &"2".repeat(32), 0, 1_700_000_500.0, "[]").await;
    let list = queries::list_replays(&db.pool, p.id, 50).await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].replay_id, "2".repeat(32), "le plus récent d'abord");
    let first = list.iter().find(|r| r.replay_id == r1).unwrap();
    assert_eq!(first.segment_count, 2);
    assert_eq!(first.duration_seconds, 30);
    assert_eq!(first.user_label.as_deref(), Some("u@x.io"));
    db.cleanup().await;
}

#[tokio::test]
async fn replay_events_concatenate_segments_in_order() {
    let (db, store, p) = setup().await;
    let rid = "3".repeat(32);
    // segments envoyés dans le désordre
    send_replay(&db, &store, &p, &rid, 1, 1_700_000_030.0, r#"[{"type":3,"timestamp":2},{"type":3,"timestamp":3}]"#).await;
    send_replay(&db, &store, &p, &rid, 0, 1_700_000_010.0, r#"[{"type":2,"timestamp":1}]"#).await;
    let events = queries::replay_events(&db.pool, &store, p.id, &rid).await.unwrap().unwrap();
    let ts: Vec<i64> = events.iter().map(|e| e["timestamp"].as_i64().unwrap()).collect();
    assert_eq!(ts, vec![1, 2, 3]);
    db.cleanup().await;
}

#[tokio::test]
async fn replay_events_are_scoped_and_missing_replay_is_none() {
    let (db, store, p) = setup().await;
    let other = projects::create_project(&db.pool, 1, "Other").await.unwrap();
    let rid = "4".repeat(32);
    send_replay(&db, &store, &p, &rid, 0, 1_700_000_010.0, "[]").await;
    assert!(queries::replay_events(&db.pool, &store, other.id, &rid).await.unwrap().is_none());
    assert!(queries::replay_events(&db.pool, &store, p.id, "nope").await.unwrap().is_none());
    assert!(queries::get_replay(&db.pool, p.id, &rid).await.unwrap().is_some());
    db.cleanup().await;
}

#[tokio::test]
async fn replay_events_tolerate_a_segment_that_is_not_json() {
    let (db, store, p) = setup().await;
    let rid = "5".repeat(32);
    send_replay(&db, &store, &p, &rid, 0, 1_700_000_010.0, r#"[{"type":2,"timestamp":1}]"#).await;
    store.put(&format!("replays/{}/{rid}/1", p.id), Bytes::from_static(b"\x00garbage")).await.unwrap();
    sqlx::query("INSERT INTO replay_segments (project_id, replay_id, segment_id, blob_key, size_bytes) VALUES (?, ?, 1, ?, 8)")
        .bind(p.id).bind(&rid).bind(format!("replays/{}/{rid}/1", p.id)).execute(&db.pool).await.unwrap();
    let events = queries::replay_events(&db.pool, &store, p.id, &rid).await.unwrap().unwrap();
    assert_eq!(events.len(), 1, "le segment illisible est ignoré");
    db.cleanup().await;
}
