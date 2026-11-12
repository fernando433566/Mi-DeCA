//! Ajustes -> <Empresa> -> Trabajadores -> <Perfil del trabajador> (solo administrador):
//! cambiar usuario, nombre o contraseña, quitar el 2FA, gestionar preferencias,
//! "Enseñar código QR" y eliminar la cuenta.

use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::FromRow;

use crate::auth::{self, hash_token, password, random_token, AdminUser};
use crate::deca::qr;
use crate::public_url;
use crate::error::{AppError, AppResult};
use crate::preferences::{self, Prefs};
use crate::state::AppState;

/// Minutos de validez del QR de acceso automático.
const QR_LOGIN_MINUTES: i64 = 10;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{id}", get(get_one).put(update).delete(remove))
        .route("/{id}/2fa", delete(remove_2fa))
        .route("/{id}/preferences", get(get_prefs).put(put_prefs))
        .route("/{id}/qr-login", post(qr_login))
}

#[derive(Serialize, FromRow)]
pub struct Worker {
    id: i64,
    #[sqlx(rename = "username")]
    usuario: String,
    #[sqlx(rename = "display_name")]
    nombre: String,
    #[sqlx(rename = "company_id")]
    empresa_id: Option<i64>,
    #[sqlx(rename = "totp_enabled")]
    tiene_2fa: bool,
    #[sqlx(rename = "created_at")]
    creado: DateTime<Utc>,
    /// Tiene permisos de administrador (por defecto, no).
    administrador: bool,
    /// DeCA emitidos en los últimos 7 días
    decas_7_dias: i64,
}

const SELECT: &str = "SELECT u.id, u.username, u.display_name, u.company_id, u.totp_enabled, u.created_at,
    u.role = 'admin' AS administrador,
    (SELECT COUNT(*) FROM decas d WHERE d.user_id = u.id AND d.created_at >= ?) AS decas_7_dias
    FROM users u";

fn week_ago() -> DateTime<Utc> {
    Utc::now() - Duration::days(7)
}

pub async fn list_for_company(state: &AppState, company_id: i64) -> AppResult<Vec<Worker>> {
    Ok(sqlx::query_as(&format!("{SELECT} WHERE u.company_id = ? AND u.active = 1 ORDER BY u.display_name"))
        .bind(week_ago())
        .bind(company_id)
        .fetch_all(&state.pool)
        .await?)
}

pub async fn fetch(state: &AppState, id: i64) -> AppResult<Worker> {
    sqlx::query_as(&format!("{SELECT} WHERE u.id = ? AND u.active = 1 AND NOT (u.role = 'admin' AND u.company_id IS NULL)"))
        .bind(week_ago())
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("Trabajador no encontrado"))
}

pub fn validate_username(u: &str) -> AppResult<()> {
    let ok_chars = u.chars().all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if u.len() < 2 || u.len() > 40 || !ok_chars {
        return Err(AppError::bad("El usuario debe tener entre 2 y 40 letras o números, sin espacios"));
    }
    Ok(())
}

async fn get_one(State(state): State<AppState>, AdminUser(_): AdminUser, Path(id): Path<i64>) -> AppResult<Json<Worker>> {
    Ok(Json(fetch(&state, id).await?))
}

#[derive(Deserialize)]
struct UpdateWorker {
    usuario: Option<String>,
    nombre: Option<String>,
    contrasena: Option<String>,
    empresa_id: Option<i64>,
    /// Casilla «Administrador» del perfil.
    administrador: Option<bool>,
}

async fn update(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<i64>, Json(req): Json<UpdateWorker>) -> AppResult<Json<Worker>> {
    let current = fetch(&state, id).await?;
    let now = Utc::now();
    if let Some(make_admin) = req.administrador.filter(|v| *v != current.administrador) {
        if id == admin.id {
            return Err(AppError::bad("No puedes cambiar tu propio permiso de administrador"));
        }
        sqlx::query("UPDATE users SET role = ?, updated_at = ? WHERE id = ?")
            .bind(if make_admin { "admin" } else { "worker" })
            .bind(now)
            .bind(id)
            .execute(&state.pool)
            .await?;
        let action = if make_admin { "administrador_concedido" } else { "administrador_retirado" };
        crate::db::audit(&state.pool, Some(admin.id), action, None, format!("id {id}")).await;
    }
    if let Some(u) = req.usuario.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        validate_username(u)?;
        sqlx::query("UPDATE users SET username = ?, updated_at = ? WHERE id = ?")
            .bind(u).bind(now).bind(id)
            .execute(&state.pool)
            .await
            .map_err(|e| match AppError::from(e) {
                AppError::Conflict(_) => AppError::Conflict("Ese nombre de usuario ya existe".into()),
                other => other,
            })?;
    }
    if let Some(n) = req.nombre.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        sqlx::query("UPDATE users SET display_name = ?, updated_at = ? WHERE id = ?")
            .bind(n).bind(now).bind(id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(p) = req.contrasena.as_deref().filter(|s| !s.is_empty()) {
        password::validate_new(p).map_err(AppError::BadRequest)?;
        auth::update_password(&state.pool, id, &password::hash(p)?, None, None).await?;
    }
    if let Some(cid) = req.empresa_id {
        sqlx::query("UPDATE users SET company_id = ?, updated_at = ? WHERE id = ?")
            .bind(cid).bind(now).bind(id)
            .execute(&state.pool)
            .await?;
    }
    crate::db::audit(&state.pool, Some(admin.id), "trabajador_modificado", None, format!("id {id}")).await;
    Ok(Json(fetch(&state, id).await?))
}

/// Elimina la cuenta. Los DeCA emitidos se conservan (obligación de 1 año).
async fn remove(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<i64>) -> AppResult<Json<Value>> {
    let w = fetch(&state, id).await?;
    if id == admin.id {
        return Err(AppError::bad("No puedes eliminar tu propia cuenta"));
    }
    sqlx::query("UPDATE users SET active = 0, username = ?, updated_at = ? WHERE id = ?")
        .bind(format!("{}#eliminado#{id}", w.usuario))
        .bind(Utc::now())
        .bind(id)
        .execute(&state.pool)
        .await?;
    auth::delete_user_sessions(&state.pool, id).await?;
    crate::db::audit(&state.pool, Some(admin.id), "trabajador_eliminado", None, w.usuario).await;
    Ok(Json(json!({ "ok": true })))
}

async fn remove_2fa(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(id): Path<i64>) -> AppResult<Json<Value>> {
    fetch(&state, id).await?;
    sqlx::query("UPDATE users SET totp_secret = NULL, totp_enabled = 0, totp_pending_secret = NULL, updated_at = ? WHERE id = ?")
        .bind(Utc::now())
        .bind(id)
        .execute(&state.pool)
        .await?;
    crate::db::audit(&state.pool, Some(admin.id), "2fa_eliminado", None, format!("id {id}")).await;
    Ok(Json(json!({ "ok": true, "mensaje": "Código de verificación eliminado" })))
}

// ---------- Perfil trabajador -> Gestionar preferencias ----------

async fn get_prefs(State(state): State<AppState>, AdminUser(_): AdminUser, Path(id): Path<i64>) -> AppResult<Json<Value>> {
    fetch(&state, id).await?;
    let (admin_worker, company, own) = preferences::admin_layers(&state.pool, id).await?;
    let campos: Vec<Value> = preferences::FIELDS.iter().map(|(k, n)| json!({ "campo": k, "nombre": n })).collect();
    Ok(Json(json!({
        "fijadas": admin_worker,
        "empresa": company,
        "del_trabajador": own,
        "campos": campos,
    })))
}

async fn put_prefs(State(state): State<AppState>, AdminUser(_): AdminUser, Path(id): Path<i64>, Json(input): Json<Prefs>) -> AppResult<Json<Value>> {
    fetch(&state, id).await?;
    let prefs = preferences::validate(input)?;
    sqlx::query("UPDATE users SET prefs_admin = ?, updated_at = ? WHERE id = ?")
        .bind(serde_json::to_string(&prefs)?)
        .bind(Utc::now())
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Json(json!({ "ok": true, "fijadas": prefs })))
}

/// "Enseñar código QR": genera un código de un solo uso (10 min) para que el
/// trabajador inicie sesión escaneándolo con la app.
async fn qr_login(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    let w = fetch(&state, id).await?;
    // Preferentemente la dirección pública; si aún no se conoce, la que usa el admin ahora mismo.
    let server = match public_url::resolve(&state, public_url::AdminRequest::of(&admin, &headers)).await {
        Ok(r) => r.url,
        Err(e) => public_url::request_base(&headers).ok_or(e)?,
    };
    let token = random_token();
    let now = Utc::now();
    let expires = now + Duration::minutes(QR_LOGIN_MINUTES);
    // Solo un QR válido a la vez por trabajador.
    sqlx::query("DELETE FROM qr_logins WHERE user_id = ?").bind(id).execute(&state.pool).await?;
    sqlx::query("INSERT INTO qr_logins (token_hash, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)")
        .bind(hash_token(&token))
        .bind(id)
        .bind(now)
        .bind(expires)
        .execute(&state.pool)
        .await?;
    let payload = json!({ "tipo": "deca-login", "servidor": server, "usuario": w.usuario, "codigo": token });
    let text = payload.to_string();
    crate::db::audit(&state.pool, Some(admin.id), "qr_acceso_generado", None, w.usuario).await;
    Ok(Json(json!({
        "contenido": payload,
        "qr_svg": qr::svg(&text)?,
        "caduca": expires,
        "instrucciones": "El trabajador abre la app, pulsa «Entrar con código QR» y escanea este código."
    })))
}
