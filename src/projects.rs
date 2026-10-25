//! Projets, membres et clés d'ingestion.

use crate::users::User;
use sqlx::{FromRow, MySqlPool};

#[derive(Debug, Clone, FromRow)]
pub struct Project {
    pub id: u64,
    pub name: String,
    pub slug: String,
    pub public_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Owner,
    Member,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Member => "member",
        }
    }

    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "owner" => Some(Role::Owner),
            "member" => Some(Role::Member),
            _ => None,
        }
    }
}

fn new_public_key() -> String {
    hex::encode(rand::random::<[u8; 16]>())
}

fn slugify(name: &str) -> String {
    let mut slug = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-').to_string();
    if slug.is_empty() { "project".into() } else { slug }
}

pub async fn create_project(
    pool: &MySqlPool,
    owner_id: u64,
    name: &str,
) -> Result<Project, sqlx::Error> {
    let base = slugify(name);
    let mut attempt = 1;
    loop {
        let slug = if attempt == 1 { base.clone() } else { format!("{base}-{attempt}") };
        let public_key = new_public_key();
        let mut tx = pool.begin().await?;
        let res = sqlx::query("INSERT INTO projects (name, slug, public_key, upload_token) VALUES (?, ?, ?, ?)")
            .bind(name)
            .bind(&slug)
            .bind(&public_key)
            .bind(new_public_key())
            .execute(&mut *tx)
            .await;
        match res {
            Ok(r) => {
                let id = r.last_insert_id();
                sqlx::query("INSERT INTO project_members (project_id, user_id, role) VALUES (?, ?, 'owner')")
                    .bind(id)
                    .bind(owner_id)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                return Ok(Project { id, name: name.to_string(), slug, public_key });
            }
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => attempt += 1,
            Err(e) => return Err(e),
        }
    }
}

pub async fn add_member(
    pool: &MySqlPool,
    project_id: u64,
    user_id: u64,
    role: Role,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO project_members (project_id, user_id, role) VALUES (?, ?, ?) \
         ON DUPLICATE KEY UPDATE role = VALUES(role)",
    )
    .bind(project_id)
    .bind(user_id)
    .bind(role.as_str())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn user_role(
    pool: &MySqlPool,
    project_id: u64,
    user_id: u64,
) -> Result<Option<Role>, sqlx::Error> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT role FROM project_members WHERE project_id = ? AND user_id = ?")
            .bind(project_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|(r,)| Role::parse(&r)))
}

/// Projets visibles par l'utilisateur (tous pour un admin), triés par id.
pub async fn list_for_user(pool: &MySqlPool, user: &User) -> Result<Vec<Project>, sqlx::Error> {
    if user.is_admin {
        sqlx::query_as("SELECT id, name, slug, public_key FROM projects ORDER BY id")
            .fetch_all(pool)
            .await
    } else {
        sqlx::query_as(
            "SELECT p.id, p.name, p.slug, p.public_key FROM projects p \
             JOIN project_members m ON m.project_id = p.id WHERE m.user_id = ? ORDER BY p.id",
        )
        .bind(user.id)
        .fetch_all(pool)
        .await
    }
}

/// Le couple (id, clé publique) d'une requête d'ingestion désigne-t-il un projet ?
pub async fn authorize_ingest(
    pool: &MySqlPool,
    project_id: u64,
    public_key: &str,
) -> Result<Option<Project>, sqlx::Error> {
    sqlx::query_as("SELECT id, name, slug, public_key FROM projects WHERE id = ? AND public_key = ?")
        .bind(project_id)
        .bind(public_key)
        .fetch_optional(pool)
        .await
}

pub async fn rotate_key(pool: &MySqlPool, project_id: u64) -> Result<String, sqlx::Error> {
    let key = new_public_key();
    sqlx::query("UPDATE projects SET public_key = ? WHERE id = ?")
        .bind(&key)
        .bind(project_id)
        .execute(pool)
        .await?;
    Ok(key)
}

/// Jeton (Bearer) qui autorise l'upload de source maps vers ce projet.
pub async fn upload_token(pool: &MySqlPool, project_id: u64) -> Result<String, sqlx::Error> {
    sqlx::query_scalar("SELECT upload_token FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_one(pool)
        .await
}

/// Projet auquel appartient ce jeton d'upload (sentry-cli ne transmet pas l'id du projet dans l'URL).
pub async fn project_id_by_upload_token(pool: &MySqlPool, token: &str) -> Result<Option<u64>, sqlx::Error> {
    if token.is_empty() {
        return Ok(None);
    }
    let found: Option<(u64,)> = sqlx::query_as("SELECT id FROM projects WHERE upload_token = ?").bind(token).fetch_optional(pool).await?;
    Ok(found.map(|(id,)| id))
}

pub async fn authorize_upload(pool: &MySqlPool, project_id: u64, token: &str) -> Result<bool, sqlx::Error> {
    if token.is_empty() {
        return Ok(false);
    }
    let found: Option<(u64,)> = sqlx::query_as("SELECT id FROM projects WHERE id = ? AND upload_token = ?")
        .bind(project_id)
        .bind(token)
        .fetch_optional(pool)
        .await?;
    Ok(found.is_some())
}

pub async fn rotate_upload_token(pool: &MySqlPool, project_id: u64) -> Result<String, sqlx::Error> {
    let token = new_public_key();
    sqlx::query("UPDATE projects SET upload_token = ? WHERE id = ?")
        .bind(&token)
        .bind(project_id)
        .execute(pool)
        .await?;
    Ok(token)
}

pub async fn get(pool: &MySqlPool, project_id: u64) -> Result<Option<Project>, sqlx::Error> {
    sqlx::query_as("SELECT id, name, slug, public_key FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_optional(pool)
        .await
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Member {
    pub user_id: u64,
    pub name: String,
    pub email: String,
    pub role: &'static str,
}

pub async fn list_members(pool: &MySqlPool, project_id: u64) -> Result<Vec<Member>, sqlx::Error> {
    let rows: Vec<(u64, String, String, String)> = sqlx::query_as(
        "SELECT u.id, u.name, u.email, m.role FROM project_members m JOIN users u ON u.id = m.user_id \
         WHERE m.project_id = ? ORDER BY m.role, u.name",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(user_id, name, email, role)| {
            Some(Member { user_id, name, email, role: Role::parse(&role)?.as_str() })
        })
        .collect())
}

#[derive(Debug, thiserror::Error)]
pub enum RemoveMemberError {
    #[error("le dernier propriétaire ne peut pas être retiré")]
    LastOwner,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

pub async fn remove_member(pool: &MySqlPool, project_id: u64, user_id: u64) -> Result<(), RemoveMemberError> {
    let mut tx = pool.begin().await?;
    let (owners,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM project_members WHERE project_id = ? AND role = 'owner' FOR UPDATE")
            .bind(project_id)
            .fetch_one(&mut *tx)
            .await?;
    let is_owner: Option<(String,)> =
        sqlx::query_as("SELECT role FROM project_members WHERE project_id = ? AND user_id = ? AND role = 'owner'")
            .bind(project_id)
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?;
    if is_owner.is_some() && owners <= 1 {
        return Err(RemoveMemberError::LastOwner);
    }
    sqlx::query("DELETE FROM project_members WHERE project_id = ? AND user_id = ?")
        .bind(project_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
