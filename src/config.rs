//! Configuration de l'application, lue depuis l'environnement.

use crate::storage::StorageConfig;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind: String,
    /// URL publique, utilisée pour construire les DSN donnés aux SDK.
    pub public_url: String,
    pub max_body: usize,
    pub storage: StorageConfig,
    /// Serveur de développement Vite (`VITE_DEV_URL`) ; absent en production.
    pub vite_dev_url: Option<String>,
    /// Serveur SMTP des alertes (`SMTP_URL`, ex. `smtps://user:pass@host`) ; sans lui, pas d'email.
    pub smtp_url: Option<String>,
    pub mail_from: String,
    /// Filtre des journaux (`RUST_LOG`, syntaxe tracing) ; `info` par défaut.
    pub log_filter: String,
    /// Alertes email maximum par projet et par heure (`ALERT_MAX_PER_HOUR`, 0 = illimité).
    pub alert_limit: u32,
    /// Clé de chiffrement des secrets stockés (`SECRET_KEY`, 64 caractères hexadécimaux) ; sans elle, la 2FA est indisponible.
    pub secret_key: Option<crate::secretbox::SecretKey>,
}

/// Variables d'un fichier `.env` (`CLE=valeur`, commentaires `#`, guillemets optionnels).
pub fn parse_env_file(content: &str) -> HashMap<String, String> {
    dotenvy::from_read_iter(content.as_bytes()).filter_map(Result::ok).collect()
}

/// Lit un fichier `.env` ; un fichier absent donne un jeu vide.
pub fn read_env_file(path: &Path) -> anyhow::Result<HashMap<String, String>> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(parse_env_file(&content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(e) => Err(anyhow::anyhow!("lecture de {} : {e}", path.display())),
    }
}

/// L'environnement réel prime sur le fichier.
pub fn layered(env: impl Fn(&str) -> Option<String>, file: HashMap<String, String>) -> impl Fn(&str) -> Option<String> {
    move |k| env(k).or_else(|| file.get(k).cloned())
}

impl Config {
    /// Environnement, complété par le fichier `.env` du répertoire courant (ou `ENV_FILE`).
    pub fn from_env() -> anyhow::Result<Self> {
        let path = std::env::var("ENV_FILE").unwrap_or_else(|_| ".env".into());
        let file = read_env_file(Path::new(&path))?;
        Self::from_vars(layered(|k| std::env::var(k).ok(), file))
    }

    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let database_url = var("DATABASE_URL").ok_or_else(|| anyhow::anyhow!("DATABASE_URL requise"))?;
        let max_body_mb: usize = match var("MAX_BODY_MB") {
            Some(v) => v.parse().map_err(|_| anyhow::anyhow!("MAX_BODY_MB invalide : {v}"))?,
            None => 20,
        };
        Ok(Config {
            database_url,
            bind: var("BIND").unwrap_or_else(|| "0.0.0.0:8080".into()),
            public_url: var("PUBLIC_URL").unwrap_or_else(|| "http://localhost:8080".into()),
            max_body: max_body_mb * 1024 * 1024,
            storage: StorageConfig::from_vars(&var)?,
            vite_dev_url: var("VITE_DEV_URL"),
            smtp_url: var("SMTP_URL").filter(|v| !v.is_empty()),
            log_filter: var("RUST_LOG").unwrap_or_else(|| "info".into()),
            alert_limit: match var("ALERT_MAX_PER_HOUR") {
                Some(v) => v.parse().map_err(|_| anyhow::anyhow!("ALERT_MAX_PER_HOUR invalide : {v}"))?,
                None => 10,
            },
            secret_key: match var("SECRET_KEY").filter(|v| !v.trim().is_empty()) {
                Some(v) => Some(crate::secretbox::SecretKey::from_hex(&v).map_err(|e| anyhow::anyhow!("SECRET_KEY invalide : {e}"))?),
                None => None,
            },
            mail_from: var("MAIL_FROM").unwrap_or_else(|| "error500@localhost".into()),
        })
    }

    /// Transport d'alertes : SMTP si configuré, sinon désactivé.
    pub fn mailer(&self) -> anyhow::Result<crate::alerts::Mailer> {
        match &self.smtp_url {
            Some(url) => crate::alerts::Mailer::smtp(url, &self.mail_from),
            None => Ok(crate::alerts::Mailer::disabled()),
        }
    }

    pub fn secure_cookies(&self) -> bool {
        self.public_url.starts_with("https://")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn env_file_values_are_read_and_real_environment_wins() {
        let file = "# commentaire\nDATABASE_URL=mysql://file\nBIND=\"127.0.0.1:9000\"\nMAX_BODY_MB = 7\n\nPUBLIC_URL='https://err.example.com'\n";
        let c = Config::from_vars(layered(vars(&[("BIND", "0.0.0.0:1")]), parse_env_file(file))).unwrap();
        assert_eq!(c.database_url, "mysql://file");
        assert_eq!(c.bind, "0.0.0.0:1", "la variable d'environnement prime sur le fichier");
        assert_eq!(c.max_body, 7 * 1024 * 1024);
        assert_eq!(c.public_url, "https://err.example.com");
    }

    #[test]
    fn log_filter_defaults_to_info_and_follows_rust_log() {
        let c = Config::from_vars(vars(&[("DATABASE_URL", "mysql://x")])).unwrap();
        assert_eq!(c.log_filter, "info");
        let c = Config::from_vars(vars(&[("DATABASE_URL", "mysql://x"), ("RUST_LOG", "error500=debug,warn")])).unwrap();
        assert_eq!(c.log_filter, "error500=debug,warn");
    }

    #[test]
    fn secret_key_is_optional_but_must_be_valid() {
        let c = Config::from_vars(vars(&[("DATABASE_URL", "mysql://x")])).unwrap();
        assert!(c.secret_key.is_none());
        let c = Config::from_vars(vars(&[
            ("DATABASE_URL", "mysql://x"),
            ("SECRET_KEY", "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"),
        ]))
        .unwrap();
        assert!(c.secret_key.is_some());
        assert!(Config::from_vars(vars(&[("DATABASE_URL", "mysql://x"), ("SECRET_KEY", "trop-court")])).is_err());
    }

    #[test]
    fn missing_env_file_is_not_an_error() {
        assert!(read_env_file(std::path::Path::new("/nonexistent/.env")).unwrap().is_empty());
    }

    #[test]
    fn database_url_is_required() {
        assert!(Config::from_vars(vars(&[])).is_err());
    }

    #[test]
    fn defaults_are_applied() {
        let c = Config::from_vars(vars(&[("DATABASE_URL", "mysql://root@localhost/e500")])).unwrap();
        assert_eq!(c.database_url, "mysql://root@localhost/e500");
        assert_eq!(c.bind, "0.0.0.0:8080");
        assert_eq!(c.public_url, "http://localhost:8080");
        assert_eq!(c.max_body, 20 * 1024 * 1024);
        assert_eq!(c.storage, StorageConfig::Local { path: "./data/blobs".into() });
    }

    #[test]
    fn smtp_is_optional_and_from_has_a_default() {
        let c = Config::from_vars(vars(&[("DATABASE_URL", "mysql://x")])).unwrap();
        assert_eq!(c.smtp_url, None);
        assert_eq!(c.mail_from, "error500@localhost");
        let c = Config::from_vars(vars(&[
            ("DATABASE_URL", "mysql://x"),
            ("SMTP_URL", "smtps://u:p@smtp.example.com"),
            ("MAIL_FROM", "Error500 <errors@example.com>"),
        ]))
        .unwrap();
        assert_eq!(c.smtp_url.as_deref(), Some("smtps://u:p@smtp.example.com"));
        assert_eq!(c.mail_from, "Error500 <errors@example.com>");
    }

    #[test]
    fn alert_limit_defaults_to_ten_per_hour_and_is_configurable() {
        let c = Config::from_vars(vars(&[("DATABASE_URL", "mysql://x")])).unwrap();
        assert_eq!(c.alert_limit, 10);
        let c = Config::from_vars(vars(&[("DATABASE_URL", "mysql://x"), ("ALERT_MAX_PER_HOUR", "0")])).unwrap();
        assert_eq!(c.alert_limit, 0);
        assert!(Config::from_vars(vars(&[("DATABASE_URL", "mysql://x"), ("ALERT_MAX_PER_HOUR", "beaucoup")])).is_err());
    }

    #[test]
    fn overrides_are_read() {
        let c = Config::from_vars(vars(&[
            ("DATABASE_URL", "mysql://x"),
            ("BIND", "127.0.0.1:9000"),
            ("PUBLIC_URL", "https://err.example.com"),
            ("MAX_BODY_MB", "5"),
            ("STORAGE", "s3"),
            ("S3_BUCKET", "b"),
            ("S3_ACCESS_KEY", "a"),
            ("S3_SECRET_KEY", "s"),
        ]))
        .unwrap();
        assert_eq!(c.bind, "127.0.0.1:9000");
        assert_eq!(c.public_url, "https://err.example.com");
        assert_eq!(c.max_body, 5 * 1024 * 1024);
        assert!(matches!(c.storage, StorageConfig::S3 { .. }));
    }

    #[test]
    fn vite_dev_url_is_optional() {
        let c = Config::from_vars(vars(&[("DATABASE_URL", "mysql://x")])).unwrap();
        assert_eq!(c.vite_dev_url, None);
        let c = Config::from_vars(vars(&[("DATABASE_URL", "mysql://x"), ("VITE_DEV_URL", "http://localhost:5173")])).unwrap();
        assert_eq!(c.vite_dev_url.as_deref(), Some("http://localhost:5173"));
    }

    #[test]
    fn cookies_are_secure_only_behind_https() {
        let http = Config::from_vars(vars(&[("DATABASE_URL", "mysql://x")])).unwrap();
        assert!(!http.secure_cookies());
        let https = Config::from_vars(vars(&[("DATABASE_URL", "mysql://x"), ("PUBLIC_URL", "https://err.example.com")])).unwrap();
        assert!(https.secure_cookies());
    }

    #[test]
    fn invalid_max_body_is_rejected() {
        assert!(Config::from_vars(vars(&[("DATABASE_URL", "mysql://x"), ("MAX_BODY_MB", "lots")])).is_err());
    }
}
