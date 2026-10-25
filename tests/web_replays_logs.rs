mod common;
mod web_common;

use axum::http::StatusCode;
use error500::envelope::Envelope;
use serde_json::json;
use web_common::{hex_id, web_app, W};

async fn setup(w: &W) -> (error500::projects::Project, String) {
    w.login_as("admin@example.com").await;
    let (alice, cookie) = w.login_as("alice@example.com").await;
    (w.project_for(alice, "App").await, cookie)
}

async fn replay(w: &W, p: &error500::projects::Project, rid: &str, seg: u32, ts: f64, events: &str, error_ids: &[String]) {
    let re = json!({"type": "replay_event", "replay_id": rid, "segment_id": seg, "timestamp": ts,
        "replay_start_timestamp": 1_700_000_000.0, "urls": ["https://app.example.com/a", "https://app.example.com/b"],
        "error_ids": error_ids, "user": {"email": "u@x.io"},
        "contexts": {"browser": {"name": "Chrome", "version": "120"}}}).to_string();
    let rec = format!("{{\"segment_id\":{seg}}}\n{events}");
    let raw = format!(
        "{{\"event_id\":\"{rid}\"}}\n{{\"type\":\"replay_event\",\"length\":{}}}\n{re}\n{{\"type\":\"replay_recording\",\"length\":{}}}\n{rec}\n",
        re.len(), rec.len()
    );
    error500::ingest::ingest_envelope(&w.db.pool, &w.store, p, &Envelope::parse(raw.as_bytes()).unwrap()).await.unwrap();
}

#[tokio::test]
async fn replays_index_lists_replays_with_parsed_urls() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    replay(&w, &p, &"1".repeat(32), 0, 1_700_000_030.0, "[]", &[]).await;
    let r = w.get(&format!("/projects/{}/replays", p.id), Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.component(), "Replays/Index");
    let list = r.props()["replays"].as_array().unwrap().clone();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["replay_id"], "1".repeat(32));
    assert_eq!(list[0]["urls"], json!(["https://app.example.com/a", "https://app.example.com/b"]));
    assert_eq!(list[0]["duration_seconds"], 30);
    assert_eq!(list[0]["browser"], "Chrome 120");
    w.db.cleanup().await;
}

#[tokio::test]
async fn replay_page_exposes_events_url_and_linked_errors() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    w.event(&p, &hex_id(7), "Boom", 1_700_000_010.0).await;
    let rid = "2".repeat(32);
    replay(&w, &p, &rid, 0, 1_700_000_030.0, "[]", &[hex_id(7), hex_id(404)]).await;
    let r = w.get(&format!("/projects/{}/replays/{rid}", p.id), Some(&cookie)).await;
    assert_eq!(r.component(), "Replays/Show");
    let props = r.props();
    assert_eq!(props["replay"]["replay_id"], rid);
    assert_eq!(props["events_url"], format!("/projects/{}/replays/{rid}/events", p.id));
    let errors = props["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "seules les erreurs connues du projet sont listées");
    assert_eq!(errors[0]["event_id"], hex_id(7));
    assert!(errors[0]["title"].as_str().unwrap().starts_with("Boom"));
    assert!(errors[0]["issue_id"].is_u64());
    w.db.cleanup().await;
}

#[tokio::test]
async fn replay_events_endpoint_returns_all_segments_as_json() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    let rid = "3".repeat(32);
    replay(&w, &p, &rid, 1, 1_700_000_030.0, r#"[{"type":3,"timestamp":2}]"#, &[]).await;
    replay(&w, &p, &rid, 0, 1_700_000_010.0, r#"[{"type":2,"timestamp":1}]"#, &[]).await;
    let r = w.get_html(&format!("/projects/{}/replays/{rid}/events", p.id), Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.headers["content-type"], "application/json");
    assert_eq!(r.json(), json!([{"type": 2, "timestamp": 1}, {"type": 3, "timestamp": 2}]));
    w.db.cleanup().await;
}

#[tokio::test]
async fn replay_pages_are_protected_and_scoped() {
    let w = web_app().await;
    let (p, _) = setup(&w).await;
    let (_, bob_cookie) = w.login_as("bob@example.com").await;
    let rid = "4".repeat(32);
    replay(&w, &p, &rid, 0, 1_700_000_030.0, "[]", &[]).await;
    for suffix in ["", "/events"] {
        let url = format!("/projects/{}/replays/{rid}{suffix}", p.id);
        assert!(w.get_html(&url, None).await.status.is_redirection(), "anonyme {suffix}");
        assert_eq!(w.get_html(&url, Some(&bob_cookie)).await.status, StatusCode::NOT_FOUND, "bob {suffix}");
    }
    w.db.cleanup().await;
}

#[tokio::test]
async fn unknown_replay_is_not_found() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    assert_eq!(w.get(&format!("/projects/{}/replays/nope", p.id), Some(&cookie)).await.status, StatusCode::NOT_FOUND);
    assert_eq!(w.get_html(&format!("/projects/{}/replays/nope/events", p.id), Some(&cookie)).await.status, StatusCode::NOT_FOUND);
    w.db.cleanup().await;
}

async fn logs(w: &W, p: &error500::projects::Project) {
    let payload = json!({"items": [
        {"timestamp": 1700000000.0, "level": "info", "body": "server started", "attributes": {"port": {"value": 8080, "type": "integer"}}},
        {"timestamp": 1700000001.0, "level": "error", "body": "db connection lost", "trace_id": "0af7651916cd43dd8448eb211c80319c"},
        {"timestamp": 1700000002.0, "level": "warn", "body": "slow request"},
    ]}).to_string();
    let raw = format!("{{}}\n{{\"type\":\"log\",\"length\":{}}}\n{payload}\n", payload.len());
    error500::ingest::ingest_envelope(&w.db.pool, &w.store, p, &Envelope::parse(raw.as_bytes()).unwrap()).await.unwrap();
}

#[tokio::test]
async fn logs_page_lists_newest_first_with_parsed_attributes() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    logs(&w, &p).await;
    let r = w.get(&format!("/projects/{}/logs", p.id), Some(&cookie)).await;
    assert_eq!(r.component(), "Logs/Index");
    let list = r.props()["logs"].as_array().unwrap().clone();
    assert_eq!(list.iter().map(|l| l["body"].as_str().unwrap()).collect::<Vec<_>>(), ["slow request", "db connection lost", "server started"]);
    assert_eq!(list[2]["attributes"]["port"]["value"], 8080);
    assert!(list[0]["attributes"].is_null());
    assert_eq!(list[1]["trace_id"], "0af7651916cd43dd8448eb211c80319c");
    assert_eq!(r.props()["filters"], json!({"level": null, "q": null}));
    w.db.cleanup().await;
}

#[tokio::test]
async fn logs_can_be_filtered_by_level_and_text() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    logs(&w, &p).await;
    let r = w.get(&format!("/projects/{}/logs?level=error", p.id), Some(&cookie)).await;
    assert_eq!(r.props()["logs"].as_array().unwrap().len(), 1);
    assert_eq!(r.props()["filters"]["level"], "error");
    let r = w.get(&format!("/projects/{}/logs?q=started", p.id), Some(&cookie)).await;
    assert_eq!(r.props()["logs"][0]["body"], "server started");
    let r = w.get(&format!("/projects/{}/logs?level=&q=", p.id), Some(&cookie)).await;
    assert_eq!(r.props()["logs"].as_array().unwrap().len(), 3, "filtres vides ignorés");
    w.db.cleanup().await;
}
