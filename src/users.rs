//! Utilisateurs, mots de passe (argon2) et sessions.

use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::Argon2;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, MySqlPool};

const MIN_PASSWORD_LEN: usize = 8;
const SESSION_DAYS: i64 = 30;

#[derive(Debug, thiserror::Error)]
pub enum UserError {
    #[error("adresse e-mail déjà utilisée")]
    EmailTaken,
    #[error("mot de passe trop court (8 caractères minimum)")]
    WeakPassword,
    #[error("erreur de hachage : {0}")]
    Hash(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: u64,
    pub email: String,
    pub name: String,
    pub is_admin: bool,
}

pub async fn create_user(
    pool: &MySqlPool,
    email: &str,
    name: &str,
    password: &str,
) -> Result<User, UserError> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(UserError::WeakPassword);
    }
    let email = email.trim().to_lowercase();
    let hash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(|e| UserError::Hash(e.to_string()))?
        .to_string();

    let mut tx = pool.begin().await?;
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users FOR UPDATE")
        .fetch_one(&mut *tx)
        .await?;
    let is_admin = count == 0;
    let res = sqlx::query("INSERT INTO users (email, name, password_hash, is_admin) VALUES (?, ?, ?, ?)")
        .bind(&email)
        .bind(name)
        .bind(hash)
        .bind(is_admin)
        .execute(&mut *tx)
        .await;
    let id = match res {
        Ok(r) => r.last_insert_id(),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => return Err(UserError::EmailTaken),
        Err(e) => return Err(e.into()),
    };
    tx.commit().await?;
    Ok(User { id, email, name: name.to_string(), is_admin })
}

pub async fn authenticate(
    pool: &MySqlPool,
    email: &str,
    password: &str,
) -> Result<Option<User>, UserError> {
    let row: Option<(u64, String, String, bool, String)> = sqlx::query_as(
        "SELECT id, email, name, is_admin, password_hash FROM users WHERE email = ?",
    )
    .bind(email.trim().to_lowercase())
    .fetch_optional(pool)
    .await?;
    let Some((id, email, name, is_admin, hash)) = row else {
        return Ok(None);
    };
    let parsed = PasswordHash::new(&hash).map_err(|e| UserError::Hash(e.to_string()))?;
    if Argon2::default().verify_password(password.as_bytes(), &parsed).is_err() {
        return Ok(None);
    }
    Ok(Some(User { id, email, name, is_admin }))
}

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Crée une session et renvoie le jeton en clair (seul son hash est stocké).
pub async fn create_session(pool: &MySqlPool, user_id: u64) -> Result<String, UserError> {
    let token = hex::encode(rand::random::<[u8; 32]>());
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, expires_at) VALUES (?, ?, DATE_ADD(NOW(), INTERVAL ? DAY))",
    )
    .bind(hash_token(&token))
    .bind(user_id)
    .bind(SESSION_DAYS)
    .execute(pool)
    .await?;
    Ok(token)
}

pub async fn user_for_session(pool: &MySqlPool, token: &str) -> Result<Option<User>, UserError> {
    Ok(sqlx::query_as(
        "SELECT u.id, u.email, u.name, u.is_admin FROM sessions s \
         JOIN users u ON u.id = s.user_id WHERE s.token_hash = ? AND s.expires_at > NOW()",
    )
    .bind(hash_token(token))
    .fetch_optional(pool)
    .await?)
}

pub async fn delete_session(pool: &MySqlPool, token: &str) -> Result<(), UserError> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
        .bind(hash_token(token))
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn count(pool: &MySqlPool) -> Result<u64, sqlx::Error> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(pool).await?;
    Ok(n as u64)
}

#[derive(Debug, Clone, serde::Serialize, FromRow)]
pub struct UserRow {
    pub id: u64,
    pub email: String,
    pub name: String,
    pub is_admin: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list(pool: &MySqlPool) -> Result<Vec<UserRow>, sqlx::Error> {
    sqlx::query_as("SELECT id, email, name, is_admin, created_at FROM users ORDER BY id").fetch_all(pool).await
}

pub async fn find_by_email(pool: &MySqlPool, email: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as("SELECT id, email, name, is_admin FROM users WHERE email = ?")
        .bind(email.trim().to_lowercase())
        .fetch_optional(pool)
        .await
}

pub async fn set_admin(pool: &MySqlPool, user_id: u64, is_admin: bool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET is_admin = ? WHERE id = ?").bind(is_admin).bind(user_id).execute(pool).await?;
    Ok(())
}
