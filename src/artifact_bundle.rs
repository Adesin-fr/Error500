//! Archive « artifact bundle » produite par `sentry-cli sourcemaps upload` : un zip avec
//! `manifest.json` décrivant les fichiers (type, URL `~/…`, en-têtes dont `debug-id`).

use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;

/// Taille maximale d'un fichier de l'archive une fois décompressé (protège des zip bombs).
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    release: Option<String>,
    files: HashMap<String, ManifestFile>,
}

#[derive(Debug, Deserialize)]
struct ManifestFile {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    headers: HashMap<String, String>,
}

/// Une source map extraite de l'archive.
#[derive(Debug, PartialEq, Eq)]
pub struct BundledMap {
    /// Nom d'artefact, ex. `~/static/app.min.js.map`.
    pub url: String,
    pub debug_id: Option<String>,
    pub bytes: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Bundle {
    pub release: Option<String>,
    pub maps: Vec<BundledMap>,
}

pub fn parse(zip_bytes: &[u8]) -> Result<Bundle, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).map_err(|e| format!("archive invalide : {e}"))?;
    let manifest: Manifest = {
        let entry = zip.by_name("manifest.json").map_err(|_| "manifest.json absent de l'archive".to_string())?;
        serde_json::from_reader(entry.take(MAX_FILE_BYTES)).map_err(|e| format!("manifest.json invalide : {e}"))?
    };
    let mut maps = Vec::new();
    for (path, file) in &manifest.files {
        if file.kind != "source_map" {
            continue;
        }
        let entry = zip.by_name(path).map_err(|_| format!("fichier {path} absent de l'archive"))?;
        let mut bytes = Vec::new();
        entry.take(MAX_FILE_BYTES).read_to_end(&mut bytes).map_err(|e| format!("lecture de {path} : {e}"))?;
        let debug_id = file.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("debug-id")).map(|(_, v)| v.clone()).filter(|v| !v.is_empty());
        maps.push(BundledMap { url: file.url.clone(), debug_id, bytes });
    }
    maps.sort_by(|a, b| a.url.cmp(&b.url));
    Ok(Bundle { release: manifest.release.filter(|r| !r.is_empty()), maps })
}
