//! Alta inicial del administrador y marca de «contraseña por defecto».
//!
//! En el primer arranque se crea `Admin`. Su contraseña es la que aporte el operador
//! (`DECA_ADMIN_PASSWORD_FILE` o `DECA_ADMIN_PASSWORD`, 12+ caracteres) o, si no hay
//! ninguna, la inicial `1234`. Mientras un administrador conserve `1234`, su cuenta
//! queda marcada con `must_change_password` y el servidor solo le deja cambiarla
//! (ver `auth::CurrentUser`).

use anyhow::{bail, Context};
use chrono::Utc;
use sqlx::{SqliteConnection, SqlitePool};

use super::{DEFAULT_ADMIN_PASSWORD, DEFAULT_ADMIN_USER};
use crate::auth::password;

/// Contraseña inicial opcional aportada por el operador. Nunca se imprime.
fn operator_password() -> anyhow::Result<Option<String>> {
    let value = if let Ok(path) = std::env::var("DECA_ADMIN_PASSWORD_FILE") {
        let bytes = std::fs::read(path).context("no se pudo leer DECA_ADMIN_PASSWORD_FILE")?;
        if bytes.len() > 1024 {
            bail!("el fichero de contraseña inicial es demasiado grande");
        }
        String::from_utf8(bytes).context("la contraseña inicial debe ser UTF-8")?
    } else {
        match std::env::var("DECA_ADMIN_PASSWORD") {
            Ok(v) => v,
            Err(_) => return Ok(None),
        }
    };
    let value = value.trim_end_matches(['\r', '\n']).to_string();
    if value.is_empty() {
        return Ok(None);
    }
    password::validate_new(&value).map_err(anyhow::Error::msg)?;
    if value.chars().count() < 12 {
        bail!("DECA_ADMIN_PASSWORD debe tener al menos 12 caracteres");
    }
    Ok(Some(value))
}

/// Crea la cuenta `Admin` si no hay ningún usuario y actualiza las marcas.
pub(super) async fn seed(pool: &SqlitePool) -> anyhow::Result<()> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(pool).await?;
    if count == 0 {
        let chosen = operator_password()?;
        let initial = chosen.as_deref().unwrap_or(DEFAULT_ADMIN_PASSWORD);
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO users (username, display_name, password_hash, role, created_at, updated_at)
             VALUES (?, ?, ?, 'admin', ?, ?)",
        )
        .bind(DEFAULT_ADMIN_USER)
        .bind(DEFAULT_ADMIN_USER)
        .bind(password::hash(initial)?)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await?;
        match chosen {
            Some(_) => tracing::info!("Primer arranque: creada la cuenta '{DEFAULT_ADMIN_USER}' con la contraseña del operador"),
            None => tracing::warn!(
                "Primer arranque: creada la cuenta '{DEFAULT_ADMIN_USER}' con la contraseña inicial. \
                 Solo podrá usarse para cambiarla; entra cuanto antes o configura DECA_ADMIN_PASSWORD"
            ),
        }
    }
    flag_default_passwords(&mut *pool.acquire().await?).await?;
    Ok(())
}

/// Marca los administradores activos que siguen con la contraseña inicial.
/// Se ejecuta al arrancar y al restaurar una copia (las copias antiguas no traen la marca).
pub async fn flag_default_passwords(conn: &mut SqliteConnection) -> anyhow::Result<()> {
    let admins: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, password_hash FROM users WHERE role = 'admin' AND active = 1")
            .fetch_all(&mut *conn)
            .await?;
    for (id, hash) in admins {
        let is_default = password::verify(DEFAULT_ADMIN_PASSWORD, &hash);
        sqlx::query("UPDATE users SET must_change_password = ? WHERE id = ?")
            .bind(is_default)
            .bind(id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}
