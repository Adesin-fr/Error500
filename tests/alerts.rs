//! Alertes email : nouvelle issue et régression, envoyées aux membres du projet qui ne les ont pas coupées.

mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use common::TestDb;
use error500::alerts::{Mail, Mailer};
use error500::http::{router, AppState};
use error500::projects::{self, Project, Role};
use error500::storage::BlobStore;
use error500::users;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tower::ServiceExt;

struct App {
    db: TestDb,
    project: Project,
    mailer: Mailer,
    router: Router,
}

async fn app() -> App {
    app_with_limit(0).await
}

/// `limit` : alertes par projet et par heure (0 = pas de limite).
async fn app_with_limit(limit: u32) -> App {
    let db = TestDb::new().await;
    let owner = users::create_user(&db.pool, "owner@example.com", "Owner", "correct horse").await.unwrap();
    let project = projects::create_project(&db.pool, owner.id, "Mon App").await.unwrap();
    let mailer = Mailer::memory();
    let router = router(AppState {
        pool: db.pool.clone(),
        store: BlobStore::memory(),
        max_body: 1024 * 1024,
        mailer: mailer.clone(),
        public_url: "https://errors.example.com".into(),
        alert_limit: limit,
    });
    App { db, project, mailer, router }
}

async fn send_error(a: &App, event_id: &str, message: &str) {
    let ev = json!({"event_id": event_id, "level": "error", "timestamp": 1700000000,
                    "exception": {"values": [{"type": "TypeError", "value": message}]}})
    .to_string();
    let body = format!("{{\"event_id\":\"{event_id}\"}}\n{{\"type\":\"event\",\"length\":{}}}\n{ev}\n", ev.len());
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/{}/envelope/", a.project.id))
        .header("X-Sentry-Auth", format!("Sentry sentry_version=7, sentry_key={}", a.project.public_key))
        .body(Body::from(body))
        .unwrap();
    let resp = a.router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

const E1: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const E2: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const E3: &str = "cccccccccccccccccccccccccccccccc";

#[tokio::test]
async fn new_issue_emails_the_members_with_a_link() {
    let a = app().await;
    send_error(&a, E1, "x is undefined").await;
    let outbox = a.mailer.outbox();
    assert_eq!(outbox.len(), 1);
    let mail = &outbox[0];
    assert_eq!(mail.to, "owner@example.com");
    assert!(mail.subject.contains("Mon App"), "{}", mail.subject);
    assert!(mail.subject.contains("TypeError: x is undefined"), "{}", mail.subject);
    let issue_id: u64 = sqlx::query_scalar("SELECT id FROM issues").fetch_one(&a.db.pool).await.unwrap();
    assert!(
        mail.body.contains(&format!("https://errors.example.com/projects/{}/issues/{issue_id}", a.project.id)),
        "{}",
        mail.body
    );
    a.db.cleanup().await;
}

#[tokio::test]
async fn further_occurrences_do_not_email_again() {
    let a = app().await;
    send_error(&a, E1, "boom").await;
    send_error(&a, E2, "boom").await;
    assert_eq!(a.mailer.outbox().len(), 1);
    a.db.cleanup().await;
}

#[tokio::test]
async fn regression_of_a_resolved_issue_emails_again() {
    let a = app().await;
    send_error(&a, E1, "boom").await;
    sqlx::query("UPDATE issues SET status = 'resolved'").execute(&a.db.pool).await.unwrap();
    send_error(&a, E2, "boom").await;
    let outbox = a.mailer.outbox();
    assert_eq!(outbox.len(), 2);
    assert!(outbox[1].subject.to_lowercase().contains("regress"), "{}", outbox[1].subject);
    a.db.cleanup().await;
}

#[tokio::test]
async fn only_members_who_kept_notifications_on_are_mailed() {
    let a = app().await;
    let off = users::create_user(&a.db.pool, "off@example.com", "Off", "correct horse").await.unwrap();
    let on = users::create_user(&a.db.pool, "on@example.com", "On", "correct horse").await.unwrap();
    let outsider = users::create_user(&a.db.pool, "out@example.com", "Out", "correct horse").await.unwrap();
    projects::add_member(&a.db.pool, a.project.id, off.id, Role::Member).await.unwrap();
    projects::add_member(&a.db.pool, a.project.id, on.id, Role::Member).await.unwrap();
    projects::set_notifications(&a.db.pool, a.project.id, off.id, false).await.unwrap();
    let _ = outsider;

    send_error(&a, E1, "boom").await;
    let mut to: Vec<String> = a.mailer.outbox().into_iter().map(|m| m.to).collect();
    to.sort();
    assert_eq!(to, vec!["on@example.com", "owner@example.com"]);
    a.db.cleanup().await;
}

#[tokio::test]
async fn notifications_can_be_read_back_per_member() {
    let a = app().await;
    let owner = users::find_by_email(&a.db.pool, "owner@example.com").await.unwrap().unwrap();
    assert!(projects::list_members(&a.db.pool, a.project.id).await.unwrap()[0].notify);
    projects::set_notifications(&a.db.pool, a.project.id, owner.id, false).await.unwrap();
    assert!(!projects::list_members(&a.db.pool, a.project.id).await.unwrap()[0].notify);
    a.db.cleanup().await;
}

#[tokio::test]
async fn disabled_mailer_ignores_alerts_without_failing_ingestion() {
    let a = app().await;
    let router = router(AppState {
        pool: a.db.pool.clone(),
        store: BlobStore::memory(),
        max_body: 1024 * 1024,
        mailer: Mailer::disabled(),
        public_url: "http://x".into(),
        alert_limit: 0,
    });
    let a = App { router, ..a };
    send_error(&a, E3, "boom").await;
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events").fetch_one(&a.db.pool).await.unwrap();
    assert_eq!(n, 1);
    a.db.cleanup().await;
}

// --- Limitation du débit ---

const E4: &str = "dddddddddddddddddddddddddddddddd";

#[tokio::test]
async fn alerts_beyond_the_hourly_limit_are_suppressed() {
    let a = app_with_limit(2).await;
    for (id, msg) in [(E1, "un"), (E2, "deux"), (E3, "trois")] {
        send_error(&a, id, msg).await;
    }
    let subjects: Vec<String> = a.mailer.outbox().into_iter().map(|m| m.subject).collect();
    assert_eq!(subjects.len(), 2, "{subjects:?}");
    assert!(subjects[0].contains("un") && subjects[1].contains("deux"));
    a.db.cleanup().await;
}

#[tokio::test]
async fn limit_counts_alerts_not_recipients() {
    let a = app_with_limit(1).await;
    let bob = users::create_user(&a.db.pool, "bob@example.com", "Bob", "correct horse").await.unwrap();
    projects::add_member(&a.db.pool, a.project.id, bob.id, Role::Member).await.unwrap();
    send_error(&a, E1, "un").await;
    assert_eq!(a.mailer.outbox().len(), 2, "un envoi par membre");
    send_error(&a, E2, "deux").await;
    assert_eq!(a.mailer.outbox().len(), 2);
    a.db.cleanup().await;
}

#[tokio::test]
async fn a_new_window_resumes_alerts_and_reports_what_was_suppressed() {
    let a = app_with_limit(1).await;
    send_error(&a, E1, "un").await;
    send_error(&a, E2, "deux").await;
    send_error(&a, E3, "trois").await;
    assert_eq!(a.mailer.outbox().len(), 1);

    sqlx::query("UPDATE alert_throttle SET window_start = NOW() - INTERVAL 2 HOUR").execute(&a.db.pool).await.unwrap();
    send_error(&a, E4, "quatre").await;
    let outbox = a.mailer.outbox();
    assert_eq!(outbox.len(), 2);
    assert!(outbox[1].body.contains("2 alert"), "{}", outbox[1].body);
    assert!(!outbox[0].body.contains("suppressed"), "{}", outbox[0].body);

    // le compteur est remis à zéro une fois annoncé
    sqlx::query("UPDATE alert_throttle SET window_start = NOW() - INTERVAL 2 HOUR").execute(&a.db.pool).await.unwrap();
    send_error(&a, "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", "cinq").await;
    assert!(!a.mailer.outbox()[2].body.contains("suppressed"));
    a.db.cleanup().await;
}

#[tokio::test]
async fn the_limit_is_per_project() {
    let a = app_with_limit(1).await;
    let owner = users::find_by_email(&a.db.pool, "owner@example.com").await.unwrap().unwrap();
    let other = projects::create_project(&a.db.pool, owner.id, "Autre").await.unwrap();
    send_error(&a, E1, "un").await;
    send_error(&a, E2, "deux").await;
    assert_eq!(a.mailer.outbox().len(), 1);

    let ev = json!({"event_id": E3, "level": "error", "timestamp": 1700000000,
                    "exception": {"values": [{"type": "Error", "value": "ailleurs"}]}})
    .to_string();
    let body = format!("{{\"event_id\":\"{E3}\"}}\n{{\"type\":\"event\",\"length\":{}}}\n{ev}\n", ev.len());
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/{}/envelope/", other.id))
        .header("X-Sentry-Auth", format!("Sentry sentry_version=7, sentry_key={}", other.public_key))
        .body(Body::from(body))
        .unwrap();
    assert_eq!(a.router.clone().oneshot(req).await.unwrap().status(), StatusCode::OK);
    assert_eq!(a.mailer.outbox().len(), 2);
    a.db.cleanup().await;
}

/// Serveur SMTP minimal : accepte un message et renvoie tout ce qu'il a reçu.
async fn fake_smtp() -> (u16, tokio::task::JoinHandle<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = tokio::spawn(async move {
        let (sock, _) = listener.accept().await.unwrap();
        let (r, mut w) = sock.into_split();
        let mut r = BufReader::new(r);
        let mut received = String::new();
        w.write_all(b"220 fake ESMTP\r\n").await.unwrap();
        let mut in_data = false;
        loop {
            let mut line = String::new();
            if r.read_line(&mut line).await.unwrap() == 0 {
                break;
            }
            received.push_str(&line);
            if in_data {
                if line == ".\r\n" {
                    in_data = false;
                    w.write_all(b"250 queued\r\n").await.unwrap();
                }
                continue;
            }
            let up = line.to_ascii_uppercase();
            let reply: &[u8] = if up.starts_with("EHLO") {
                b"250-fake\r\n250 8BITMIME\r\n"
            } else if up.starts_with("DATA") {
                in_data = true;
                b"354 go\r\n"
            } else if up.starts_with("QUIT") {
                w.write_all(b"221 bye\r\n").await.unwrap();
                break;
            } else {
                b"250 ok\r\n"
            };
            w.write_all(reply).await.unwrap();
        }
        received
    });
    (port, handle)
}

#[tokio::test]
async fn smtp_mailer_delivers_the_message() {
    let (port, server) = fake_smtp().await;
    let mailer = Mailer::smtp(&format!("smtp://127.0.0.1:{port}"), "error500@example.com").unwrap();
    mailer
        .deliver(Mail { to: "dev@example.com".into(), subject: "[App] New issue".into(), body: "voir le lien".into() })
        .await
        .unwrap();
    let received = tokio::time::timeout(std::time::Duration::from_secs(5), server).await.unwrap().unwrap();
    assert!(received.contains("MAIL FROM:<error500@example.com>"), "{received}");
    assert!(received.contains("RCPT TO:<dev@example.com>"), "{received}");
    assert!(received.contains("Subject: [App] New issue"), "{received}");
    assert!(received.contains("voir le lien"), "{received}");
}

#[tokio::test]
async fn smtp_mailer_rejects_invalid_configuration() {
    assert!(Mailer::smtp("pas une url", "error500@example.com").is_err());
    assert!(Mailer::smtp("smtp://127.0.0.1:25", "pas une adresse").is_err());
}
