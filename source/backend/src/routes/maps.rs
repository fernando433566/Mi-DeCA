//! OpenStreetMap en la app (Ajustes -> Mapas):
//! - Buscar direcciones (Nominatim) para Origen y Destino.
//! - Descargar mapas por regiones para usarlos sin conexión, como en Organic Maps:
//!   catálogo versionado -> preparar región -> descarga reanudable (HTTP Range) -> comprobar SHA-256.
//! - Teselas raster en caché para ver el mapa con conexión.

use axum::{
    extract::{Path, Query, Request, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::auth::{AdminUser, CurrentUser};
use crate::db::settings;
use crate::error::{AppError, AppResult};
use crate::maps::{regions, tiles};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/search", get(search))
        .route("/reverse", get(reverse))
        .route("/tiles/{z}/{x}/{y}", get(tile))
        .route("/catalogo", get(catalog))
        .route("/regiones/{id}/preparar", post(prepare))
        .route("/regiones/{id}", axum::routing::delete(delete_region))
        .route("/files/{version}/{file}", get(download))
}

// ===================== Búsqueda de direcciones =====================

async fn nominatim(state: &AppState, path: &str, params: &[(&str, String)]) -> AppResult<Value> {
    let s = settings::load(&state.pool).await?;
    let resp = state
        .http
        .get(format!("{}/{path}", s.geocoder_url))
        .query(params)
        .header(header::ACCEPT_LANGUAGE, "es")
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("buscador de direcciones no disponible: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(anyhow::anyhow!("Nominatim respondió {}", resp.status())));
    }
    resp.json().await.map_err(|e| AppError::Internal(e.into()))
}

/// Resultado simplificado para mostrar en la lista de sugerencias.
fn simplify(v: &Value) -> Value {
    json!({
        "direccion": v.get("display_name"),
        "lat": v.get("lat").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()),
        "lon": v.get("lon").and_then(|x| x.as_str()).and_then(|x| x.parse::<f64>().ok()),
        "tipo": v.get("type"),
        "detalle": v.get("address"),
    })
}

#[derive(Deserialize)]
struct SearchQ {
    q: String,
}

async fn search(State(state): State<AppState>, _u: CurrentUser, Query(q): Query<SearchQ>) -> AppResult<Json<Value>> {
    if q.q.trim().len() < 3 {
        return Ok(Json(json!([])));
    }
    let v = nominatim(
        &state,
        "search",
        &[
            ("q", q.q.trim().to_string()),
            ("format", "jsonv2".into()),
            ("addressdetails", "1".into()),
            ("countrycodes", "es,pt,fr,ad".into()),
            ("limit", "8".into()),
        ],
    )
    .await?;
    let list: Vec<Value> = v.as_array().map(|a| a.iter().map(simplify).collect()).unwrap_or_default();
    Ok(Json(json!(list)))
}

#[derive(Deserialize)]
struct ReverseQ {
    lat: f64,
    lon: f64,
}

async fn reverse(State(state): State<AppState>, _u: CurrentUser, Query(q): Query<ReverseQ>) -> AppResult<Json<Value>> {
    let v = nominatim(
        &state,
        "reverse",
        &[("lat", q.lat.to_string()), ("lon", q.lon.to_string()), ("format", "jsonv2".into()), ("addressdetails", "1".into())],
    )
    .await?;
    Ok(Json(simplify(&v)))
}

async fn tile(State(state): State<AppState>, _u: CurrentUser, Path((z, x, y)): Path<(u8, u32, String)>) -> AppResult<Response> {
    let y: u32 = y.trim_end_matches(".png").parse().map_err(|_| AppError::bad("Tesela no válida"))?;
    let bytes = tiles::get_tile(&state, z, x, y).await?;
    Ok(([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "public, max-age=604800")], bytes).into_response())
}

// ===================== Mapas sin conexión por regiones =====================

/// Catálogo de regiones agrupadas (como el `countries.json` de Organic Maps).
/// La app compara `version` con la del fichero que tiene guardado para saber si hay actualización.
async fn catalog(State(state): State<AppState>, _u: CurrentUser) -> AppResult<Json<Value>> {
    let packages = regions::packages(&state).await?;
    let s = settings::load(&state.pool).await?;
    let groups: Vec<Value> = regions::GROUPS
        .iter()
        .map(|(gid, gname)| {
            let items: Vec<Value> = regions::REGIONS
                .iter()
                .filter(|r| r.grupo == *gid)
                .map(|r| {
                    let p = packages.iter().find(|p| p.region_id == r.id);
                    let ready = p.filter(|p| p.estado == "lista");
                    json!({
                        "id": r.id,
                        "nombre": r.nombre,
                        "limites": r.bbox(),
                        "estado": p.map(|p| p.estado.as_str()).unwrap_or("no_preparada"),
                        "progreso": p.map(|p| p.progreso).unwrap_or(0.0),
                        "error": p.and_then(|p| p.error.clone()),
                        "version": ready.and_then(|p| p.version.clone()),
                        "zoom_maximo": ready.and_then(|p| p.max_zoom),
                        "tamano": ready.and_then(|p| p.size),
                        "sha256": ready.and_then(|p| p.sha256.clone()),
                        "url": ready.and_then(|p| p.version.as_ref().map(|v| format!("/api/maps/files/{v}/{}.pmtiles", r.id))),
                    })
                })
                .collect();
            json!({ "id": gid, "nombre": gname, "regiones": items })
        })
        .collect();
    Ok(Json(json!({
        "formato": "pmtiles",
        "tipo": "vectorial (Mapbox Vector Tiles, esquema Protomaps basemap)",
        "fuente": "Protomaps",
        "atribucion": "© OpenStreetMap contributors",
        "zoom_maximo": s.maps_max_zoom,
        "grupos": groups,
    })))
}

/// Pide al servidor que prepare (o actualice) una región. Cualquier usuario puede pedirlo.
async fn prepare(State(state): State<AppState>, _u: CurrentUser, Path(id): Path<String>) -> AppResult<Json<Value>> {
    regions::enqueue(&state, &id).await?;
    Ok(Json(json!({ "ok": true, "mensaje": "El mapa se está preparando. Puedes seguir usando la app." })))
}

async fn delete_region(State(state): State<AppState>, AdminUser(_): AdminUser, Path(id): Path<String>) -> AppResult<Json<Value>> {
    if let Some(p) = regions::package(&state, &id).await? {
        if p.estado == "generando" {
            return Err(AppError::Conflict("El mapa se está generando; espera a que termine".into()));
        }
        if let Some(rel) = p.file_rel {
            let _ = tokio::fs::remove_file(state.config.tiles_dir().join(rel)).await;
        }
        sqlx::query("DELETE FROM map_packages WHERE region_id = ?").bind(&id).execute(&state.pool).await?;
    }
    Ok(Json(json!({ "ok": true })))
}

/// Descarga del fichero de una región. Admite `Range` para reanudar descargas
/// interrumpidas (igual que Organic Maps, que descarga por trozos).
async fn download(
    State(state): State<AppState>,
    _u: CurrentUser,
    Path((version, file)): Path<(String, String)>,
    req: Request,
) -> AppResult<Response> {
    let id = file.strip_suffix(".pmtiles").ok_or_else(|| AppError::not_found("Mapa no encontrado"))?;
    let p = regions::package(&state, id)
        .await?
        .filter(|p| p.estado == "lista" && p.version.as_deref() == Some(version.as_str()))
        .ok_or_else(|| AppError::not_found("Esa versión del mapa ya no está disponible. Vuelve a consultar el catálogo"))?;
    let path = state.config.tiles_dir().join(p.file_rel.unwrap_or_default());
    let mut res = ServeFile::new_with_mime(&path, &"application/vnd.pmtiles".parse().unwrap())
        .oneshot(req)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("{e}")))?
        .into_response();
    if let Some(sha) = p.sha256 {
        if let Ok(v) = sha.parse() {
            res.headers_mut().insert("x-sha256", v);
        }
    }
    Ok(res)
}
