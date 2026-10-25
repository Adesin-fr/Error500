//! Source maps : résolution d'un frame minifié vers le code d'origine, et stockage par projet.

use crate::storage::BlobStore;
use bytes::Bytes;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::MySqlPool;
use std::collections::HashMap;
use std::sync::Arc;
use url::Url;

const CONTEXT_LINES: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFrame {
    pub filename: String,
    pub lineno: u32,
    pub colno: u32,
    pub function: Option<String>,
    pub pre_context: Vec<String>,
    pub context_line: Option<String>,
    pub post_context: Vec<String>,
}

/// Résout une position minifiée (`lineno`/`colno` 1-indexés, comme dans un frame Sentry).
pub fn resolve_frame(map: &sourcemap::SourceMap, lineno: u32, colno: u32) -> Option<ResolvedFrame> {
    let line = lineno.checked_sub(1)?;
    let token = map.lookup_token(line, colno.saturating_sub(1))?;
    // `lookup_token` renvoie le dernier token précédent, y compris sur une autre ligne générée.
    if token.get_dst_line() != line {
        return None;
    }
    let filename = token.get_source()?.to_string();
    let src_line = token.get_src_line() as usize;

    let (mut pre_context, mut context_line, mut post_context) = (Vec::new(), None, Vec::new());
    if let Some(content) = map.get_source_contents(token.get_src_id()) {
        let lines: Vec<&str> = content.lines().collect();
        if let Some(line) = lines.get(src_line) {
            context_line = Some(line.to_string());
            pre_context = lines[src_line.saturating_sub(CONTEXT_LINES)..src_line]
                .iter()
                .map(|l| l.to_string())
                .collect();
            let end = (src_line + 1 + CONTEXT_LINES).min(lines.len());
            post_context = lines[src_line + 1..end].iter().map(|l| l.to_string()).collect();
        }
    }
    Some(ResolvedFrame {
        filename,
        lineno: token.get_src_line() + 1,
        colno: token.get_src_col() + 1,
        function: token.get_name().map(str::to_string),
        pre_context,
        context_line,
        post_context,
    })
}

#[derive(Debug, thiserror::Error)]
pub enum SourceMapError {
    #[error("source map invalide : {0}")]
    Invalid(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Storage(#[from] object_store::Error),
}

#[derive(Debug, Clone)]
pub struct SourceMapRow {
    pub id: u64,
    pub release: Option<String>,
    pub name: String,
    pub debug_id: Option<String>,
    pub size_bytes: u64,
}

/// Cartes chargées pour un événement, indexées par le `abs_path` des frames.
#[derive(Default, Clone)]
pub struct MapSet(HashMap<String, Arc<sourcemap::SourceMap>>);

impl MapSet {
    pub fn get(&self, abs_path: &str) -> Option<&sourcemap::SourceMap> {
        self.0.get(abs_path).map(Arc::as_ref)
    }

    pub fn insert(&mut self, abs_path: &str, map: sourcemap::SourceMap) {
        self.0.insert(abs_path.to_string(), Arc::new(map));
    }
}

fn parse_map(bytes: &[u8]) -> Result<sourcemap::SourceMap, String> {
    match sourcemap::decode_slice(bytes).map_err(|e| e.to_string())? {
        sourcemap::DecodedMap::Regular(m) => Ok(m),
        sourcemap::DecodedMap::Index(i) => i.flatten().map_err(|e| e.to_string()),
        _ => Err("format de source map non supporté".into()),
    }
}

fn name_hash(name: &str) -> String {
    hex::encode(Sha256::digest(name.as_bytes()))[..40].to_string()
}

/// Enregistre (ou remplace) la source map `name` d'un projet, pour une release ou pour toutes (`None`).
pub async fn store_sourcemap(
    pool: &MySqlPool,
    store: &BlobStore,
    project_id: u64,
    release: Option<&str>,
    name: &str,
    bytes: Bytes,
) -> Result<(), SourceMapError> {
    store_sourcemap_with_debug_id(pool, store, project_id, release, name, bytes, None).await
}

/// Comme [`store_sourcemap`], avec un `debug_id` connu par ailleurs (en-tête du manifeste d'une
/// archive sentry-cli) qui prime sur celui que la map contient éventuellement.
pub async fn store_sourcemap_with_debug_id(
    pool: &MySqlPool,
    store: &BlobStore,
    project_id: u64,
    release: Option<&str>,
    name: &str,
    bytes: Bytes,
    debug_id: Option<&str>,
) -> Result<(), SourceMapError> {
    parse_map(&bytes).map_err(SourceMapError::Invalid)?;
    let release_key = release.unwrap_or("");
    let hash = name_hash(name);
    let blob_key = format!("sourcemaps/{project_id}/{}", hex::encode(Sha256::digest([release_key.as_bytes(), b"\0", name.as_bytes()].concat())));
    let size = bytes.len() as u64;
    let debug_id = debug_id.map(str::to_string).or_else(|| debug_id_of(&bytes));
    store.put(&blob_key, bytes).await?;
    sqlx::query(
        "INSERT INTO sourcemaps (project_id, release_key, name, name_hash, debug_id, blob_key, size_bytes) \
         VALUES (?, ?, ?, ?, ?, ?, ?) \
         ON DUPLICATE KEY UPDATE debug_id = VALUES(debug_id), blob_key = VALUES(blob_key), \
           size_bytes = VALUES(size_bytes), created_at = CURRENT_TIMESTAMP",
    )
    .bind(project_id)
    .bind(release_key)
    .bind(name)
    .bind(hash)
    .bind(debug_id)
    .bind(blob_key)
    .bind(size)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list(pool: &MySqlPool, project_id: u64) -> Result<Vec<SourceMapRow>, sqlx::Error> {
    let rows: Vec<(u64, String, String, Option<String>, u64)> = sqlx::query_as(
        "SELECT id, release_key, name, debug_id, size_bytes FROM sourcemaps WHERE project_id = ? \
         ORDER BY release_key DESC, name",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, release, name, debug_id, size_bytes)| SourceMapRow {
            id,
            release: Some(release).filter(|r| !r.is_empty()),
            name,
            debug_id,
            size_bytes,
        })
        .collect())
}

pub async fn delete(pool: &MySqlPool, store: &BlobStore, project_id: u64, id: u64) -> Result<(), SourceMapError> {
    let blob: Option<(String,)> = sqlx::query_as("SELECT blob_key FROM sourcemaps WHERE id = ? AND project_id = ?")
        .bind(id)
        .bind(project_id)
        .fetch_optional(pool)
        .await?;
    if let Some((key,)) = blob {
        sqlx::query("DELETE FROM sourcemaps WHERE id = ? AND project_id = ?")
            .bind(id)
            .bind(project_id)
            .execute(pool)
            .await?;
        store.delete(&key).await?;
    }
    Ok(())
}

async fn load_blob(store: &BlobStore, row: Option<(String,)>) -> Option<sourcemap::SourceMap> {
    let (key,) = row?;
    let bytes = store.get(&key).await.ok()??;
    parse_map(&bytes).ok()
}

/// `abs_path` des frames d'un événement, dans l'ordre d'apparition, sans doublon.
fn frame_paths(payload: &Value) -> Vec<String> {
    let exc = payload.get("exception");
    let values = exc.and_then(|e| e.get("values").or(Some(e))).and_then(Value::as_array);
    let mut paths: Vec<String> = Vec::new();
    for ex in values.into_iter().flatten() {
        let frames = ex.get("stacktrace").and_then(|s| s.get("frames")).and_then(Value::as_array);
        for f in frames.into_iter().flatten() {
            if let Some(p) = f.get("abs_path").and_then(Value::as_str) {
                if !paths.iter().any(|x| x == p) {
                    paths.push(p.to_string());
                }
            }
        }
    }
    paths
}

/// Charge les source maps utiles à l'événement : par `debug_id` d'abord, puis par release + nom d'artefact.
pub async fn load_for_event(
    pool: &MySqlPool,
    store: &BlobStore,
    project_id: u64,
    payload: &Value,
) -> Result<MapSet, SourceMapError> {
    let release = payload.get("release").and_then(Value::as_str).unwrap_or("");
    let images: Vec<(&str, &str)> = payload
        .get("debug_meta")
        .and_then(|d| d.get("images"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|i| i.get("type").and_then(Value::as_str) == Some("sourcemap"))
        .filter_map(|i| Some((i.get("code_file")?.as_str()?, i.get("debug_id")?.as_str()?)))
        .collect();

    let mut set = MapSet::default();
    for path in frame_paths(payload) {
        let mut row: Option<(String,)> = None;
        if let Some((_, debug_id)) = images.iter().find(|(code_file, _)| *code_file == path) {
            row = sqlx::query_as("SELECT blob_key FROM sourcemaps WHERE project_id = ? AND debug_id = ? LIMIT 1")
                .bind(project_id)
                .bind(debug_id)
                .fetch_optional(pool)
                .await?;
        }
        if row.is_none() {
            let artifact = format!("{}.map", frame_path(&path));
            row = sqlx::query_as(
                "SELECT blob_key FROM sourcemaps WHERE project_id = ? AND name_hash = ? AND name = ? \
                 AND release_key IN (?, '') ORDER BY release_key DESC LIMIT 1",
            )
            .bind(project_id)
            .bind(name_hash(&artifact))
            .bind(&artifact)
            .bind(release)
            .fetch_optional(pool)
            .await?;
        }
        if let Some(map) = load_blob(store, row).await {
            set.insert(&path, map);
        }
    }
    Ok(set)
}

/// Nom d'artefact d'un fichier minifié : `~/chemin/du/fichier.js` (hôte, requête et fragment retirés).
pub fn frame_path(abs_path: &str) -> String {
    let path = match Url::parse(abs_path) {
        Ok(url) => url.path().to_string(),
        Err(_) => abs_path.split(['?', '#']).next().unwrap_or("").to_string(),
    };
    if path.starts_with("~/") {
        path
    } else {
        format!("~/{}", path.trim_start_matches('/'))
    }
}

/// `debugId` (ou `debug_id`) injecté dans la map par les plugins de bundler Sentry.
pub fn debug_id_of(map_json: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(map_json).ok()?;
    ["debugId", "debug_id"]
        .iter()
        .find_map(|k| v.get(k).and_then(Value::as_str))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une seule position : minifié (ligne 1, col 10) -> src/app.ts ligne 3, col 5, nom `handleClick`.
    pub(crate) const MAP: &str = r#"{
        "version": 3, "file": "app.min.js",
        "sources": ["src/app.ts"], "names": ["handleClick"],
        "sourcesContent": ["line1\nline2\nfunction handleClick() {\n  throw 1\n}\nline6"],
        "mappings": "SAEIA",
        "debugId": "5a3c8a2e-1f7d-4b9e-9d3a-0c1b2d3e4f50"
    }"#;

    fn map() -> sourcemap::SourceMap {
        sourcemap::SourceMap::from_slice(MAP.as_bytes()).unwrap()
    }

    #[test]
    fn resolves_position_to_original_source_with_context() {
        let r = resolve_frame(&map(), 1, 10).unwrap();
        assert_eq!(r.filename, "src/app.ts");
        assert_eq!(r.lineno, 3);
        assert_eq!(r.colno, 5);
        assert_eq!(r.function.as_deref(), Some("handleClick"));
        assert_eq!(r.context_line.as_deref(), Some("function handleClick() {"));
        assert_eq!(r.pre_context, vec!["line1", "line2"]);
        assert_eq!(r.post_context, vec!["  throw 1", "}", "line6"]);
    }

    #[test]
    fn unmapped_position_resolves_to_none() {
        assert!(resolve_frame(&map(), 50, 1).is_none());
    }

    #[test]
    fn context_is_limited_to_five_lines_each_side() {
        let lines: Vec<String> = (1..=30).map(|i| format!("l{i}")).collect();
        let json = serde_json::json!({
            "version": 3, "sources": ["a.ts"], "names": [],
            "sourcesContent": [lines.join("\n")],
            // ligne générée 0 -> source ligne 15 (index 14) : "AAcA" = col0, src0, ligne +14 ('c'=14*2=28), col0
            "mappings": "AAcA"
        });
        let m = sourcemap::SourceMap::from_slice(json.to_string().as_bytes()).unwrap();
        let r = resolve_frame(&m, 1, 1).unwrap();
        assert_eq!(r.lineno, 15);
        assert_eq!(r.pre_context.len(), 5);
        assert_eq!(r.pre_context[0], "l10");
        assert_eq!(r.post_context.len(), 5);
        assert_eq!(r.post_context[4], "l20");
    }

    #[test]
    fn frame_path_normalizes_urls() {
        assert_eq!(frame_path("https://app.example.com/static/js/app.min.js?v=3#x"), "~/static/js/app.min.js");
        assert_eq!(frame_path("app:///dist/main.js"), "~/dist/main.js");
        assert_eq!(frame_path("/var/www/bundle.js"), "~/var/www/bundle.js");
        assert_eq!(frame_path("~/already.js"), "~/already.js");
    }

    #[test]
    fn debug_id_is_read_from_the_map() {
        assert_eq!(debug_id_of(MAP.as_bytes()).as_deref(), Some("5a3c8a2e-1f7d-4b9e-9d3a-0c1b2d3e4f50"));
        assert_eq!(debug_id_of(br#"{"debug_id":"abc"}"#).as_deref(), Some("abc"));
        assert_eq!(debug_id_of(b"{}"), None);
        assert_eq!(debug_id_of(b"not json"), None);
    }
}
