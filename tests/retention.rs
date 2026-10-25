//! Rétention des logs et des replays, et suppression d'un projet.

mod common;

use bytes::Bytes;
use common::TestDb;
use error500::projects::{self, Project};
use error500::retention::{self, Retention};
use error500::storage::BlobStore;
use error500::users;

async fn setup() -> (TestDb, BlobStore, Project) {
    let db = TestDb::new().await;
    let u = users::create_user(&db.pool, "a@example.com", "A", "correct horse").await.unwrap();
    let p = projects::create_project(&db.pool, u.id, "App").await.unwrap();
    (db, BlobStore::memory(), p)
}

async fn count(db: &TestDb, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}"))).fetch_one(&db.pool).await.unwrap()
}

async fn add_log(db: &TestDb, project_id: u64, days_ago: u32) {
    sqlx::query("INSERT INTO logs (project_id, occurred_at, level, body) VALUES (?, UTC_TIMESTAMP(3) - INTERVAL ? DAY, 'info', 'x')")
        .bind(project_id)
        .bind(days_ago)
        .execute(&db.pool)
        .await
        .unwrap();
}

/// Un replay d'un segment dont la dernière activité remonte à `days_ago` jours.
async fn add_replay(db: &TestDb, store: &BlobStore, project_id: u64, replay_id: &str, days_ago: u32) -> String {
    sqlx::query(
        "INSERT INTO replays (project_id, replay_id, first_seen, last_seen) \
         VALUES (?, ?, UTC_TIMESTAMP(3) - INTERVAL ? DAY, UTC_TIMESTAMP(3) - INTERVAL ? DAY)",
    )
    .bind(project_id)
    .bind(replay_id)
    .bind(days_ago)
    .bind(days_ago)
    .execute(&db.pool)
    .await
    .unwrap();
    let key = format!("replays/{project_id}/{replay_id}/0");
    store.put(&key, Bytes::from_static(b"[]")).await.unwrap();
    sqlx::query("INSERT INTO replay_segments (project_id, replay_id, segment_id, blob_key, size_bytes) VALUES (?, ?, 0, ?, 2)")
        .bind(project_id)
        .bind(replay_id)
        .bind(&key)
        .execute(&db.pool)
        .await
        .unwrap();
    key
}

const R1: &str = "11111111111111111111111111111111";
const R2: &str = "22222222222222222222222222222222";

#[tokio::test]
async fn retention_defaults_to_30_days_and_can_be_changed() {
    let (db, _, p) = setup().await;
    assert_eq!(retention::get(&db.pool, p.id).await.unwrap(), Retention { logs_days: 30, replays_days: 30 });
    retention::set(&db.pool, p.id, Retention { logs_days: 7, replays_days: 90 }).await.unwrap();
    assert_eq!(retention::get(&db.pool, p.id).await.unwrap(), Retention { logs_days: 7, replays_days: 90 });
    db.cleanup().await;
}

#[test]
fn retention_is_validated() {
    assert!(Retention { logs_days: 0, replays_days: 3650 }.validate().is_ok());
    assert!(Retention { logs_days: 3651, replays_days: 1 }.validate().unwrap_err().contains_key("logs_days"));
    assert!(Retention { logs_days: 1, replays_days: 3651 }.validate().unwrap_err().contains_key("replays_days"));
}

#[tokio::test]
async fn purge_deletes_old_logs_only() {
    let (db, store, p) = setup().await;
    add_log(&db, p.id, 40).await;
    add_log(&db, p.id, 31).await;
    add_log(&db, p.id, 29).await;
    add_log(&db, p.id, 0).await;
    let stats = retention::purge(&db.pool, &store).await.unwrap();
    assert_eq!(stats.logs, 2);
    assert_eq!(count(&db, "logs").await, 2);
    db.cleanup().await;
}

#[tokio::test]
async fn purge_deletes_old_replays_with_their_segments_and_blobs() {
    let (db, store, p) = setup().await;
    let old_key = add_replay(&db, &store, p.id, R1, 45).await;
    let new_key = add_replay(&db, &store, p.id, R2, 2).await;
    let stats = retention::purge(&db.pool, &store).await.unwrap();
    assert_eq!(stats.replays, 1);
    assert_eq!(count(&db, "replays").await, 1);
    assert_eq!(count(&db, "replay_segments").await, 1);
    assert!(store.get(&old_key).await.unwrap().is_none());
    assert!(store.get(&new_key).await.unwrap().is_some());
    db.cleanup().await;
}

#[tokio::test]
async fn logs_and_replays_have_independent_delays_per_project() {
    let (db, store, p) = setup().await;
    let u = users::find_by_email(&db.pool, "a@example.com").await.unwrap().unwrap();
    let other = projects::create_project(&db.pool, u.id, "Autre").await.unwrap();
    retention::set(&db.pool, p.id, Retention { logs_days: 5, replays_days: 60 }).await.unwrap();
    for pid in [p.id, other.id] {
        add_log(&db, pid, 10).await;
    }
    add_replay(&db, &store, p.id, R1, 10).await;
    add_replay(&db, &store, other.id, R2, 10).await;
    retention::purge(&db.pool, &store).await.unwrap();
    // p : log de 10 j supprimé (délai 5), replay de 10 j gardé (délai 60) ; other : défaut 30 j, tout est gardé
    let logs: Vec<(u64,)> = sqlx::query_as("SELECT project_id FROM logs").fetch_all(&db.pool).await.unwrap();
    assert_eq!(logs, vec![(other.id,)]);
    assert_eq!(count(&db, "replays").await, 2);
    db.cleanup().await;
}

#[tokio::test]
async fn zero_days_keeps_everything() {
    let (db, store, p) = setup().await;
    retention::set(&db.pool, p.id, Retention { logs_days: 0, replays_days: 0 }).await.unwrap();
    add_log(&db, p.id, 3000).await;
    add_replay(&db, &store, p.id, R1, 3000).await;
    let stats = retention::purge(&db.pool, &store).await.unwrap();
    assert_eq!((stats.logs, stats.replays), (0, 0));
    assert_eq!((count(&db, "logs").await, count(&db, "replays").await), (1, 1));
    db.cleanup().await;
}

#[tokio::test]
async fn purge_handles_more_rows_than_one_batch() {
    let (db, store, p) = setup().await;
    for _ in 0..1203 {
        add_log(&db, p.id, 40).await;
    }
    let stats = retention::purge(&db.pool, &store).await.unwrap();
    assert_eq!(stats.logs, 1203);
    assert_eq!(count(&db, "logs").await, 0);
    db.cleanup().await;
}

#[tokio::test]
async fn deleting_a_project_removes_its_data_and_blobs_but_not_other_projects() {
    let (db, store, p) = setup().await;
    let u = users::find_by_email(&db.pool, "a@example.com").await.unwrap().unwrap();
    let other = projects::create_project(&db.pool, u.id, "Autre").await.unwrap();

    for pid in [p.id, other.id] {
        add_log(&db, pid, 0).await;
        sqlx::query(
            "INSERT INTO issues (project_id, fingerprint, title, level, first_seen, last_seen, event_count) \
             VALUES (?, ?, 't', 'error', NOW(), NOW(), 1)",
        )
        .bind(pid)
        .bind(format!("fp{pid}"))
        .execute(&db.pool)
        .await
        .unwrap();
        store.put(&format!("sourcemaps/{pid}/abc"), Bytes::from_static(b"m")).await.unwrap();
        store.put(&format!("chunks/{pid}/def"), Bytes::from_static(b"c")).await.unwrap();
    }
    let mine = add_replay(&db, &store, p.id, R1, 0).await;
    let theirs = add_replay(&db, &store, other.id, R2, 0).await;
    // un projet dont l'id commence par les mêmes chiffres ne doit pas être touché (préfixe `replays/1/` vs `replays/10/`)
    store.put(&format!("replays/{}0/x/0", p.id), Bytes::from_static(b"z")).await.unwrap();

    projects::delete(&db.pool, &store, p.id).await.unwrap();

    assert!(projects::get(&db.pool, p.id).await.unwrap().is_none());
    for table in ["logs", "issues", "replays", "replay_segments", "project_members"] {
        let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table} WHERE project_id = {}", p.id)))
            .fetch_one(&db.pool)
            .await
            .unwrap();
        assert_eq!(n, 0, "{table}");
    }
    assert!(store.get(&mine).await.unwrap().is_none());
    assert!(store.get(&format!("sourcemaps/{}/abc", p.id)).await.unwrap().is_none());
    assert!(store.get(&format!("chunks/{}/def", p.id)).await.unwrap().is_none());
    // l'autre projet est intact
    assert!(projects::get(&db.pool, other.id).await.unwrap().is_some());
    assert!(store.get(&theirs).await.unwrap().is_some());
    assert!(store.get(&format!("sourcemaps/{}/abc", other.id)).await.unwrap().is_some());
    assert!(store.get(&format!("replays/{}0/x/0", p.id)).await.unwrap().is_some());
    assert_eq!(count(&db, "logs").await, 1);
    db.cleanup().await;
}
