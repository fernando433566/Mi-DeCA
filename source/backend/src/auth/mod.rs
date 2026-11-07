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

/// Crea una sesión y devuelve el token en claro (solo se entrega una vez).
pub async fn create_session(pool: &SqlitePool, user_id: i64) -> Result<(String, DateTime<Utc>), AppError> {
    let token = random_token();
    let now = Utc::now();
    let expires = now + Duration::days(SESSION_DAYS);
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at, last_used_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(hash_token(&token))
    .bind(user_id)
    .bind(now)
    .bind(expires)
    .bind(now)
    .execute(pool)
    .await?;
    Ok((token, expires))
}

pub async fn delete_user_sessions(pool: &SqlitePool, user_id: i64) -> Result<(), AppError> {
    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
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

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = bearer(parts).ok_or(AppError::Unauthorized)?;
        let token_hash = hash_token(token);
        let now = Utc::now();
        let row: Option<(i64, String, String, String, Option<i64>)> = sqlx::query_as(
            "SELECT u.id, u.username, u.display_name, u.role, u.company_id
             FROM sessions s JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = ? AND s.expires_at > ? AND u.active = 1",
        )
        .bind(&token_hash)
        .bind(now)
        .fetch_optional(&state.pool)
        .await?;
        let (id, username, display_name, role, company_id) = row.ok_or(AppError::Unauthorized)?;
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
