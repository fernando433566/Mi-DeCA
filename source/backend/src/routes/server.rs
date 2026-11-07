//! Ajustes -> Servidor (solo administrador): dirección pública, eliminación automática,
//! copias de seguridad y restauración.

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;

use crate::auth::AdminUser;
use crate::db::{self, settings};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::{backup, jobs, public_url};
use axum::http::HeaderMap;

/// Tamaño máximo de una copia de seguridad subida (4 GB).
const MAX_UPLOAD: usize = 4 * 1024 * 1024 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/settings", get(get_settings).put(put_settings))
        .route("/status", get(status))
        .route("/backup/config", get(backup_config))
        .route("/backup/full", get(backup_full))
        .route("/restore", post(restore).layer(DefaultBodyLimit::max(MAX_UPLOAD)))
        .route("/cleanup/run", post(run_cleanup))
        .route("/public-url/check", post(check_public_url))
}

async fn get_settings(State(state): State<AppState>, AdminUser(_): AdminUser, headers: HeaderMap) -> AppResult<Json<Value>> {
    let s = settings::load(&state.pool).await?;
    Ok(Json(json!({
        "ajustes": s,
        "direccion_publica_por_defecto": state.config.public_base_url,
        "direccion_publica": public_status(&state, &headers).await,
    })))
}

/// Dirección pública en uso y resultado de la última comprobación HTTPS.
pub async fn public_status(state: &AppState, headers: &HeaderMap) -> Value {
    match public_url::resolve(state, Some(headers)).await {
        Ok(r) => json!({
            "url": r.url,
            "origen": r.origen,
            "verificada": r.verificada,
            "aviso": r.aviso,
            "ultima_comprobacion": state.public_url.last(&r.url),
        }),
        Err(e) => json!({ "url": null, "verificada": false, "aviso": e.to_string() }),
    }
}

/// Ajustes -> Servidor -> Comprobar enlace público (fuerza una comprobación nueva).
async fn check_public_url(State(state): State<AppState>, AdminUser(_): AdminUser, headers: HeaderMap) -> AppResult<Json<Value>> {
    let r = public_url::resolve(&state, Some(&headers)).await?;
    let check = public_url::verify(&state, &r.url).await;
    Ok(Json(json!({ "url": r.url, "origen": r.origen, "comprobacion": check })))
}

#[derive(Deserialize)]
struct SettingsPatch {
    /// Permitir eliminación automática
    auto_delete_enabled: Option<bool>,
    public_base_url: Option<String>,
    public_link_days: Option<i64>,
    tile_server_url: Option<String>,
    geocoder_url: Option<String>,
    maps_builds_url: Option<String>,
    maps_source_url: Option<String>,
    /// Detalle máximo de los mapas sin conexión (10–15)
    maps_max_zoom: Option<u8>,
    /// Actualizar mapas cada N días (0 = nunca)
    maps_auto_update_days: Option<i64>,
}

async fn put_settings(State(state): State<AppState>, AdminUser(admin): AdminUser, Json(p): Json<SettingsPatch>) -> AppResult<Json<settings::ServerSettings>> {
    let mut s = settings::load(&state.pool).await?;
    if let Some(v) = p.auto_delete_enabled {
        s.auto_delete_enabled = v;
    }
    if let Some(url) = p.public_base_url {
        let url = url.trim().trim_end_matches('/').to_string();
        if url.is_empty() {
            s.public_base_url = None;
        } else {
            let https = url.starts_with("https://");
            if !(https || state.config.allow_insecure_public_url && url.starts_with("http://")) || url.contains(char::is_whitespace) {
                return Err(AppError::bad(
                    "La dirección pública debe empezar por https:// (la de Cloudflare Tunnel). \
                     Déjala vacía para que se detecte sola.",
                ));
            }
            s.public_base_url = Some(url);
        }
    }
    if let Some(d) = p.public_link_days {
        if !(7..=366).contains(&d) {
            return Err(AppError::bad("El enlace público debe durar entre 7 y 366 días"));
        }
        s.public_link_days = d;
    }
    if let Some(t) = p.tile_server_url {
        if !(t.starts_with("https://") && t.contains("{z}") && t.contains("{x}") && t.contains("{y}")) {
            return Err(AppError::bad("El servidor de mapas debe ser https:// y contener {z}, {x} e {y}"));
        }
        s.tile_server_url = t;
    }
    if let Some(g) = p.geocoder_url {
        if !g.starts_with("https://") {
            return Err(AppError::bad("El buscador de direcciones debe empezar por https://"));
        }
        s.geocoder_url = g.trim_end_matches('/').to_string();
    }
    for (v, target) in [(p.maps_builds_url, &mut s.maps_builds_url), (p.maps_source_url, &mut s.maps_source_url)] {
        if let Some(v) = v {
            if !v.starts_with("https://") {
                return Err(AppError::bad("Las direcciones de mapas deben empezar por https://"));
            }
            *target = v.trim_end_matches('/').to_string();
        }
    }
    if let Some(z) = p.maps_max_zoom {
        if !(10..=15).contains(&z) {
            return Err(AppError::bad("El detalle del mapa debe estar entre 10 y 15"));
        }
        s.maps_max_zoom = z;
    }
    if let Some(d) = p.maps_auto_update_days {
        if !(0..=365).contains(&d) {
            return Err(AppError::bad("La actualización de mapas debe estar entre 0 (nunca) y 365 días"));
        }
        s.maps_auto_update_days = d;
    }
    settings::save(&state.pool, &s).await?;
    db::audit(&state.pool, Some(admin.id), "ajustes_servidor", None, serde_json::to_string(&s)?).await;
    Ok(Json(s))
}

async fn status(State(state): State<AppState>, AdminUser(_): AdminUser) -> AppResult<Json<Value>> {
    let (total, active, deleted): (i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(expires_at > ? AND pdf_deleted = 0), 0), COALESCE(SUM(pdf_deleted), 0) FROM decas",
    )
    .bind(Utc::now())
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "decas_total": total,
        "decas_con_enlace_activo": active,
        "decas_pdf_eliminado": deleted,
        "retencion_dias": jobs::RETENTION_DAYS,
        "hora_eliminacion_automatica": "12:00 (hora peninsular)",
    })))
}

fn attachment(name: &str, content_type: &str, body: Body) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\"")),
        ],
        body,
    )
        .into_response()
}

/// "Copia de seguridad de configuración" (.json, sin PDF).
async fn backup_config(State(state): State<AppState>, AdminUser(admin): AdminUser) -> AppResult<Response> {
    let data = backup::export_json(&state.pool).await?;
    db::audit(&state.pool, Some(admin.id), "copia_configuracion", None, "").await;
    let name = format!("DeCA-configuracion-{}.json", Utc::now().format("%Y-%m-%d-%H%M"));
    Ok(attachment(&name, "application/json", Body::from(serde_json::to_vec_pretty(&data)?)))
}

/// "Copia de seguridad completa" (.zip con config.json y la carpeta DeCA/).
async fn backup_full(State(state): State<AppState>, AdminUser(admin): AdminUser) -> AppResult<Response> {
    let data = backup::export_json(&state.pool).await?;
    tokio::fs::create_dir_all(state.config.tmp_dir()).await?;
    let tmp = state.config.tmp_dir().join(format!("backup-{}.zip", uuid::Uuid::new_v4()));
    let deca_dir = state.config.deca_dir();
    let dest = tmp.clone();
    tokio::task::spawn_blocking(move || backup::write_full_zip(&dest, &data, &deca_dir))
        .await
        .map_err(|e| AppError::Internal(e.into()))??;
    let file = tokio::fs::File::open(&tmp).await?;
    // En Linux el fichero se puede borrar ya: se sigue leyendo mientras esté abierto.
    let _ = tokio::fs::remove_file(&tmp).await;
    db::audit(&state.pool, Some(admin.id), "copia_completa", None, "").await;
    let name = format!("DeCA-copia-completa-{}.zip", Utc::now().format("%Y-%m-%d-%H%M"));
    Ok(attachment(&name, "application/zip", Body::from_stream(tokio_util::io::ReaderStream::new(file))))
}

/// "Restaurar de una copia de seguridad": acepta el .json o el .zip (campo `archivo`).
/// Sustituye todos los datos; todas las sesiones se cierran y hay que volver a entrar.
async fn restore(State(state): State<AppState>, AdminUser(admin): AdminUser, mut mp: Multipart) -> AppResult<Json<Value>> {
    tokio::fs::create_dir_all(state.config.tmp_dir()).await?;
    let tmp = state.config.tmp_dir().join(format!("restore-{}", uuid::Uuid::new_v4()));
    let mut got_file = false;
    while let Some(mut field) = mp.next_field().await.map_err(|e| AppError::bad(format!("Subida no válida: {e}")))? {
        if field.name() != Some("archivo") {
            continue;
        }
        let mut f = tokio::fs::File::create(&tmp).await?;
        while let Some(chunk) = field.chunk().await.map_err(|e| AppError::bad(format!("Subida interrumpida: {e}")))? {
            f.write_all(&chunk).await?;
        }
        f.flush().await?;
        got_file = true;
        break;
    }
    if !got_file {
        return Err(AppError::bad("Selecciona el fichero de copia de seguridad (.json o .zip)"));
    }

    let result = restore_file(&state, &tmp).await;
    let _ = tokio::fs::remove_file(&tmp).await;
    let full = result?;
    db::audit(&state.pool, Some(admin.id), "restauracion", None, if full { "completa" } else { "configuracion" }).await;
    Ok(Json(json!({
        "ok": true,
        "tipo": if full { "completa" } else { "configuracion" },
        "mensaje": "Copia restaurada. Vuelve a iniciar sesión."
    })))
}

/// Devuelve `true` si era una copia completa (.zip).
async fn restore_file(state: &AppState, path: &std::path::Path) -> AppResult<bool> {
    let mut head = [0u8; 4];
    {
        use tokio::io::AsyncReadExt;
        let mut f = tokio::fs::File::open(path).await?;
        let _ = f.read(&mut head).await?;
    }
    let is_zip = head == *b"PK\x03\x04";
    let config: Value = if is_zip {
        let src = path.to_path_buf();
        let deca_dir = state.config.deca_dir();
        tokio::task::spawn_blocking(move || backup::read_full_zip(&src, &deca_dir))
            .await
            .map_err(|e| AppError::Internal(e.into()))?
            .map_err(|e| AppError::bad(format!("No se pudo leer la copia: {e}")))?
    } else {
        let text = tokio::fs::read_to_string(path)
            .await
            .map_err(|_| AppError::bad("El fichero no es un .json ni un .zip válido"))?;
        serde_json::from_str(&text).map_err(|_| AppError::bad("El fichero .json no es válido"))?
    };
    backup::import_json(&state.pool, &config)
        .await
        .map_err(|e| AppError::bad(format!("No se pudo restaurar: {e}")))?;
    Ok(is_zip)
}

async fn run_cleanup(State(state): State<AppState>, AdminUser(_): AdminUser) -> AppResult<Json<Value>> {
    let n = jobs::cleanup(&state).await?;
    Ok(Json(json!({ "ok": true, "eliminados": n })))
}
