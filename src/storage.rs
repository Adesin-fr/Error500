//! Stockage de blobs (replays rrweb, pièces jointes) : disque local, S3 ou mémoire.

use bytes::Bytes;
use object_store::aws::AmazonS3Builder;
use object_store::local::LocalFileSystem;
use object_store::memory::InMemory;
use object_store::path::Path;
use object_store::{ObjectStore, ObjectStoreExt, PutPayload};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageConfig {
    Local { path: PathBuf },
    S3 {
        bucket: String,
        region: String,
        endpoint: Option<String>,
        access_key: String,
        secret_key: String,
    },
}

impl StorageConfig {
    /// Lit la configuration depuis des variables (`STORAGE`, `STORAGE_PATH`, `S3_*`).
    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let required = |k: &str| var(k).ok_or_else(|| anyhow::anyhow!("variable {k} requise pour STORAGE=s3"));
        match var("STORAGE").as_deref().unwrap_or("local") {
            "local" => Ok(StorageConfig::Local {
                path: var("STORAGE_PATH").unwrap_or_else(|| "./data/blobs".into()).into(),
            }),
            "s3" => Ok(StorageConfig::S3 {
                bucket: required("S3_BUCKET")?,
                region: var("S3_REGION").unwrap_or_else(|| "us-east-1".into()),
                endpoint: var("S3_ENDPOINT"),
                access_key: required("S3_ACCESS_KEY")?,
                secret_key: required("S3_SECRET_KEY")?,
            }),
            other => anyhow::bail!("STORAGE inconnu : {other} (attendu : local ou s3)"),
        }
    }
}

#[derive(Clone)]
pub struct BlobStore {
    inner: Arc<dyn ObjectStore>,
}

impl BlobStore {
    pub fn memory() -> Self {
        BlobStore { inner: Arc::new(InMemory::new()) }
    }

    pub fn from_config(cfg: &StorageConfig) -> anyhow::Result<Self> {
        let inner: Arc<dyn ObjectStore> = match cfg {
            StorageConfig::Local { path } => {
                std::fs::create_dir_all(path)?;
                Arc::new(LocalFileSystem::new_with_prefix(path)?)
            }
            StorageConfig::S3 { bucket, region, endpoint, access_key, secret_key } => {
                let mut b = AmazonS3Builder::new()
                    .with_bucket_name(bucket)
                    .with_region(region)
                    .with_access_key_id(access_key)
                    .with_secret_access_key(secret_key);
                if let Some(ep) = endpoint {
                    b = b.with_endpoint(ep).with_allow_http(ep.starts_with("http://")).with_virtual_hosted_style_request(false);
                }
                Arc::new(b.build()?)
            }
        };
        Ok(BlobStore { inner })
    }

    pub async fn put(&self, key: &str, data: Bytes) -> object_store::Result<()> {
        self.inner.put(&Path::from(key), PutPayload::from_bytes(data)).await?;
        Ok(())
    }

    pub async fn get(&self, key: &str) -> object_store::Result<Option<Bytes>> {
        match self.inner.get(&Path::from(key)).await {
            Ok(r) => Ok(Some(r.bytes().await?)),
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn exists(&self, key: &str) -> object_store::Result<bool> {
        match self.inner.head(&Path::from(key)).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Supprime tous les blobs sous `prefix` (un dossier : `replays/12` ne touche pas `replays/120`).
    pub async fn delete_prefix(&self, prefix: &str) -> object_store::Result<()> {
        use futures::TryStreamExt;
        let metas: Vec<_> = self.inner.list(Some(&Path::from(prefix))).try_collect().await?;
        for meta in metas {
            self.inner.delete(&meta.location).await?;
        }
        Ok(())
    }

    pub async fn delete(&self, key: &str) -> object_store::Result<()> {
        match self.inner.delete(&Path::from(key)).await {
            Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    async fn roundtrip(store: &BlobStore) {
        assert_eq!(store.get("replays/1/abc/0").await.unwrap(), None);
        store.put("replays/1/abc/0", Bytes::from_static(b"hello")).await.unwrap();
        assert_eq!(store.get("replays/1/abc/0").await.unwrap(), Some(Bytes::from_static(b"hello")));
        store.put("replays/1/abc/0", Bytes::from_static(b"bye")).await.unwrap();
        assert_eq!(store.get("replays/1/abc/0").await.unwrap(), Some(Bytes::from_static(b"bye")));
        store.delete("replays/1/abc/0").await.unwrap();
        assert_eq!(store.get("replays/1/abc/0").await.unwrap(), None);
    }

    #[tokio::test]
    async fn memory_store_roundtrip() {
        roundtrip(&BlobStore::memory()).await;
    }

    #[tokio::test]
    async fn local_store_roundtrip_creates_nested_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::from_config(&StorageConfig::Local { path: dir.path().join("blobs") }).unwrap();
        roundtrip(&store).await;
    }

    #[tokio::test]
    async fn local_store_writes_under_the_configured_directory() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::from_config(&StorageConfig::Local { path: dir.path().to_path_buf() }).unwrap();
        store.put("a/b", Bytes::from_static(b"x")).await.unwrap();
        assert_eq!(std::fs::read(dir.path().join("a/b")).unwrap(), b"x");
    }

    #[test]
    fn s3_store_can_be_built_without_network() {
        let cfg = StorageConfig::S3 {
            bucket: "b".into(),
            region: "eu-west-3".into(),
            endpoint: Some("http://localhost:9000".into()),
            access_key: "k".into(),
            secret_key: "s".into(),
        };
        assert!(BlobStore::from_config(&cfg).is_ok());
    }

    fn vars(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn config_defaults_to_local_data_dir() {
        let cfg = StorageConfig::from_vars(vars(&[])).unwrap();
        assert_eq!(cfg, StorageConfig::Local { path: "./data/blobs".into() });
    }

    #[test]
    fn config_local_with_custom_path() {
        let cfg = StorageConfig::from_vars(vars(&[("STORAGE", "local"), ("STORAGE_PATH", "/srv/e500")])).unwrap();
        assert_eq!(cfg, StorageConfig::Local { path: "/srv/e500".into() });
    }

    #[test]
    fn config_s3_reads_all_fields() {
        let cfg = StorageConfig::from_vars(vars(&[
            ("STORAGE", "s3"),
            ("S3_BUCKET", "bk"),
            ("S3_REGION", "eu-west-1"),
            ("S3_ACCESS_KEY", "ak"),
            ("S3_SECRET_KEY", "sk"),
            ("S3_ENDPOINT", "http://minio:9000"),
        ]))
        .unwrap();
        assert_eq!(
            cfg,
            StorageConfig::S3 {
                bucket: "bk".into(),
                region: "eu-west-1".into(),
                endpoint: Some("http://minio:9000".into()),
                access_key: "ak".into(),
                secret_key: "sk".into(),
            }
        );
    }

    #[test]
    fn config_s3_requires_bucket_and_credentials() {
        assert!(StorageConfig::from_vars(vars(&[("STORAGE", "s3")])).is_err());
        assert!(StorageConfig::from_vars(vars(&[("STORAGE", "s3"), ("S3_BUCKET", "b")])).is_err());
    }

    #[test]
    fn config_rejects_unknown_driver() {
        assert!(StorageConfig::from_vars(vars(&[("STORAGE", "ftp")])).is_err());
    }
}
