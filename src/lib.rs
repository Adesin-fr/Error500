pub mod envelope;
pub mod auth;
pub mod users;
pub mod totp;
pub mod secretbox;
pub mod projects;
pub mod event;
pub mod storage;
pub mod ingest;
pub mod alerts;
pub mod retention;
pub mod http;
pub mod config;
pub mod sourcemaps;
pub mod artifact_bundle;
pub mod cli_upload;
pub mod stacktrace;
pub mod queries;
pub mod web;

use axum::Router;

/// Application complète : API d'ingestion Sentry et interface web, dans un seul routeur.
pub fn app(pool: sqlx::MySqlPool, store: storage::BlobStore, mailer: alerts::Mailer, config: &config::Config) -> Router {
    let ingest = http::router(http::AppState {
        pool: pool.clone(),
        store: store.clone(),
        max_body: config.max_body,
        mailer,
        public_url: config.public_url.clone(),
        alert_limit: config.alert_limit,
    });
    let web = web::router(web::WebState {
        pool,
        store,
        public_url: config.public_url.clone(),
        assets: std::sync::Arc::new(web::assets::Assets::embedded()),
        dev_url: config.vite_dev_url.clone(),
        secure_cookies: config.secure_cookies(),
        live_poll: std::time::Duration::from_secs(1),
        secret_key: config.secret_key.clone(),
    });
    ingest.merge(web)
}
