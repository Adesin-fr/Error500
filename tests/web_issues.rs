mod common;
mod web_common;

use axum::http::StatusCode;
use bytes::Bytes;
use serde_json::json;
use web_common::{hex_id, web_app, W};

const MAP: &str = r#"{"version":3,"sources":["src/app.ts"],"names":["handleClick"],
 "sourcesContent":["line1\nline2\nfunction handleClick() {\n  throw 1\n}\nline6"],"mappings":"SAEIA"}"#;
const URL: &str = "https://app.example.com/static/app.min.js";

/// (project, cookie du propriétaire) ; un admin existe déjà pour que les rôles soient « normaux ».
async fn setup(w: &W) -> (error500::projects::Project, String) {
    w.login_as("admin@example.com").await;
    let (alice, cookie) = w.login_as("alice@example.com").await;
    (w.project_for(alice, "App").await, cookie)
}

async fn first_issue_id(w: &W, project_id: u64) -> u64 {
    error500::queries::list_issues(&w.db.pool, project_id, None, 1, 0).await.unwrap()[0].id
}

fn js_event(id: &str, ts: f64) -> serde_json::Value {
    json!({
        "event_id": id, "timestamp": ts, "release": "1.0", "environment": "prod", "platform": "javascript",
        "tags": {"browser": "Chrome", "server_name": "web-1"},
        "user": {"email": "jane@example.com"},
        "request": {"url": "https://app.example.com/checkout", "method": "GET"},
        "breadcrumbs": {"values": [
            {"timestamp": 1700000000.0, "category": "ui.click", "message": "button#pay", "level": "info"},
            {"timestamp": 1700000001.0, "category": "console", "message": "about to fail", "level": "warning"}]},
        "exception": {"values": [{"type": "TypeError", "value": "x is undefined", "stacktrace": {"frames": [
            {"abs_path": URL, "filename": URL, "function": "t", "lineno": 1, "colno": 10, "in_app": true}]}}]}
    })
}

#[tokio::test]
async fn issue_page_shows_latest_event_with_context() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    w.event_payload(&p, js_event(&hex_id(1), 1_700_000_000.0)).await;
    w.event_payload(&p, js_event(&hex_id(2), 1_700_000_100.0)).await;
    let iid = first_issue_id(&w, p.id).await;

    let r = w.get(&format!("/projects/{}/issues/{iid}", p.id), Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.component(), "Issues/Show");
    let props = r.props();
    assert_eq!(props["issue"]["title"], "TypeError: x is undefined");
    assert_eq!(props["issue"]["event_count"], 2);
    assert_eq!(props["event"]["event_id"], hex_id(2));
    assert_eq!(props["event"]["release"], "1.0");
    assert_eq!(props["event"]["user"]["email"], "jane@example.com");
    assert_eq!(props["event"]["request"]["url"], "https://app.example.com/checkout");
    assert_eq!(props["event"]["tags"], json!([{"key": "browser", "value": "Chrome"}, {"key": "server_name", "value": "web-1"}]));
    assert_eq!(props["event"]["breadcrumbs"].as_array().unwrap().len(), 2);
    assert_eq!(props["event"]["breadcrumbs"][0]["category"], "ui.click");
    assert_eq!(props["events"].as_array().unwrap().len(), 2);
    assert_eq!(props["exceptions"][0]["type"], "TypeError");
    assert_eq!(props["exceptions"][0]["frames"][0]["resolved"], false);
    w.db.cleanup().await;
}

#[tokio::test]
async fn a_specific_event_can_be_selected() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    w.event_payload(&p, js_event(&hex_id(1), 1_700_000_000.0)).await;
    w.event_payload(&p, js_event(&hex_id(2), 1_700_000_100.0)).await;
    let iid = first_issue_id(&w, p.id).await;
    let r = w.get(&format!("/projects/{}/issues/{iid}?event={}", p.id, hex_id(1)), Some(&cookie)).await;
    assert_eq!(r.props()["event"]["event_id"], hex_id(1));
    let r = w.get(&format!("/projects/{}/issues/{iid}?event=unknown", p.id), Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    w.db.cleanup().await;
}

#[tokio::test]
async fn stack_trace_is_resolved_with_uploaded_source_map() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    error500::sourcemaps::store_sourcemap(&w.db.pool, &w.store, p.id, Some("1.0"), "~/static/app.min.js.map", Bytes::from(MAP)).await.unwrap();
    w.event_payload(&p, js_event(&hex_id(1), 1_700_000_000.0)).await;
    let iid = first_issue_id(&w, p.id).await;

    let r = w.get(&format!("/projects/{}/issues/{iid}", p.id), Some(&cookie)).await;
    let frame = &r.props()["exceptions"][0]["frames"][0];
    assert_eq!(frame["resolved"], true);
    assert_eq!(frame["filename"], "src/app.ts");
    assert_eq!(frame["lineno"], 3);
    assert_eq!(frame["context_line"], "function handleClick() {");
    assert_eq!(frame["minified"]["lineno"], 1);
    w.db.cleanup().await;
}

#[tokio::test]
async fn tags_given_as_pairs_and_array_breadcrumbs_are_normalized() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    let mut ev = js_event(&hex_id(1), 1_700_000_000.0);
    ev["tags"] = json!([["a", "1"], ["b", "2"]]);
    ev["breadcrumbs"] = json!([{"message": "only", "category": "x"}]);
    w.event_payload(&p, ev).await;
    let iid = first_issue_id(&w, p.id).await;
    let r = w.get(&format!("/projects/{}/issues/{iid}", p.id), Some(&cookie)).await;
    assert_eq!(r.props()["event"]["tags"], json!([{"key": "a", "value": "1"}, {"key": "b", "value": "2"}]));
    assert_eq!(r.props()["event"]["breadcrumbs"][0]["message"], "only");
    w.db.cleanup().await;
}

#[tokio::test]
async fn linked_replay_is_exposed_only_when_it_exists() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    let rid = "ab".repeat(16);
    let mut ev = js_event(&hex_id(1), 1_700_000_000.0);
    ev["contexts"] = json!({"replay": {"replay_id": rid}});
    w.event_payload(&p, ev).await;
    let iid = first_issue_id(&w, p.id).await;
    let url = format!("/projects/{}/issues/{iid}", p.id);
    assert!(w.get(&url, Some(&cookie)).await.props()["replay_id"].is_null());

    sqlx::query("INSERT INTO replays (project_id, replay_id, first_seen, last_seen) VALUES (?, ?, NOW(3), NOW(3))")
        .bind(p.id).bind(&rid).execute(&w.db.pool).await.unwrap();
    assert_eq!(w.get(&url, Some(&cookie)).await.props()["replay_id"], rid);
    w.db.cleanup().await;
}

#[tokio::test]
async fn issue_of_another_project_or_foreign_user_is_not_found() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    let (bob, bob_cookie) = w.login_as("bob@example.com").await;
    let other = w.project_for(bob, "Bob").await;
    w.event_payload(&p, js_event(&hex_id(1), 1_700_000_000.0)).await;
    let iid = first_issue_id(&w, p.id).await;
    // bob ne voit pas le projet d'alice
    assert_eq!(w.get(&format!("/projects/{}/issues/{iid}", p.id), Some(&bob_cookie)).await.status, StatusCode::NOT_FOUND);
    // l'issue n'est pas accessible via un autre projet (même pour son propriétaire)
    assert_eq!(w.get(&format!("/projects/{}/issues/{iid}", other.id), Some(&bob_cookie)).await.status, StatusCode::NOT_FOUND);
    assert_eq!(w.get(&format!("/projects/{}/issues/{iid}", p.id), Some(&cookie)).await.status, StatusCode::OK);
    w.db.cleanup().await;
}

#[tokio::test]
async fn status_can_be_changed_by_a_member() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    w.event_payload(&p, js_event(&hex_id(1), 1_700_000_000.0)).await;
    let iid = first_issue_id(&w, p.id).await;
    let url = format!("/projects/{}/issues/{iid}/status", p.id);

    let r = w.post_json(&url, Some(&cookie), json!({"status": "resolved"})).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.location(), Some(format!("/projects/{}/issues/{iid}", p.id)));
    let issue = error500::queries::get_issue(&w.db.pool, p.id, iid).await.unwrap().unwrap();
    assert_eq!(issue.status, error500::queries::IssueStatus::Resolved);

    let r = w.post_json(&url, Some(&cookie), json!({"status": "bogus"})).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY);
    w.db.cleanup().await;
}

#[tokio::test]
async fn status_change_is_refused_to_non_members() {
    let w = web_app().await;
    let (p, _) = setup(&w).await;
    let (_, bob_cookie) = w.login_as("bob@example.com").await;
    w.event_payload(&p, js_event(&hex_id(1), 1_700_000_000.0)).await;
    let iid = first_issue_id(&w, p.id).await;
    let r = w.post_json(&format!("/projects/{}/issues/{iid}/status", p.id), Some(&bob_cookie), json!({"status": "resolved"})).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let issue = error500::queries::get_issue(&w.db.pool, p.id, iid).await.unwrap().unwrap();
    assert_eq!(issue.status, error500::queries::IssueStatus::Unresolved);
    w.db.cleanup().await;
}
