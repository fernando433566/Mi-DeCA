//! Autenticación: sesiones con token Bearer, extractores `CurrentUser` / `AdminUser`
//! y utilidades de tokens aleatorios.

pub mod password;
pub mod totp;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::state::AppState;

/// Duración de una sesión iniciada en la app.
pub const SESSION_DAYS: i64 = 90;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Worker,
}

impl Role {
    pub fn from_db(s: &str) -> Self {
        if s == "admin" { Role::Admin } else { Role::Worker }
    }
}

/// Usuario que hace la petición (obtenido del token de sesión).
#[derive(Clone, Debug, Serialize)]
pub struct CurrentUser {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    pub company_id: Option<i64>,
}

impl CurrentUser {
    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }
}

/// Igual que `CurrentUser` pero rechaza a quien no sea administrador.
pub struct AdminUser(pub CurrentUser);

/// Token aleatorio largo e impredecible (256 bits, Base64 URL).
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// En la base de datos solo se guarda el hash del token de sesión.
pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    data_encoding::HEXLOWER.encode(&digest)
}

/// Emite una sesión usando la misma conexión que validó un QR, o comparando
/// atómicamente la contraseña y el factor comprobados durante el login.
pub async fn issue_session(connection: &mut sqlx::SqliteConnection, user_id: i64,
    verified_password: Option<&str>, totp_enabled: bool, totp_secret: Option<&str>)
    -> Result<(String, DateTime<Utc>), AppError> {
    let token = random_token();
    let now = Utc::now();
    let expires = now + Duration::days(SESSION_DAYS);
    let inserted = sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at, last_used_at)
         SELECT ?, id, ?, ?, ? FROM users WHERE id = ? AND active = 1
         AND (? IS NULL OR (password_hash = ? AND totp_enabled = ? AND totp_secret IS ?))",
    )
    .bind(hash_token(&token)).bind(now).bind(expires).bind(now).bind(user_id)
    .bind(verified_password).bind(verified_password).bind(totp_enabled).bind(totp_secret)
    .execute(connection).await?;
    if inserted.rows_affected() != 1 { return Err(AppError::Unauthorized); }
    Ok((token, expires))
}

#[cfg(test)]
pub async fn create_session(pool: &SqlitePool, user_id: i64) -> Result<(String, DateTime<Utc>), AppError> {
    issue_session(&mut *pool.acquire().await?, user_id, None, false, None).await
}

pub async fn delete_user_sessions(pool: &SqlitePool, user_id: i64) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(user_id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM qr_logins WHERE user_id = ?")
        .bind(user_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Cambia la clave y revoca sus credenciales en la misma transacción.
/// `expected_hash` evita aplicar una comprobación de contraseña que ya quedó obsoleta.
pub async fn update_password(pool: &SqlitePool, user_id: i64, new_hash: &str,
    expected_hash: Option<&str>, keep_session_hash: Option<&str>) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    let updated = sqlx::query("UPDATE users SET password_hash = ?, must_change_password = 0, updated_at = ? WHERE id = ? AND active = 1 AND (? IS NULL OR password_hash = ?)")
        .bind(new_hash).bind(Utc::now()).bind(user_id).bind(expected_hash).bind(expected_hash)
        .execute(&mut *tx).await?;
    if updated.rows_affected() != 1 { return Err(AppError::Unauthorized); }
    sqlx::query("DELETE FROM sessions WHERE user_id = ? AND (? IS NULL OR token_hash != ?)")
        .bind(user_id).bind(keep_session_hash).bind(keep_session_hash).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM qr_logins WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

fn bearer(parts: &Parts) -> Option<&str> {
    parts
        .headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
}

/// Lo único que puede hacer una cuenta con la contraseña inicial: ver quién es
/// y cambiarla. Se usa la ruta original porque los routers anidados la recortan.
fn allowed_before_password_change(parts: &Parts) -> bool {
    let path = parts
        .extensions
        .get::<axum::extract::OriginalUri>()
        .map(|u| u.0.path().to_string())
        .unwrap_or_else(|| parts.uri.path().to_string());
    matches!(path.as_str(), "/api/auth/me" | "/api/me" | "/api/me/password" | "/api/setup/status")
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = bearer(parts).ok_or(AppError::Unauthorized)?;
        let token_hash = hash_token(token);
        let now = Utc::now();
        let row: Option<(i64, String, String, String, Option<i64>, bool)> = sqlx::query_as(
            "SELECT u.id, u.username, u.display_name, u.role, u.company_id, u.must_change_password
             FROM sessions s JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = ? AND s.expires_at > ? AND u.active = 1",
        )
        .bind(&token_hash)
        .bind(now)
        .fetch_optional(&state.pool)
        .await?;
        let (id, username, display_name, role, company_id, must_change) = row.ok_or(AppError::Unauthorized)?;
        if must_change && !allowed_before_password_change(parts) {
            return Err(AppError::PasswordChangeRequired);
        }
        sqlx::query("UPDATE sessions SET last_used_at = ? WHERE token_hash = ?")
            .bind(now)
            .bind(&token_hash)
            .execute(&state.pool)
            .await?;
        Ok(CurrentUser {
            id,
            username,
            display_name,
            role: Role::from_db(&role),
            company_id,
        })
    }
}

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let user = CurrentUser::from_request_parts(parts, state).await?;
        if !user.is_admin() {
            return Err(AppError::Forbidden);
        }
        Ok(AdminUser(user))
    }
}
