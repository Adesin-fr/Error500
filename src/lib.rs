pub mod envelope;
pub mod auth;
pub mod users;
pub mod projects;
pub mod event;
pub mod storage;
pub mod ingest;
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
pub fn app(pool: sqlx::MySqlPool, store: storage::BlobStore, config: &config::Config) -> Router {
    let ingest = http::router(http::AppState { pool: pool.clone(), store: store.clone(), max_body: config.max_body });
    let web = web::router(web::WebState {
        pool,
        store,
        public_url: config.public_url.clone(),
        assets: std::sync::Arc::new(web::assets::Assets::embedded()),
        dev_url: config.vite_dev_url.clone(),
        secure_cookies: config.secure_cookies(),
    });
    ingest.merge(web)
}
