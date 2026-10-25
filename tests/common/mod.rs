//! Base MariaDB temporaire (une par test) sur l'instance en RAM de `scripts/test-db.sh`.
#![allow(dead_code)]

use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions};
use sqlx::{ConnectOptions, MySqlPool};
use std::str::FromStr;

pub struct TestDb {
    pub pool: MySqlPool,
    admin: MySqlConnectOptions,
    name: String,
}

impl TestDb {
    pub async fn new() -> Self {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "mysql://root@127.0.0.1:3307/error500_test".into());
        let admin = MySqlConnectOptions::from_str(&url).expect("DATABASE_URL invalide");
        let name = format!("t_{}", hex::encode(rand::random::<[u8; 6]>()));

        let mut conn = admin.connect().await.unwrap_or_else(|e| {
            panic!("MariaDB de test injoignable ({e}). Lancer `scripts/test-db.sh start`.")
        });
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE `{name}` CHARACTER SET utf8mb4")))
            .execute(&mut conn)
            .await
            .unwrap();

        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect_with(admin.clone().database(&name))
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        TestDb { pool, admin, name }
    }

    pub async fn cleanup(self) {
        self.pool.close().await;
        let mut conn = self.admin.connect().await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE `{}`", self.name)))
            .execute(&mut conn)
            .await
            .unwrap();
    }
}
