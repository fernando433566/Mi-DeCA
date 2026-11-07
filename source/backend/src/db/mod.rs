//! Conexión a SQLite, creación del esquema y datos iniciales
//! (cuenta "Admin" / "1234" en el primer arranque).

pub mod settings;

use std::path::Path;
use std::str::FromStr;

use anyhow::Context;
use chrono::Utc;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::auth::password;

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
    seed(&pool).await?;
    Ok(pool)
}

/// Cambios de esquema para bases de datos creadas con versiones anteriores.
async fn migrate(pool: &SqlitePool) -> anyhow::Result<()> {
    for (table, column, decl) in [
        ("companies", "titular", "TEXT NOT NULL DEFAULT ''"),
        ("parties", "titular", "TEXT NOT NULL DEFAULT ''"),
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

/// Crea la cuenta de administrador por defecto si no hay ningún usuario.
async fn seed(pool: &SqlitePool) -> anyhow::Result<()> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    if count == 0 {
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO users (username, display_name, password_hash, role, created_at, updated_at)
             VALUES (?, ?, ?, 'admin', ?, ?)",
        )
        .bind(DEFAULT_ADMIN_USER)
        .bind(DEFAULT_ADMIN_USER)
        .bind(password::hash(DEFAULT_ADMIN_PASSWORD)?)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await?;
        tracing::warn!(
            "Primer arranque: creada la cuenta '{DEFAULT_ADMIN_USER}' con contraseña '{DEFAULT_ADMIN_PASSWORD}'. Cámbiala cuanto antes."
        );
    }
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
