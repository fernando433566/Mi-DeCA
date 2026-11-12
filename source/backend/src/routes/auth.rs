//! Inicio de sesión: manual (usuario + contraseña [+ código 2FA]) o automático por QR.

use axum::{extract::State, routing::{get, post}, Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::{self, hash_token, password, totp, CurrentUser};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/qr", post(qr_login))
        .route("/logout", post(logout))
        .route("/me", get(me))
}

#[derive(Deserialize)]
struct LoginReq {
    usuario: String,
    contrasena: String,
    /// Código de 6 dígitos si el usuario tiene activada la verificación en dos pasos.
    codigo_2fa: Option<String>,
}

#[derive(Serialize)]
pub struct LoginResp {
    token: String,
    caduca: DateTime<Utc>,
    usuario: UserInfo,
}

#[derive(Serialize)]
pub struct UserInfo {
    pub id: i64,
    pub usuario: String,
    pub nombre: String,
    pub rol: auth::Role,
    pub empresa_id: Option<i64>,
    pub empresa_nombre: Option<String>,
    pub tiene_2fa: bool,
    /// El administrador sigue con la contraseña inicial "1234": solo puede cambiarla.
    pub contrasena_por_defecto: bool,
}

pub async fn user_info(state: &AppState, user_id: i64) -> AppResult<UserInfo> {
    let row: Option<(i64, String, String, String, Option<i64>, Option<String>, bool, bool)> = sqlx::query_as(
        "SELECT u.id, u.username, u.display_name, u.role, u.company_id, c.name, u.totp_enabled, u.must_change_password
         FROM users u LEFT JOIN companies c ON c.id = u.company_id WHERE u.id = ?",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?;
    let (id, usuario, nombre, rol, empresa_id, empresa_nombre, tiene_2fa, contrasena_por_defecto) =
        row.ok_or_else(|| AppError::not_found("Usuario no encontrado"))?;
    let rol = auth::Role::from_db(&rol);
    Ok(UserInfo {
        id,
        usuario,
        nombre,
        contrasena_por_defecto,
        rol,
        empresa_id,
        empresa_nombre,
        tiene_2fa,
    })
}

async fn login(State(state): State<AppState>, Json(req): Json<LoginReq>) -> AppResult<Json<LoginResp>> {
    let key = req.usuario.trim().to_string();
    if key.is_empty() || key.len() > 160 || req.contrasena.len() > 256 {
        return Err(AppError::InvalidCredentials);
    }
    if state.login_limiter.is_blocked(&key) {
        return Err(AppError::TooManyRequests);
    }
    let row: Option<(i64, String, bool, Option<String>)> = sqlx::query_as(
        "SELECT id, password_hash, totp_enabled, totp_secret FROM users WHERE username = ? AND active = 1",
    )
    .bind(&key)
    .fetch_optional(&state.pool)
    .await?;
    let Some((id, hash, totp_on, secret)) = row else {
        // Solo se cuentan fallos de cuentas que existen: así la tabla del limitador
        // no crece con nombres inventados y nadie puede llenarla para bloquear a otros.
        return Err(AppError::InvalidCredentials);
    };
    let verified_hash = hash.clone();
    let pw = req.contrasena.clone();
    let ok = tokio::task::spawn_blocking(move || password::verify(&pw, &hash))
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if !ok {
        state.login_limiter.fail(&key);
        return Err(AppError::InvalidCredentials);
    }
    if totp_on {
        let code = req.codigo_2fa.as_deref().unwrap_or("");
        if code.is_empty() {
            return Err(AppError::TotpRequired);
        }
        if !totp::verify(secret.as_deref().unwrap_or(""), code) {
            state.login_limiter.fail(&key);
            return Err(AppError::bad("Código de verificación incorrecto"));
        }
    }
    state.login_limiter.success(&key);
    let (token, caduca) = auth::issue_session(&mut *state.pool.acquire().await?, id,
        Some(&verified_hash), totp_on, secret.as_deref()).await?;
    db::audit(&state.pool, Some(id), "inicio_sesion", None, "manual").await;
    Ok(Json(LoginResp { token, caduca, usuario: user_info(&state, id).await? }))
}

#[derive(Deserialize)]
struct QrLoginReq {
    /// Valor `codigo` leído del QR que enseña el administrador.
    codigo: String,
}

async fn qr_login(State(state): State<AppState>, Json(req): Json<QrLoginReq>) -> AppResult<Json<LoginResp>> {
    let now = Utc::now();
    let mut tx = state.pool.begin().await?;
    let row: Option<(i64,)> = sqlx::query_as(
        "UPDATE qr_logins SET used = 1
         WHERE token_hash = ? AND used = 0 AND expires_at > ?
           AND EXISTS (SELECT 1 FROM users WHERE id = qr_logins.user_id AND active = 1)
         RETURNING user_id",
    )
    .bind(hash_token(req.codigo.trim()))
    .bind(now)
    .fetch_optional(&mut *tx)
    .await?;
    let (user_id,) = row.ok_or_else(|| AppError::bad("Código QR no válido o caducado. Pide al administrador uno nuevo"))?;
    let (token, caduca) = auth::issue_session(&mut *tx, user_id, None, false, None).await?;
    tx.commit().await?;
    db::audit(&state.pool, Some(user_id), "inicio_sesion", None, "qr").await;
    Ok(Json(LoginResp { token, caduca, usuario: user_info(&state, user_id).await? }))
}

async fn logout(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    _user: CurrentUser,
) -> AppResult<Json<serde_json::Value>> {
    if let Some(token) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
    {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(hash_token(token.trim()))
            .execute(&state.pool)
            .await?;
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn me(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<UserInfo>> {
    Ok(Json(user_info(&state, user.id).await?))
}
