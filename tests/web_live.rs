//! Mode live : indicateur « en direct » et flux SSE des segments de replay au fur et à mesure de leur arrivée.

mod common;
mod web_common;

use axum::body::BodyDataStream;
use axum::http::StatusCode;
use error500::projects::Project;
use futures::StreamExt;
use serde_json::{json, Value};
use std::time::Duration;
use web_common::{web_app, W};

const RID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

async fn setup(w: &W) -> (Project, String) {
    w.login_as("admin@example.com").await;
    let (alice, cookie) = w.login_as("alice@example.com").await;
    (w.project_for(alice, "App").await, cookie)
}

/// Segment reçu il y a `age_secs` secondes, contenant `events`.
async fn add_segment(w: &W, p: &Project, rid: &str, segment_id: u32, events: Value, age_secs: u32) {
    sqlx::query("INSERT IGNORE INTO replays (project_id, replay_id, first_seen, last_seen) VALUES (?, ?, UTC_TIMESTAMP(3), UTC_TIMESTAMP(3))")
        .bind(p.id)
        .bind(rid)
        .execute(&w.db.pool)
        .await
        .unwrap();
    let key = format!("replays/{}/{rid}/{segment_id}", p.id);
    w.store.put(&key, bytes::Bytes::from(events.to_string())).await.unwrap();
    sqlx::query(
        "INSERT INTO replay_segments (project_id, replay_id, segment_id, blob_key, size_bytes, received_at) \
         VALUES (?, ?, ?, ?, 10, CURRENT_TIMESTAMP(3) - INTERVAL ? SECOND)",
    )
    .bind(p.id)
    .bind(rid)
    .bind(segment_id)
    .bind(&key)
    .bind(age_secs)
    .execute(&w.db.pool)
    .await
    .unwrap();
}

fn stream_url(p: &Project, rid: &str) -> String {
    format!("/projects/{}/replays/{rid}/stream", p.id)
}

/// Lit le flux jusqu'à ce que `done` soit vrai du texte reçu (ou échoue au bout de 5 s).
async fn read_until(stream: &mut BodyDataStream, done: impl Fn(&str) -> bool) -> String {
    let mut text = String::new();
    let read = async {
        while !done(&text) {
            let chunk = stream.next().await.expect("le flux s'est terminé").expect("erreur de flux");
            text.push_str(&String::from_utf8_lossy(&chunk));
        }
    };
    tokio::time::timeout(Duration::from_secs(5), read).await.unwrap_or_else(|_| panic!("délai dépassé, reçu : {text:?}"));
    text
}

/// Trames SSE `(événement, données JSON)` d'un texte reçu.
fn frames(text: &str) -> Vec<(String, Value)> {
    text.split("\n\n")
        .filter_map(|f| {
            let event = f.lines().find_map(|l| l.strip_prefix("event:"))?.trim().to_string();
            let data = f.lines().find_map(|l| l.strip_prefix("data:"))?.trim();
            Some((event, serde_json::from_str(data).ok()?))
        })
        .collect()
}

fn segment_ids(text: &str) -> Vec<u64> {
    frames(text).into_iter().filter(|(e, _)| e == "segment").map(|(_, d)| d["segment_id"].as_u64().unwrap()).collect()
}

// --- Indicateur « en direct » ---

#[tokio::test]
async fn a_replay_with_a_recent_segment_is_live_and_an_old_one_is_not() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    let old = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    add_segment(&w, &p, RID, 0, json!([]), 5).await;
    add_segment(&w, &p, old, 0, json!([]), 300).await;

    let r = w.get(&format!("/projects/{}/replays", p.id), Some(&cookie)).await;
    let list = r.props()["replays"].as_array().unwrap().clone();
    let live_of = |id: &str| list.iter().find(|x| x["replay_id"] == id).unwrap()["live"].clone();
    assert_eq!(live_of(RID), true);
    assert_eq!(live_of(old), false);

    let r = w.get(&format!("/projects/{}/replays/{RID}", p.id), Some(&cookie)).await;
    assert_eq!(r.props()["replay"]["live"], true);
    assert_eq!(r.props()["stream_url"], stream_url(&p, RID));
    w.db.cleanup().await;
}

// --- Flux SSE ---

#[tokio::test]
async fn stream_sends_status_then_every_stored_segment_in_order() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    add_segment(&w, &p, RID, 1, json!([{"type": 3, "timestamp": 2}]), 5).await;
    add_segment(&w, &p, RID, 0, json!([{"type": 4, "timestamp": 1}, {"type": 2, "timestamp": 1}]), 6).await;

    let (status, headers, mut body) = w.open_stream(&stream_url(&p, RID), Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers["content-type"].to_str().unwrap().starts_with("text/event-stream"));
    let text = read_until(&mut body, |t| segment_ids(t).len() == 2).await;
    let f = frames(&text);
    assert_eq!(f[0].0, "status");
    assert_eq!(f[0].1["live"], true);
    assert_eq!(segment_ids(&text), vec![0, 1]);
    let seg0 = f.iter().find(|(e, d)| e == "segment" && d["segment_id"] == 0).unwrap();
    assert_eq!(seg0.1["events"], json!([{"type": 4, "timestamp": 1}, {"type": 2, "timestamp": 1}]));
    drop(body);
    w.db.cleanup().await;
}

#[tokio::test]
async fn stream_skips_segments_the_client_already_has() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    for id in 0..3 {
        add_segment(&w, &p, RID, id, json!([{"type": 3, "timestamp": id}]), 5).await;
    }
    let (_, _, mut body) = w.open_stream(&format!("{}?after=1", stream_url(&p, RID)), Some(&cookie)).await;
    let text = read_until(&mut body, |t| !segment_ids(t).is_empty()).await;
    assert_eq!(segment_ids(&text), vec![2]);
    drop(body);
    w.db.cleanup().await;
}

#[tokio::test]
async fn stream_delivers_segments_that_arrive_while_it_is_open_exactly_once() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    add_segment(&w, &p, RID, 0, json!([{"type": 4, "timestamp": 1}]), 1).await;
    let (_, _, mut body) = w.open_stream(&stream_url(&p, RID), Some(&cookie)).await;
    let mut text = read_until(&mut body, |t| segment_ids(t) == vec![0]).await;

    add_segment(&w, &p, RID, 1, json!([{"type": 3, "timestamp": 2}]), 0).await;
    text.push_str(&read_until(&mut body, |t| t.contains("\"segment_id\":1")).await);
    // laisse passer quelques cycles d'interrogation : pas de doublon
    tokio::time::sleep(Duration::from_millis(300)).await;
    while let Ok(Some(Ok(chunk))) = tokio::time::timeout(Duration::from_millis(50), body.next()).await {
        text.push_str(&String::from_utf8_lossy(&chunk));
    }
    assert_eq!(segment_ids(&text), vec![0, 1]);
    drop(body);
    w.db.cleanup().await;
}

#[tokio::test]
async fn stream_reports_when_the_session_goes_quiet_and_resumes() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    add_segment(&w, &p, RID, 0, json!([]), 600).await;
    let (_, _, mut body) = w.open_stream(&stream_url(&p, RID), Some(&cookie)).await;
    let text = read_until(&mut body, |t| frames(t).iter().any(|(e, _)| e == "status")).await;
    assert_eq!(frames(&text)[0].1["live"], false, "dernier segment il y a 10 minutes");

    add_segment(&w, &p, RID, 1, json!([]), 0).await;
    let text = read_until(&mut body, |t| {
        frames(t).iter().any(|(e, d)| e == "status" && d["live"] == true) && segment_ids(t).contains(&1)
    })
    .await;
    assert!(segment_ids(&text).contains(&1));
    drop(body);
    w.db.cleanup().await;
}

#[tokio::test]
async fn an_unreadable_segment_does_not_stop_the_stream() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    add_segment(&w, &p, RID, 0, json!([]), 5).await;
    w.store.delete(&format!("replays/{}/{RID}/0", p.id)).await.unwrap();
    add_segment(&w, &p, RID, 1, json!([{"type": 3, "timestamp": 2}]), 5).await;
    let (_, _, mut body) = w.open_stream(&stream_url(&p, RID), Some(&cookie)).await;
    let text = read_until(&mut body, |t| !segment_ids(t).is_empty()).await;
    assert_eq!(segment_ids(&text), vec![1]);
    drop(body);
    w.db.cleanup().await;
}

#[tokio::test]
async fn stream_is_reserved_to_project_members() {
    let w = web_app().await;
    let (p, _) = setup(&w).await;
    add_segment(&w, &p, RID, 0, json!([]), 5).await;
    let (_, eve) = w.login_as("eve@example.com").await;
    assert_eq!(w.open_stream(&stream_url(&p, RID), Some(&eve)).await.0, StatusCode::NOT_FOUND);
    assert_ne!(w.open_stream(&stream_url(&p, RID), None).await.0, StatusCode::OK);
    w.db.cleanup().await;
}

#[tokio::test]
async fn stream_of_an_unknown_replay_is_not_found() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    let (status, ..) = w.open_stream(&stream_url(&p, &"c".repeat(32)), Some(&cookie)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    w.db.cleanup().await;
}
