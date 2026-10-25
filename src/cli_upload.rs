//! Routes de `sentry-cli sourcemaps upload` : `chunk-upload`, `artifactbundle/assemble`, releases.
//!
//! sentry-cli ne transmet ni l'id du projet ni son slug de façon fiable : le projet est celui du
//! jeton d'upload (`Authorization: Bearer`), le slug d'organisation de l'URL est ignoré.

use crate::artifact_bundle;
use crate::http::{ApiError, AppState};
use crate::projects;
use crate::sourcemaps::{self, SourceMapError};
use axum::body::Bytes;
use axum::extract::{Multipart, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use flate2::read::GzDecoder;
use serde::Deserialize;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::io::Read;

const MAX_CHUNK_SIZE: usize = 8 * 1024 * 1024;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/0/organizations/{org}/chunk-upload/", get(options).post(upload_chunks))
        .route("/api/0/organizations/{org}/artifactbundle/assemble/", post(assemble))
        .route("/api/0/organizations/{org}/releases/", post(create_release))
        .route("/api/0/organizations/{org}/releases/{version}/", put(finalize_release).post(finalize_release))
        .route("/api/0/projects/{org}/{project}/releases/", post(create_release_in_project))
        .route("/api/0/projects/{org}/{project}/releases/{version}/", put(finalize_release_in_project))
}

async fn authorize(state: &AppState, headers: &HeaderMap) -> Result<u64, ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "jeton d'upload manquant"))?;
    projects::project_id_by_upload_token(&state.pool, token)
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::FORBIDDEN, "jeton d'upload invalide"))
}

fn chunk_size(max_body: usize) -> usize {
    (max_body / 2).clamp(1, MAX_CHUNK_SIZE)
}

fn chunk_key(project_id: u64, sha1: &str) -> String {
    format!("chunks/{project_id}/{sha1}")
}

fn sha1_hex(data: &[u8]) -> String {
    hex::encode(Sha1::digest(data))
}

fn is_sha1(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

async fn options(
    State(state): State<AppState>,
    Path(org): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    authorize(&state, &headers).await?;
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("localhost");
    let scheme = headers.get("x-forwarded-proto").and_then(|v| v.to_str().ok()).unwrap_or("http");
    let size = chunk_size(state.max_body);
    Ok(Json(json!({
        "url": format!("{scheme}://{host}/api/0/organizations/{org}/chunk-upload/"),
        "chunkSize": size,
        "chunksPerRequest": (state.max_body / size).max(1),
        "maxRequestSize": state.max_body,
        "maxFileSize": 2u64 * 1024 * 1024 * 1024,
        "concurrency": 4,
        "hashAlgorithm": "sha1",
        "compression": ["gzip"],
        "accept": ["artifact_bundles"],
    })))
}

/// Reçoit des chunks (`file` brut ou `file_gzip`, nommés par le sha1 de leur contenu décompressé).
async fn upload_chunks(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<StatusCode, ApiError> {
    let project_id = authorize(&state, &headers).await?;
    let bad = |d: &str| ApiError::new(StatusCode::BAD_REQUEST, d.to_string());
    while let Some(field) = multipart.next_field().await.map_err(|_| bad("multipart invalide"))? {
        let gzipped = match field.name() {
            Some("file") => false,
            Some("file_gzip") => true,
            _ => continue,
        };
        let name = field.file_name().unwrap_or("").to_string();
        let data = field.bytes().await.map_err(|_| bad("chunk illisible"))?;
        let data = if gzipped {
            let mut out = Vec::new();
            GzDecoder::new(&data[..]).take(state.max_body as u64 + 1).read_to_end(&mut out).map_err(|_| bad("chunk gzip invalide"))?;
            if out.len() > state.max_body {
                return Err(ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "chunk trop gros"));
            }
            Bytes::from(out)
        } else {
            data
        };
        if !is_sha1(&name) || sha1_hex(&data) != name.to_ascii_lowercase() {
            return Err(bad("le contenu du chunk ne correspond pas à son sha1"));
        }
        state.store.put(&chunk_key(project_id, &name.to_ascii_lowercase()), data).await.map_err(internal)?;
    }
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct AssembleRequest {
    checksum: String,
    #[serde(default)]
    chunks: Vec<String>,
    #[serde(default)]
    version: Option<String>,
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    tracing::error!("échec de l'upload sentry-cli : {e}");
    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "erreur interne")
}

fn assembled(state: &str, detail: Option<String>, missing: Vec<String>) -> Json<Value> {
    let mut v = json!({ "state": state, "missingChunks": missing });
    if let Some(d) = detail {
        v["detail"] = json!(d);
    }
    Json(v)
}

/// Assemble l'archive à partir des chunks déjà reçus, et enregistre les source maps qu'elle contient.
async fn assemble(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<AssembleRequest>,
) -> Result<Json<Value>, ApiError> {
    let project_id = authorize(&state, &headers).await?;
    let chunks: Vec<String> = req.chunks.iter().map(|c| c.to_ascii_lowercase()).collect();
    if chunks.iter().any(|c| !is_sha1(c)) || !is_sha1(&req.checksum) {
        return Ok(assembled("error", Some("sha1 invalide".into()), vec![]));
    }

    let mut missing = Vec::new();
    for c in &chunks {
        if !state.store.exists(&chunk_key(project_id, c)).await.map_err(internal)? {
            missing.push(c.clone());
        }
    }
    if !missing.is_empty() {
        return Ok(assembled("not_found", None, missing));
    }

    let mut zip = Vec::new();
    for c in &chunks {
        let part = state.store.get(&chunk_key(project_id, c)).await.map_err(internal)?;
        match part {
            Some(p) => zip.extend_from_slice(&p),
            None => return Ok(assembled("not_found", None, vec![c.clone()])),
        }
    }
    if sha1_hex(&zip) != req.checksum.to_ascii_lowercase() {
        return Ok(assembled("error", Some("somme de contrôle de l'archive incorrecte".into()), vec![]));
    }
    let bundle = match artifact_bundle::parse(&zip) {
        Ok(b) => b,
        Err(d) => return Ok(assembled("error", Some(d), vec![])),
    };

    let release = req.version.filter(|v| !v.is_empty()).or(bundle.release);
    for map in bundle.maps {
        let stored = sourcemaps::store_sourcemap_with_debug_id(
            &state.pool,
            &state.store,
            project_id,
            release.as_deref(),
            &map.url,
            Bytes::from(map.bytes),
            map.debug_id.as_deref(),
        )
        .await;
        match stored {
            Ok(()) => {}
            Err(SourceMapError::Invalid(d)) => return Ok(assembled("error", Some(format!("{} : {d}", map.url)), vec![])),
            Err(e) => return Err(internal(e)),
        }
    }
    Ok(assembled("ok", None, vec![]))
}

// Les releases n'ont pas d'existence propre ici : une release est le texte `release` des événements
// et des source maps. Ces routes répondent comme Sentry pour que sentry-cli poursuive.

async fn create_release(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    authorize(&state, &headers).await?;
    Ok((StatusCode::CREATED, Json(release_json(body.get("version").and_then(Value::as_str).unwrap_or("")))))
}

async fn create_release_in_project(
    state: State<AppState>,
    Path(_): Path<(String, String)>,
    headers: HeaderMap,
    body: Json<Value>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    create_release(state, headers, body).await
}

async fn finalize_release(
    State(state): State<AppState>,
    Path(params): Path<Vec<String>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    authorize(&state, &headers).await?;
    Ok(Json(release_json(params.last().map(String::as_str).unwrap_or(""))))
}

async fn finalize_release_in_project(
    State(state): State<AppState>,
    Path(params): Path<Vec<String>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    authorize(&state, &headers).await?;
    Ok(Json(release_json(params.last().map(String::as_str).unwrap_or(""))))
}

fn release_json(version: &str) -> Value {
    json!({ "version": version, "shortVersion": version, "projects": [] })
}
