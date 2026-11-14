//! Ajustes del propio usuario: nombre, contraseña, verificación en dos pasos (2FA)
//! y Ajustes -> Preferencias.

use axum::{extract::{Query, State}, routing::{get, post, put}, Json, Router};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};

use super::auth::{user_info, UserInfo};
use crate::auth::{hash_token, password, totp, CurrentUser};
use crate::deca::qr;
use crate::error::{AppError, AppResult};
use crate::preferences::{self, Prefs};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", put(update_me))
        .route("/password", put(change_password))
        .route("/2fa/iniciar", post(totp_start))
        .route("/2fa/activar", post(totp_enable))
        .route("/2fa/desactivar", post(totp_disable))
        .route("/preferences", get(get_prefs).put(put_prefs))
        .route("/preferences/effective", get(effective))
}

#[derive(Deserialize)]
struct UpdateMe {
    nombre: Option<String>,
    usuario: Option<String>,
}

async fn update_me(State(state): State<AppState>, user: CurrentUser, Json(req): Json<UpdateMe>) -> AppResult<Json<UserInfo>> {
    if let Some(n) = req.nombre.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        sqlx::query("UPDATE users SET display_name = ?, updated_at = ? WHERE id = ?")
            .bind(n)
            .bind(Utc::now())
            .bind(user.id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(u) = req.usuario.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        super::workers::validate_username(u)?;
        sqlx::query("UPDATE users SET username = ?, updated_at = ? WHERE id = ?")
            .bind(u)
            .bind(Utc::now())
            .bind(user.id)
            .execute(&state.pool)
            .await?;
    }
    Ok(Json(user_info(&state, user.id).await?))
}

#[derive(Deserialize)]
struct ChangePassword {
    actual: String,
    nueva: String,
}

async fn change_password(State(state): State<AppState>, user: CurrentUser, headers: axum::http::HeaderMap, Json(req): Json<ChangePassword>) -> AppResult<Json<Value>> {
    let (hash,): (String,) = sqlx::query_as("SELECT password_hash FROM users WHERE id = ?")
        .bind(user.id)
        .fetch_one(&state.pool)
        .await?;
    if !password::verify(&req.actual, &hash) {
        return Err(AppError::bad("La contraseña actual no es correcta"));
    }
    password::validate_new(&req.nueva).map_err(AppError::BadRequest)?;
    // Conserva únicamente la sesión que acaba de demostrar la contraseña actual.
    let current = headers.get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok()).and_then(|h| h.strip_prefix("Bearer "))
        .map(|t| hash_token(t.trim())).ok_or(AppError::Unauthorized)?;
    crate::auth::update_password(&state.pool, user.id, &password::hash(&req.nueva)?,
        Some(&hash), Some(&current)).await?;
    Ok(Json(json!({ "ok": true, "mensaje": "Contraseña cambiada" })))
}

/// Paso 1: genera un secreto y devuelve el QR para la app de autenticación.
async fn totp_start(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    let secret = totp::generate_secret();
    let updated = sqlx::query("UPDATE users SET totp_pending_secret = ? WHERE id = ? AND totp_enabled = 0")
        .bind(&secret)
        .bind(user.id)
        .execute(&state.pool)
        .await?;
    if updated.rows_affected() != 1 {
        return Err(AppError::bad("Desactiva primero la verificación actual con su código"));
    }
    let url = totp::otpauth_url(&secret, "DeCA", &user.username);
    Ok(Json(json!({
        "secreto": secret,
        "otpauth_url": url,
        "qr_svg": qr::svg(&url)?,
        "instrucciones": "Escanea el QR con tu app de códigos (Google Authenticator, Aegis...) y escribe el código de 6 dígitos para activarlo."
    })))
}

#[derive(Deserialize)]
struct Code {
    codigo: String,
}

/// Paso 2: confirma con un código válido y activa la verificación en dos pasos.
async fn totp_enable(State(state): State<AppState>, user: CurrentUser, Json(req): Json<Code>) -> AppResult<Json<Value>> {
    let (pending,): (Option<String>,) = sqlx::query_as("SELECT totp_pending_secret FROM users WHERE id = ? AND totp_enabled = 0")
        .bind(user.id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::bad("La verificación ya está activada"))?;
    let pending = pending.ok_or_else(|| AppError::bad("Primero pulsa «Añadir código de verificación»"))?;
    if !totp::verify(&pending, &req.codigo) {
        return Err(AppError::bad("Código incorrecto. Revisa la hora del móvil y prueba otra vez"));
    }
    let updated = sqlx::query("UPDATE users SET totp_secret = ?, totp_enabled = 1, totp_pending_secret = NULL, updated_at = ? WHERE id = ? AND totp_enabled = 0 AND totp_pending_secret = ?")
        .bind(&pending)
        .bind(Utc::now())
        .bind(user.id)
        .bind(&pending)
        .execute(&state.pool)
        .await?;
    if updated.rows_affected() != 1 { return Err(AppError::bad("La configuración 2FA ha cambiado; vuelve a empezar")); }
    Ok(Json(json!({ "ok": true, "mensaje": "Verificación en dos pasos activada" })))
}

async fn totp_disable(State(state): State<AppState>, user: CurrentUser, Json(req): Json<Code>) -> AppResult<Json<Value>> {
    let (secret,): (Option<String>,) = sqlx::query_as("SELECT totp_secret FROM users WHERE id = ?")
        .bind(user.id)
        .fetch_one(&state.pool)
        .await?;
    if !totp::verify(secret.as_deref().unwrap_or(""), &req.codigo) {
        return Err(AppError::bad("Código incorrecto"));
    }
    let updated = sqlx::query("UPDATE users SET totp_secret = NULL, totp_enabled = 0, totp_pending_secret = NULL, updated_at = ? WHERE id = ? AND totp_enabled = 1 AND totp_secret IS ?")
        .bind(Utc::now()).bind(user.id).bind(secret.as_deref())
        .execute(&state.pool).await?;
    if updated.rows_affected() != 1 { return Err(AppError::bad("La configuración 2FA ha cambiado; vuelve a empezar")); }
    Ok(Json(json!({ "ok": true, "mensaje": "Verificación en dos pasos desactivada" })))
}

/// Ajustes -> Preferencias: lo que fija el usuario + lo que ya fijó el administrador.
async fn get_prefs(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    let (admin_worker, company, own) = preferences::admin_layers(&state.pool, user.id).await?;
    let fields: Vec<Value> = preferences::FIELDS
        .iter()
        .map(|(k, n)| json!({ "campo": k, "nombre": n, "bloqueado": admin_worker.contains_key(*k) || company.contains_key(*k) }))
        .collect();
    let visuales: Vec<Value> = preferences::visual(&admin_worker, &company, &own)
        .iter()
        .map(|f| json!({ "campo": f.campo, "nombre": f.nombre, "bloqueado": f.bloqueado, "valor": f.valor }))
        .collect();
    Ok(Json(json!({
        "propias": own,
        "fijadas_por_administrador": { "trabajador": admin_worker, "empresa": company },
        "campos": fields,
        "visuales": visuales,
    })))
}

async fn put_prefs(State(state): State<AppState>, user: CurrentUser, Json(input): Json<Prefs>) -> AppResult<Json<Value>> {
    let own = preferences::validate(input)?;
    let (admin_worker, company, _) = preferences::admin_layers(&state.pool, user.id).await?;
    preferences::reject_locked(&own, &admin_worker, &company)?;
    sqlx::query("UPDATE users SET prefs_own = ?, updated_at = ? WHERE id = ?")
        .bind(serde_json::to_string(&own)?)
        .bind(Utc::now())
        .bind(user.id)
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({ "ok": true, "propias": own })))
}

#[derive(Deserialize)]
pub struct CompanyQuery {
    pub empresa_id: Option<i64>,
}

async fn effective(State(state): State<AppState>, user: CurrentUser, Query(q): Query<CompanyQuery>) -> AppResult<Json<preferences::Effective>> {
    let company = if user.is_admin() { q.empresa_id } else { None };
    Ok(Json(preferences::effective(&state.pool, &user, company).await?))
}
