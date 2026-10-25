//! Routes HTTP d'ingestion compatibles SDK Sentry.

use crate::auth::{key_from_auth_header, key_from_dsn, key_from_query};
use crate::envelope::Envelope;
use crate::ingest::{self, IngestError};
use crate::projects::{self, Project};
use crate::sourcemaps::{self, SourceMapError};
use crate::storage::BlobStore;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, RawQuery, State};
use axum::http::{header, HeaderMap, HeaderName, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use flate2::read::{GzDecoder, ZlibDecoder};
use serde_json::{json, Value};
use sqlx::MySqlPool;
use std::io::Read;
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone)]
pub struct AppState {
    pub pool: MySqlPool,
    pub store: BlobStore,
    /// Taille maximale d'un corps de requête, après décompression.
    pub max_body: usize,
}

pub fn router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::POST, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            header::CONTENT_ENCODING,
            header::AUTHORIZATION,
            HeaderName::from_static("x-sentry-auth"),
            HeaderName::from_static("sentry-trace"),
            HeaderName::from_static("baggage"),
        ]);
    let max_body = state.max_body;
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/api/{project_id}/envelope/", post(envelope))
        .route("/api/{project_id}/envelope", post(envelope))
        .route("/api/{project_id}/store/", post(store))
        .route("/api/{project_id}/store", post(store))
        .route("/api/{project_id}/sourcemaps/", post(upload_sourcemap).put(upload_sourcemap))
        .route("/api/{project_id}/sourcemaps", post(upload_sourcemap).put(upload_sourcemap))
        .layer(DefaultBodyLimit::max(max_body))
        .layer(cors)
        .with_state(state)
}

#[derive(Debug)]
struct ApiError(StatusCode, String);

impl ApiError {
    fn new(status: StatusCode, detail: impl Into<String>) -> Self {
        ApiError(status, detail.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "detail": self.1 }))).into_response()
    }
}

impl From<IngestError> for ApiError {
    fn from(e: IngestError) -> Self {
        match e {
            IngestError::BadPayload(d) => ApiError::new(StatusCode::BAD_REQUEST, d),
            other => {
                tracing::error!("échec d'ingestion : {other}");
                ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "erreur interne")
            }
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!("erreur base de données : {e}");
        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "erreur interne")
    }
}

/// Décompresse le corps selon `Content-Encoding`, en bornant la taille résultante.
fn decode_body(headers: &HeaderMap, body: &[u8], max: usize) -> Result<Vec<u8>, ApiError> {
    let encoding = headers
        .get(header::CONTENT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_ascii_lowercase());
    let reader: Box<dyn Read + '_> = match encoding.as_deref() {
        Some("gzip") => Box::new(GzDecoder::new(body)),
        Some("deflate") => Box::new(ZlibDecoder::new(body)),
        None | Some("") | Some("identity") => return Ok(body.to_vec()),
        Some(other) => {
            return Err(ApiError::new(StatusCode::BAD_REQUEST, format!("Content-Encoding non supporté : {other}")))
        }
    };
    let mut out = Vec::new();
    reader
        .take(max as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, format!("corps compressé invalide : {e}")))?;
    if out.len() > max {
        return Err(ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "corps trop volumineux"));
    }
    Ok(out)
}

fn request_key(headers: &HeaderMap, query: Option<&str>) -> Option<String> {
    headers
        .get("x-sentry-auth")
        .and_then(|v| v.to_str().ok())
        .and_then(key_from_auth_header)
        .or_else(|| query.and_then(key_from_query))
}

async fn authorize(pool: &MySqlPool, project_id: u64, key: Option<String>) -> Result<Project, ApiError> {
    let key = key.ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "clé d'authentification manquante"))?;
    projects::authorize_ingest(pool, project_id, &key)
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::FORBIDDEN, "clé ou projet invalide"))
}

async fn envelope(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let raw = decode_body(&headers, &body, state.max_body)?;
    let envelope = Envelope::parse(&raw).map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e.to_string()))?;
    let key = request_key(&headers, query.as_deref()).or_else(|| envelope.dsn.as_deref().and_then(key_from_dsn));
    let project = authorize(&state.pool, project_id, key).await?;

    ingest::ingest_envelope(&state.pool, &state.store, &project, &envelope).await?;
    Ok(Json(json!({ "id": envelope.event_id.unwrap_or_default() })))
}

async fn store(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let project = authorize(&state.pool, project_id, request_key(&headers, query.as_deref())).await?;
    let raw = decode_body(&headers, &body, state.max_body)?;
    let mut payload: Value =
        serde_json::from_slice(&raw).map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e.to_string()))?;
    let obj = payload
        .as_object_mut()
        .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "l'événement doit être un objet JSON"))?;
    let id = match obj.get("event_id").and_then(Value::as_str) {
        Some(id) => id.to_string(),
        None => {
            let id = hex::encode(rand::random::<[u8; 16]>());
            obj.insert("event_id".into(), Value::String(id.clone()));
            id
        }
    };
    ingest::ingest_event_payload(&state.pool, &project, &payload).await?;
    Ok(Json(json!({ "id": id })))
}

/// Upload d'une source map : `POST /api/{id}/sourcemaps/?name=~/static/app.js.map&release=1.0`,
/// corps = fichier `.map`, `Authorization: Bearer <jeton d'upload du projet>`.
async fn upload_sourcemap(
    State(state): State<AppState>,
    Path(project_id): Path<u64>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "jeton d'upload manquant"))?;
    if !projects::authorize_upload(&state.pool, project_id, token).await? {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "jeton ou projet invalide"));
    }

    let params: Vec<(String, String)> =
        url::form_urlencoded::parse(query.as_deref().unwrap_or("").as_bytes()).into_owned().collect();
    let param = |k: &str| params.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str()).filter(|v| !v.is_empty());
    let name = param("name").ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "paramètre name requis"))?;

    match sourcemaps::store_sourcemap(&state.pool, &state.store, project_id, param("release"), name, body).await {
        Ok(()) => Ok(Json(json!({ "name": name }))),
        Err(SourceMapError::Invalid(d)) => Err(ApiError::new(StatusCode::BAD_REQUEST, d)),
        Err(e) => {
            tracing::error!("échec d'upload de source map : {e}");
            Err(ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "erreur interne"))
        }
    }
}
