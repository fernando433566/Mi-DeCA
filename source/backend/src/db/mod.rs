//! Conexión a SQLite, creación del esquema y datos iniciales
//! (cuenta "Admin" en el primer arranque, ver `bootstrap`).

pub mod settings;
pub mod bootstrap;

use std::path::Path;
use std::str::FromStr;

use anyhow::Context;
use chrono::Utc;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;


pub const DEFAULT_ADMIN_USER: &str = "Admin";
pub const DEFAULT_ADMIN_PASSWORD: &str = "1234";

pub async fn connect(path: &Path) -> anyhow::Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(10));
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await
        .context("no se pudo abrir la base de datos")?;
    sqlx::raw_sql(include_str!("schema.sql"))
        .execute(&pool)
        .await
        .context("no se pudo crear el esquema")?;
    migrate(&pool).await?;
    bootstrap::seed(&pool).await?;
    Ok(pool)
}

/// Cambios de esquema para bases de datos creadas con versiones anteriores.
async fn migrate(pool: &SqlitePool) -> anyhow::Result<()> {
    for (table, column, decl) in [
        ("companies", "titular", "TEXT NOT NULL DEFAULT ''"),
        ("parties", "titular", "TEXT NOT NULL DEFAULT ''"),
        ("users", "must_change_password", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        let exists: bool = sqlx::query_scalar(&format!("SELECT COUNT(*) > 0 FROM pragma_table_info('{table}') WHERE name = ?"))
            .bind(column)
            .fetch_one(pool)
            .await?;
        if !exists {
            sqlx::query(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl}")).execute(pool).await?;
        }
    }
    sqlx::query("DROP TABLE IF EXISTS map_regions").execute(pool).await?;
    Ok(())
}

/// Añade una entrada al registro de auditoría (fecha y hora de cada acción).
pub async fn audit(
    pool: &SqlitePool,
    user_id: Option<i64>,
    action: &str,
    deca_id: Option<i64>,
    detail: impl Into<String>,
) {
    let res = sqlx::query(
        "INSERT INTO audit_log (ts, user_id, action, deca_id, detail) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(Utc::now())
    .bind(user_id)
    .bind(action)
    .bind(deca_id)
    .bind(detail.into())
    .execute(pool)
    .await;
    if let Err(e) = res {
        tracing::error!("no se pudo escribir en el registro de auditoría: {e}");
    }
}
