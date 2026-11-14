//! Pantalla de inicio: "Crear DeCA", "DeCA más reciente" (ticket), "Últimos 7 días",
//! detalle de un DeCA, modificaciones y el widget de Android.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use base64::Engine;
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::deca::{self, model::DecaDatos, qr, storage, DecaRow, DecaView};
use crate::error::{AppError, AppResult};
use crate::public_url;
use axum::http::HeaderMap;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", axum::routing::post(create))
        .route("/plantilla", get(template))
        .route("/ultimo", get(latest))
        .route("/recientes", get(recent))
        .route("/{id}", get(detail).put(modify))
        .route("/{id}/pdf", get(pdf))
        .route("/{id}/qr.svg", get(qr_svg))
        .route("/{id}/qr.png", get(qr_png))
        .route("/{id}/historial", get(history))
}

pub fn widget_router() -> Router<AppState> {
    Router::new().route("/ultimo", get(widget))
}

#[derive(Deserialize)]
struct CreateReq {
    #[serde(flatten)]
    datos: DecaDatos,
    /// Solo administrador: empresa con la que se emite.
    empresa_id: Option<i64>,
    /// Identificador generado por la app (UUID). Si la petición se repite (por ejemplo,
    /// la app Android reintenta al volver la conexión), no se duplican los DeCA.
    client_ref: Option<String>,
}

async fn company_for(state: &AppState, user: &CurrentUser, requested: Option<i64>) -> AppResult<Option<i64>> {
    if !user.is_admin() {
        return match user.company_id {
            Some(c) => Ok(Some(c)),
            None => Err(AppError::bad("Tu cuenta no pertenece a ninguna empresa. Avisa al administrador")),
        };
    }
    // Un administrador puede elegir empresa; si no, emite con la suya.
    if requested.or(user.company_id).is_some() {
        return Ok(requested.or(user.company_id));
    }
    Ok(sqlx::query_scalar("SELECT id FROM companies ORDER BY id LIMIT 1")
        .fetch_optional(&state.pool)
        .await?)
}

/// Emite un DeCA. Devuelve `decas` (lista con el DeCA creado, con URL y QR) y si el
/// enlace público está confirmado (HTTPS de ida y vuelta) o un aviso.
/// La respuesta es una lista por compatibilidad con las apps 0.2.x, que la leen así.
async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    Json(req): Json<CreateReq>,
) -> AppResult<Json<Value>> {
    if let Some(r) = &req.client_ref {
        let valid = (8..=64).contains(&r.len()) && r.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        if !valid {
            return Err(AppError::bad("Identificador de petición no válido"));
        }
        let existing: Vec<DecaRow> = sqlx::query_as("SELECT * FROM decas WHERE grupo_id = ? AND user_id = ? ORDER BY id")
            .bind(r)
            .bind(user.id)
            .fetch_all(&state.pool)
            .await?;
        if !existing.is_empty() {
            let decas: Vec<DecaView> = existing.iter().map(|r| r.view()).collect::<AppResult<_>>()?;
            return Ok(Json(json!({ "decas": decas, "enlace_verificado": true, "aviso": null, "repetido": true })));
        }
    }
    let company = company_for(&state, &user, req.empresa_id).await?;
    let public = public_url::resolve(&state, public_url::AdminRequest::of(&user, &headers)).await?;
    let row = deca::create(&state, &user, company, req.datos, &public.url, req.client_ref.as_deref()).await?;
    // Añade a la base de datos compartida los camiones, CIF, direcciones... nuevos.
    if let Err(e) = super::catalog::remember(&state, user.id, &row.datos()?).await {
        tracing::warn!("no se pudieron guardar los datos en el catálogo: {e}");
    }
    Ok(Json(json!({
        "decas": [row.view()?],
        "enlace_verificado": public.verificada,
        "aviso": public.aviso,
    })))
}

async fn template(State(state): State<AppState>, user: CurrentUser, Query(q): Query<super::me::CompanyQuery>) -> AppResult<Json<Value>> {
    let company = if user.is_admin() { q.empresa_id } else { None };
    let eff = crate::preferences::effective(&state.pool, &user, company).await?;
    Ok(Json(json!({ "plantilla": eff.plantilla, "campos": eff.campos })))
}

/// Admin: todos los DeCA (`None`). Trabajador: solo los suyos.
fn scope(user: &CurrentUser) -> Option<i64> {
    if user.is_admin() { None } else { Some(user.id) }
}

/// El DeCA más reciente (del usuario, o de todos si es administrador).
async fn latest_row(state: &AppState, user: &CurrentUser) -> AppResult<Option<DecaRow>> {
    Ok(sqlx::query_as("SELECT * FROM decas WHERE (?1 IS NULL OR user_id = ?1) ORDER BY created_at DESC, id DESC LIMIT 1")
        .bind(scope(user))
        .fetch_optional(&state.pool)
        .await?)
}

/// "DeCA más reciente" (billete de la pantalla de inicio): lista con 0 o 1 elemento.
/// Es una lista por compatibilidad con las apps 0.2.x.
async fn latest(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Vec<DecaView>>> {
    let row = latest_row(&state, &user).await?;
    Ok(Json(row.iter().map(|r| r.view()).collect::<AppResult<_>>()?))
}

/// "Últimos 7 Días"
async fn recent(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Vec<DecaView>>> {
    let rows: Vec<DecaRow> = sqlx::query_as(
        &format!("SELECT * FROM decas WHERE created_at >= ?1 AND (?2 IS NULL OR user_id = ?2) ORDER BY created_at DESC, id DESC"),
    )
    .bind(Utc::now() - Duration::days(7))
    .bind(scope(&user))
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows.iter().map(|r| r.view()).collect::<AppResult<_>>()?))
}

async fn load(state: &AppState, user: &CurrentUser, id: i64) -> AppResult<DecaRow> {
    let row = deca::get_row(state, id).await?;
    row.check_access(user)?;
    Ok(row)
}

async fn detail(State(state): State<AppState>, user: CurrentUser, Path(id): Path<i64>) -> AppResult<Json<Value>> {
    let row = load(&state, &user, id).await?;
    let versions = deca::versions(&state, id).await?;
    Ok(Json(json!({ "deca": row.view()?, "versiones_anteriores": versions })))
}

#[derive(Deserialize)]
struct ModifyReq {
    datos: DecaDatos,
    motivo: String,
}

/// Modificación durante el servicio: mismo enlace y mismo QR, PDF actualizado.
async fn modify(State(state): State<AppState>, user: CurrentUser, Path(id): Path<i64>, Json(req): Json<ModifyReq>) -> AppResult<Json<DecaView>> {
    let row = deca::modify(&state, &user, id, req.datos, &req.motivo).await?;
    Ok(Json(row.view()?))
}

#[derive(Deserialize)]
struct VersionQuery {
    /// Versión anterior concreta (por defecto la vigente).
    version: Option<i64>,
}

/// Descarga del PDF con sesión. Funciona aunque el enlace público haya caducado
/// (obligación de conservar y poder entregar el original durante 1 año).
async fn pdf(State(state): State<AppState>, user: CurrentUser, Path(id): Path<i64>, Query(q): Query<VersionQuery>) -> AppResult<Response> {
    let row = load(&state, &user, id).await?;
    if row.pdf_deleted {
        return Err(AppError::Gone("El PDF de este DeCA se eliminó al superar el plazo de conservación".into()));
    }
    let (rel, name) = match q.version {
        Some(v) if v != row.version => {
            let rel: String = sqlx::query_scalar("SELECT file_rel_path FROM deca_versions WHERE deca_id = ? AND version = ?")
                .bind(id)
                .bind(v)
                .fetch_optional(&state.pool)
                .await?
                .ok_or_else(|| AppError::not_found("Versión no encontrada"))?;
            let name = rel.rsplit('/').next().unwrap_or("deca.pdf").to_string();
            (rel, name)
        }
        _ => (row.file_rel_path.clone(), row.file_name.clone()),
    };
    let bytes = tokio::fs::read(storage::absolute(&state.config.deca_dir(), &rel))
        .await
        .map_err(|_| AppError::not_found("No se encuentra el fichero PDF en el servidor"))?;
    Ok(pdf_response(bytes, &name, "attachment"))
}

/// `Content-Disposition` con el nombre en UTF-8 (`filename*`, RFC 6266) para que la
/// «º» de «Nº» se conserve, y una versión ASCII para navegadores antiguos.
fn content_disposition(disposition: &str, file_name: &str) -> String {
    let ascii: String = file_name
        .replace('º', "o")
        .chars()
        .map(|c| if c.is_ascii_graphic() && c != '"' && c != '\\' { c } else { '_' })
        .collect();
    format!("{disposition}; filename=\"{ascii}\"; filename*=UTF-8''{}", storage::encode_url_path(file_name))
}

pub fn pdf_response(bytes: Vec<u8>, file_name: &str, disposition: &str) -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/pdf".to_string()),
            (header::CONTENT_DISPOSITION, content_disposition(disposition, file_name)),
            (header::CACHE_CONTROL, "no-store".to_string()),
            (header::HeaderName::from_static("x-robots-tag"), "noindex, nofollow".to_string()),
        ],
        Body::from(bytes),
    )
        .into_response()
}

async fn qr_svg(State(state): State<AppState>, user: CurrentUser, Path(id): Path<i64>) -> AppResult<Response> {
    let row = load(&state, &user, id).await?;
    let svg = qr::svg(&row.public_url)?;
    Ok(([(header::CONTENT_TYPE, "image/svg+xml")], svg).into_response())
}

#[derive(Deserialize)]
struct PngQuery {
    /// Píxeles por módulo (por defecto 10)
    escala: Option<usize>,
}

/// QR suelto como imagen (opcional según la normativa, útil para enseñar solo el QR).
async fn qr_png(State(state): State<AppState>, user: CurrentUser, Path(id): Path<i64>, Query(q): Query<PngQuery>) -> AppResult<Response> {
    let row = load(&state, &user, id).await?;
    let png = qr::png(&row.public_url, q.escala.unwrap_or(10))?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png".to_string()),
            (header::CONTENT_DISPOSITION, format!("inline; filename=\"QR-{}.png\"", row.numero)),
        ],
        png,
    )
        .into_response())
}

/// Registro con fecha y hora de creación, modificaciones y descargas en controles.
async fn history(State(state): State<AppState>, user: CurrentUser, Path(id): Path<i64>) -> AppResult<Json<Value>> {
    load(&state, &user, id).await?;
    let rows: Vec<(chrono::DateTime<Utc>, String, String)> =
        sqlx::query_as("SELECT ts, action, detail FROM audit_log WHERE deca_id = ? ORDER BY ts")
            .bind(id)
            .fetch_all(&state.pool)
            .await?;
    let items: Vec<Value> = rows.into_iter().map(|(ts, a, d)| json!({ "fecha": ts, "accion": a, "detalle": d })).collect();
    Ok(Json(json!(items)))
}

/// Widget de Android: el último DeCA con su QR y la información mínima.
/// `viajes` (nombre heredado de las apps 0.2.x) tiene 0 o 1 elemento.
async fn widget(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    let mut viajes = Vec::new();
    if let Some(row) = latest_row(&state, &user).await? {
        let d = row.datos()?;
        let first = d.envios.first();
        let png = qr::png(&row.public_url, 6)?;
        viajes.push(json!({
            "id": row.id,
            "numero": row.numero,
            "fecha_emision": row.created_at,
            "fecha_transporte": d.fecha_transporte,
            "matricula": d.matricula_camion,
            "origen": first.map(|e| e.origen.clone()),
            "destino": first.map(|e| e.destino.clone()),
            "url": row.public_url,
            "url_activa": !row.pdf_deleted && row.expires_at > Utc::now(),
            "qr_png_base64": base64::engine::general_purpose::STANDARD.encode(png),
        }));
    }
    Ok(Json(json!({ "viajes": viajes })))
}
