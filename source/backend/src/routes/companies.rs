//! Ajustes -> <Nombre de la empresa> (solo administrador):
//! datos de la empresa, "Preferencias de la empresa", trabajadores y
//! "Administración de DeCA emitidos".

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Europe::Madrid;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::FromRow;

use crate::auth::{password, AdminUser, CurrentUser};
use crate::deca::{DecaRow, DecaView};
use crate::error::{AppError, AppResult};
use crate::preferences::{self, Prefs};
use crate::state::AppState;

pub fn setup_router() -> Router<AppState> {
    Router::new().route("/status", get(setup_status))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(get_one).put(update).delete(delete))
        .route("/{id}/preferences", get(get_prefs).put(put_prefs))
        .route("/{id}/workers", get(list_workers).post(create_worker))
        .route("/{id}/decas", get(list_decas))
}

#[derive(Serialize, FromRow)]
pub struct Company {
    id: i64,
    name: String,
    cif: String,
    domicilio: String,
    telefono: String,
    /// "Nombre transportista": dueño de los vehículos
    titular: String,
    created_at: DateTime<Utc>,
}

/// La app consulta esto tras el primer inicio de sesión del administrador para
/// mostrar el asistente de configuración inicial (crear la empresa).
async fn setup_status(State(state): State<AppState>, user: CurrentUser, headers: axum::http::HeaderMap) -> AppResult<Json<Value>> {
    let (companies,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM companies").fetch_one(&state.pool).await?;
    let public = super::server::public_status(&state, crate::public_url::AdminRequest::of(&user, &headers)).await;
    Ok(Json(json!({
        "necesita_empresa": companies == 0,
        "necesita_direccion_publica": public["url"].is_null(),
        "direccion_publica": public,
        "es_administrador": user.is_admin(),
    })))
}

async fn list(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Vec<Company>>> {
    let rows = if user.is_admin() {
        sqlx::query_as("SELECT * FROM companies ORDER BY name").fetch_all(&state.pool).await?
    } else {
        sqlx::query_as("SELECT * FROM companies WHERE id = ?")
            .bind(user.company_id)
            .fetch_all(&state.pool)
            .await?
    };
    Ok(Json(rows))
}

async fn fetch(state: &AppState, id: i64) -> AppResult<Company> {
    sqlx::query_as("SELECT * FROM companies WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("Empresa no encontrada"))
}

async fn get_one(State(state): State<AppState>, user: CurrentUser, Path(id): Path<i64>) -> AppResult<Json<Company>> {
    if !user.is_admin() && user.company_id != Some(id) {
        return Err(AppError::Forbidden);
    }
    Ok(Json(fetch(&state, id).await?))
}

#[derive(Deserialize)]
struct CompanyReq {
    nombre: String,
    #[serde(default)]
    cif: String,
    #[serde(default)]
    domicilio: String,
    #[serde(default)]
    telefono: String,
    /// Nombre transportista: persona o sociedad dueña de los vehículos
    #[serde(default)]
    titular: String,
}

async fn create(State(state): State<AppState>, AdminUser(_): AdminUser, Json(req): Json<CompanyReq>) -> AppResult<Json<Company>> {
    let name = req.nombre.trim();
    if name.is_empty() {
        return Err(AppError::bad("Escribe el nombre de la empresa"));
    }
    let now = Utc::now();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO companies (name, cif, domicilio, telefono, titular, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(name)
    .bind(req.cif.trim().to_uppercase())
    .bind(req.domicilio.trim())
    .bind(req.telefono.trim())
    .bind(req.titular.trim())
    .bind(now)
    .bind(now)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(fetch(&state, id).await?))
}

async fn update(State(state): State<AppState>, AdminUser(_): AdminUser, Path(id): Path<i64>, Json(req): Json<CompanyReq>) -> AppResult<Json<Company>> {
    fetch(&state, id).await?;
    if req.nombre.trim().is_empty() {
        return Err(AppError::bad("Escribe el nombre de la empresa"));
    }
    sqlx::query("UPDATE companies SET name = ?, cif = ?, domicilio = ?, telefono = ?, titular = ?, updated_at = ? WHERE id = ?")
        .bind(req.nombre.trim())
        .bind(req.cif.trim().to_uppercase())
        .bind(req.domicilio.trim())
        .bind(req.telefono.trim())
        .bind(req.titular.trim())
        .bind(Utc::now())
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(Json(fetch(&state, id).await?))
}

async fn delete(State(state): State<AppState>, AdminUser(_): AdminUser, Path(id): Path<i64>) -> AppResult<Json<Value>> {
    let (workers,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE company_id = ? AND active = 1")
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    if workers > 0 {
        return Err(AppError::Conflict("La empresa aún tiene trabajadores. Elimínalos o muévelos antes".into()));
    }
    sqlx::query("DELETE FROM companies WHERE id = ?").bind(id).execute(&state.pool).await?;
    Ok(Json(json!({ "ok": true })))
}

// ---------- Administración de Preferencias -> Preferencias de la empresa ----------

async fn get_prefs(State(state): State<AppState>, AdminUser(_): AdminUser, Path(id): Path<i64>) -> AppResult<Json<Value>> {
    let (p,): (String,) = sqlx::query_as("SELECT preferences FROM companies WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("Empresa no encontrada"))?;
    let campos: Vec<Value> = preferences::FIELDS.iter().map(|(k, n)| json!({ "campo": k, "nombre": n })).collect();
    let visuales: Vec<Value> = preferences::VISUAL.iter().map(|(k, n)| json!({ "campo": k, "nombre": n })).collect();
    Ok(Json(json!({ "fijadas": preferences::parse(&p), "campos": campos, "visuales": visuales })))
}

async fn put_prefs(State(state): State<AppState>, AdminUser(_): AdminUser, Path(id): Path<i64>, Json(input): Json<Prefs>) -> AppResult<Json<Value>> {
    let prefs = preferences::validate(input)?;
    let res = sqlx::query("UPDATE companies SET preferences = ?, updated_at = ? WHERE id = ?")
        .bind(serde_json::to_string(&prefs)?)
        .bind(Utc::now())
        .bind(id)
        .execute(&state.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("Empresa no encontrada"));
    }
    Ok(Json(json!({ "ok": true, "fijadas": prefs })))
}

// ---------- Trabajadores -> Trabajadores en mi empresa ----------

async fn list_workers(State(state): State<AppState>, AdminUser(_): AdminUser, Path(id): Path<i64>) -> AppResult<Json<Vec<super::workers::Worker>>> {
    Ok(Json(super::workers::list_for_company(&state, id).await?))
}

#[derive(Deserialize)]
struct NewWorker {
    usuario: String,
    nombre: Option<String>,
    contrasena: String,
}

/// Trabajadores en mi empresa -> Crear cuenta trabajador
async fn create_worker(State(state): State<AppState>, AdminUser(admin): AdminUser, Path(cid): Path<i64>, Json(req): Json<NewWorker>) -> AppResult<Json<super::workers::Worker>> {
    fetch(&state, cid).await?;
    let username = req.usuario.trim();
    super::workers::validate_username(username)?;
    password::validate_new(&req.contrasena).map_err(AppError::BadRequest)?;
    let display = req.nombre.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or(username);
    let now = Utc::now();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO users (username, display_name, password_hash, role, company_id, created_at, updated_at)
         VALUES (?, ?, ?, 'worker', ?, ?, ?) RETURNING id",
    )
    .bind(username)
    .bind(display)
    .bind(password::hash(&req.contrasena)?)
    .bind(cid)
    .bind(now)
    .bind(now)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| match AppError::from(e) {
        AppError::Conflict(_) => AppError::Conflict("Ese nombre de usuario ya existe".into()),
        other => other,
    })?;
    crate::db::audit(&state.pool, Some(admin.id), "trabajador_creado", None, username).await;
    Ok(Json(super::workers::fetch(&state, id).await?))
}

// ---------- Administración de trabajadores -> Administración de DeCA emitidos ----------

#[derive(Deserialize)]
struct DecaFilter {
    /// Id del trabajador (opcional)
    trabajador: Option<i64>,
    /// AAAA-MM-DD
    desde: Option<NaiveDate>,
    hasta: Option<NaiveDate>,
    matricula: Option<String>,
    pagina: Option<i64>,
    por_pagina: Option<i64>,
}

fn day_start(d: NaiveDate) -> DateTime<Utc> {
    Madrid
        .from_local_datetime(&d.and_time(NaiveTime::MIN))
        .earliest()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(|| d.and_time(NaiveTime::MIN).and_utc())
}

async fn list_decas(State(state): State<AppState>, AdminUser(_): AdminUser, Path(cid): Path<i64>, Query(f): Query<DecaFilter>) -> AppResult<Json<Value>> {
    let per_page = f.por_pagina.unwrap_or(50).clamp(1, 200);
    let page = f.pagina.unwrap_or(1).max(1);
    let from = f.desde.map(day_start);
    let to = f.hasta.map(|d| day_start(d + chrono::Duration::days(1)));
    let plate = f.matricula.map(|m| crate::deca::model::normalize_plate(&m)).unwrap_or_default();

    let where_sql = "WHERE (company_id = ?1 OR user_id IN (SELECT id FROM users WHERE company_id = ?1))
          AND (?2 IS NULL OR user_id = ?2) AND (?3 IS NULL OR created_at >= ?3) AND (?4 IS NULL OR created_at < ?4)
          AND (?5 = '' OR matricula_camion = ?5)";
    let total: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM decas {where_sql}"))
        .bind(cid).bind(f.trabajador).bind(from).bind(to).bind(&plate)
        .fetch_one(&state.pool)
        .await?;
    let rows: Vec<DecaRow> = sqlx::query_as(&format!("SELECT * FROM decas {where_sql} ORDER BY created_at DESC, id DESC LIMIT ?6 OFFSET ?7"))
        .bind(cid).bind(f.trabajador).bind(from).bind(to).bind(&plate)
        .bind(per_page)
        .bind((page - 1) * per_page)
        .fetch_all(&state.pool)
        .await?;
    let items: Vec<DecaView> = rows.iter().map(|r| r.view()).collect::<AppResult<_>>()?;
    Ok(Json(json!({ "total": total, "pagina": page, "por_pagina": per_page, "decas": items })))
}
