use anyhow::{bail, Context};
use error500::auth::build_dsn;
use error500::config::Config;
use error500::storage::BlobStore;
use error500::{projects, users};
use sqlx::mysql::MySqlPoolOptions;

const USAGE: &str = "usage:
  error500 [serve]
  error500 create-user <email> <nom> <mot-de-passe>
  error500 create-project <email-propriétaire> <nom>";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    // Après la config : RUST_LOG peut venir du fichier .env.
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::new(&config.log_filter)).init();
    let pool = MySqlPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await
        .context("connexion à la base")?;
    sqlx::migrate!("./migrations").run(&pool).await.context("migrations")?;

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] | ["serve"] => {
            let store = BlobStore::from_config(&config.storage)?;
            error500::retention::spawn_purge_loop(pool.clone(), store.clone());
            let app = error500::app(pool, store, config.mailer()?, &config);
            let listener = tokio::net::TcpListener::bind(&config.bind).await?;
            tracing::info!("écoute sur {}", config.bind);
            axum::serve(listener, app).await?;
        }
        ["create-user", email, name, password] => {
            let u = users::create_user(&pool, email, name, password).await?;
            println!("utilisateur {} créé (id {}{})", u.email, u.id, if u.is_admin { ", admin" } else { "" });
        }
        ["create-project", owner_email, name] => {
            let owner: (u64,) = sqlx::query_as("SELECT id FROM users WHERE email = ?")
                .bind(owner_email.to_lowercase())
                .fetch_optional(&pool)
                .await?
                .with_context(|| format!("utilisateur introuvable : {owner_email}"))?;
            let p = projects::create_project(&pool, owner.0, name).await?;
            println!("projet {} (id {})\nDSN : {}", p.name, p.id, build_dsn(&config.public_url, &p.public_key, p.id));
        }
        _ => bail!("{USAGE}"),
    }
    Ok(())
}
