//! SQLite database access layer (server-only).
//!
//! This module owns the connection pool, schema bootstrap, blog post queries,
//! and weather cache storage. Static
//! presentation content (the About page's intro, certificates, technologies,
//! etc.) lives in the markup, not here.

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

use crate::models::{BlogPost, BlogPostSummary};

pub async fn list_blog_posts(pool: &SqlitePool) -> Result<Vec<BlogPostSummary>, sqlx::Error> {
    sqlx::query_as(
        "SELECT title, slug, published_at FROM blog_posts ORDER BY published_at DESC, id DESC",
    )
    .fetch_all(pool)
    .await
}

pub async fn find_blog_post(
    pool: &SqlitePool,
    slug: &str,
) -> Result<Option<BlogPost>, sqlx::Error> {
    sqlx::query_as("SELECT id, title, slug, body, published_at FROM blog_posts WHERE slug = ?")
        .bind(slug)
        .fetch_optional(pool)
        .await
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn blog_queries_order_posts_and_bind_the_slug() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        migrate(&pool).await.unwrap();
        assert!(list_blog_posts(&pool).await.unwrap().is_empty());
        for (title, slug, date) in [
            ("Older", "older", "2026-01-01"),
            ("Newer", "newer", "2026-02-01"),
        ] {
            sqlx::query(
                "INSERT INTO blog_posts (title, slug, body, published_at) VALUES (?, ?, ?, ?)",
            )
            .bind(title)
            .bind(slug)
            .bind("**Raw Markdown**")
            .bind(date)
            .execute(&pool)
            .await
            .unwrap();
        }
        let posts = list_blog_posts(&pool).await.unwrap();
        assert_eq!(
            posts.iter().map(|p| p.slug.as_str()).collect::<Vec<_>>(),
            ["newer", "older"]
        );
        assert_eq!(
            find_blog_post(&pool, "newer").await.unwrap().unwrap().body,
            "**Raw Markdown**"
        );
        assert!(find_blog_post(&pool, "missing").await.unwrap().is_none());
        assert!(find_blog_post(&pool, "' OR 1=1 --")
            .await
            .unwrap()
            .is_none());
    }
}
