//! Rétention des logs et des replays : délai par projet, purge périodique.

use crate::storage::BlobStore;
use sqlx::MySqlPool;
use std::collections::BTreeMap;

/// Délai maximal configurable (10 ans).
pub const MAX_DAYS: u32 = 3650;
/// Lignes supprimées par requête, pour ne pas verrouiller la table longtemps.
const BATCH: u64 = 500;

/// Délais de conservation en jours ; 0 = conserver sans limite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retention {
    pub logs_days: u32,
    pub replays_days: u32,
}

impl Retention {
    /// Erreurs par champ, au format attendu par les formulaires.
    pub fn validate(&self) -> Result<(), BTreeMap<&'static str, String>> {
        let mut errors = BTreeMap::new();
        for (field, days) in [("logs_days", self.logs_days), ("replays_days", self.replays_days)] {
            if days > MAX_DAYS {
                errors.insert(field, format!("Au plus {MAX_DAYS} jours (0 = conserver sans limite)."));
            }
        }
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

pub async fn get(pool: &MySqlPool, project_id: u64) -> Result<Retention, sqlx::Error> {
    let (logs_days, replays_days): (u32, u32) =
        sqlx::query_as("SELECT logs_retention_days, replays_retention_days FROM projects WHERE id = ?")
            .bind(project_id)
            .fetch_one(pool)
            .await?;
    Ok(Retention { logs_days, replays_days })
}

pub async fn set(pool: &MySqlPool, project_id: u64, r: Retention) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE projects SET logs_retention_days = ?, replays_retention_days = ? WHERE id = ?")
        .bind(r.logs_days)
        .bind(r.replays_days)
        .bind(project_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PurgeStats {
    pub logs: u64,
    pub replays: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum PurgeError {
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Storage(#[from] object_store::Error),
}

/// Supprime, pour chaque projet, les logs et replays plus vieux que son délai de rétention.
pub async fn purge(pool: &MySqlPool, store: &BlobStore) -> Result<PurgeStats, PurgeError> {
    let projects: Vec<(u64, u32, u32)> =
        sqlx::query_as("SELECT id, logs_retention_days, replays_retention_days FROM projects").fetch_all(pool).await?;
    let mut stats = PurgeStats::default();
    for (project_id, logs_days, replays_days) in projects {
        if logs_days > 0 {
            stats.logs += purge_logs(pool, project_id, logs_days).await?;
        }
        if replays_days > 0 {
            stats.replays += purge_replays(pool, store, project_id, replays_days).await?;
        }
    }
    Ok(stats)
}

async fn purge_logs(pool: &MySqlPool, project_id: u64, days: u32) -> Result<u64, sqlx::Error> {
    let mut total = 0;
    loop {
        let n = sqlx::query("DELETE FROM logs WHERE project_id = ? AND occurred_at < UTC_TIMESTAMP(3) - INTERVAL ? DAY LIMIT 500")
            .bind(project_id)
            .bind(days)
            .execute(pool)
            .await?
            .rows_affected();
        total += n;
        if n < BATCH {
            return Ok(total);
        }
    }
}

async fn purge_replays(pool: &MySqlPool, store: &BlobStore, project_id: u64, days: u32) -> Result<u64, PurgeError> {
    let mut total = 0;
    loop {
        let old: Vec<(String,)> = sqlx::query_as(
            "SELECT replay_id FROM replays WHERE project_id = ? AND last_seen < UTC_TIMESTAMP(3) - INTERVAL ? DAY LIMIT 100",
        )
        .bind(project_id)
        .bind(days)
        .fetch_all(pool)
        .await?;
        if old.is_empty() {
            return Ok(total);
        }
        for (replay_id,) in &old {
            let keys: Vec<(String,)> = sqlx::query_as("SELECT blob_key FROM replay_segments WHERE project_id = ? AND replay_id = ?")
                .bind(project_id)
                .bind(replay_id)
                .fetch_all(pool)
                .await?;
            // Lignes d'abord : un blob orphelin est moins grave qu'un segment dont le fichier a disparu.
            sqlx::query("DELETE FROM replay_segments WHERE project_id = ? AND replay_id = ?")
                .bind(project_id)
                .bind(replay_id)
                .execute(pool)
                .await?;
            sqlx::query("DELETE FROM replays WHERE project_id = ? AND replay_id = ?")
                .bind(project_id)
                .bind(replay_id)
                .execute(pool)
                .await?;
            for (key,) in keys {
                store.delete(&key).await?;
            }
            total += 1;
        }
    }
}

/// Lance la purge au démarrage puis toutes les heures.
pub fn spawn_purge_loop(pool: MySqlPool, store: BlobStore) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            tick.tick().await;
            match purge(&pool, &store).await {
                Ok(s) if s.logs + s.replays > 0 => tracing::info!("rétention : {} logs, {} replays supprimés", s.logs, s.replays),
                Ok(_) => {}
                Err(e) => tracing::warn!("échec de la purge de rétention : {e}"),
            }
        }
    });
}
