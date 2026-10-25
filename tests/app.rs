mod common;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use common::TestDb;
use error500::config::Config;
use error500::storage::BlobStore;
use tower::ServiceExt;

#[tokio::test]
async fn one_router_serves_ingestion_and_the_web_ui() {
    let db = TestDb::new().await;
    let config = Config::from_vars(|k| match k {
        "DATABASE_URL" => Some("mysql://unused".into()),
        _ => None,
    })
    .unwrap();
    let app = error500::app(db.pool.clone(), BlobStore::memory(), error500::alerts::Mailer::disabled(), &config);

    let get = |uri: &str| Request::get(uri).body(Body::empty()).unwrap();
    assert_eq!(app.clone().oneshot(get("/health")).await.unwrap().status(), StatusCode::OK);
    // aucun utilisateur : l'interface renvoie vers le premier lancement
    let r = app.clone().oneshot(get("/login")).await.unwrap();
    assert_eq!(r.headers()[header::LOCATION], "/setup");
    // l'ingestion garde son CORS permissif pour les SDK navigateur
    let preflight = Request::builder()
        .method(Method::OPTIONS)
        .uri("/api/1/envelope/")
        .header(header::ORIGIN, "https://site.example.com")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .body(Body::empty())
        .unwrap();
    let r = app.clone().oneshot(preflight).await.unwrap();
    assert_eq!(r.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
    // et l'interface n'a pas ce CORS
    let r = app.oneshot(get("/setup")).await.unwrap();
    assert!(!r.headers().contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
    db.cleanup().await;
}
