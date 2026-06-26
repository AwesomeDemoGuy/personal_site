//! SQLite database access layer (server-only).
//!
//! This module owns the connection pool and the schema bootstrap. Query
//! functions return the shared types from [`crate::models`]. Concrete content
//! (posts, projects, etc.) will be filled in later.

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

/// Resolve the SQLite database URL from the environment, defaulting to a file
/// in the working directory. Override with `DATABASE_URL`, e.g.
/// `sqlite:///data/personal_site.db` inside Docker.
fn database_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://personal_site.db".to_string())
}

/// Create the connection pool, creating the database file if missing, and run
/// the schema migration.
pub async fn init_pool() -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(&database_url())?
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    migrate(&pool).await?;
    Ok(pool)
}

/// Create tables if they do not already exist.
///
/// Kept as inline DDL for now; can be moved to versioned migration files
/// (`sqlx::migrate!`) once the schema stabilizes.
async fn migrate(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(
        r#"
        CREATE TABLE IF NOT EXISTS blog_posts (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            title        TEXT NOT NULL,
            slug         TEXT NOT NULL UNIQUE,
            body         TEXT NOT NULL,
            published_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS projects (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL,
            description TEXT NOT NULL,
            url         TEXT
        );

        CREATE TABLE IF NOT EXISTS certificates (
            id     INTEGER PRIMARY KEY AUTOINCREMENT,
            name   TEXT NOT NULL,
            issuer TEXT NOT NULL,
            year   INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS technologies (
            id       INTEGER PRIMARY KEY AUTOINCREMENT,
            name     TEXT NOT NULL,
            category TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS weather_cache (
            location    TEXT PRIMARY KEY,
            temp_f      REAL NOT NULL,
            description TEXT NOT NULL,
            emoji       TEXT NOT NULL,
            fetched_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}
