//! Enlace público del QR: `/deca/Año/Mes/Día/<token>/<fichero>.pdf`
//!
//! Requisitos (Resolución DGTCF 5/6/2026, apartado Tercero): al abrir la URL se
//! descarga directamente el PDF, sin usuario, contraseña ni pasos intermedios.
//! Pasados los días configurados tras el fin del servicio el enlace deja de servir
//! el PDF, aunque el fichero se sigue conservando en el servidor.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use chrono::Utc;

use crate::db;
use crate::deca::{storage, DecaRow};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/deca/{y}/{m}/{d}/{token}/{file}", get(download))
        .route("/verificacion/{nonce}", get(verification))
}

/// Responde a la comprobación que el propio servidor lanza contra su dirección
/// pública (ver `public_url.rs`). Solo contesta a códigos que está esperando.
async fn verification(State(state): State<AppState>, Path(nonce): Path<String>) -> Response {
    if state.public_url.is_pending(&nonce) {
        (StatusCode::OK, nonce).into_response()
    } else {
        plain(StatusCode::NOT_FOUND, "No encontrado")
    }
}

fn plain(status: StatusCode, msg: &'static str) -> Response {
    (status, [(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")], msg).into_response()
}

async fn download(
    State(state): State<AppState>,
    Path((y, m, d, token, file)): Path<(String, String, String, String, String)>,
) -> Response {
    let row: Option<DecaRow> = match sqlx::query_as("SELECT * FROM decas WHERE token = ?")
        .bind(&token)
        .fetch_optional(&state.pool)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("error consultando DeCA público: {e}");
            return plain(StatusCode::SERVICE_UNAVAILABLE, "Servicio no disponible temporalmente");
        }
    };
    // El token debe coincidir con la fecha y el nombre del fichero de la URL.
    let expected = format!("/deca/{y}/{m}/{d}/{token}/{file}");
    let Some(row) = row.filter(|r| r.url_path == expected) else {
        return plain(StatusCode::NOT_FOUND, "Documento no encontrado");
    };
    if row.pdf_deleted || row.expires_at <= Utc::now() {
        return plain(StatusCode::GONE, "El enlace público de este DeCA ha caducado");
    }
    match tokio::fs::read(storage::absolute(&state.config.deca_dir(), &row.file_rel_path)).await {
        Ok(bytes) => {
            db::audit(&state.pool, None, "descarga_publica", Some(row.id), row.numero.clone()).await;
            super::decas::pdf_response(bytes, &row.file_name, "inline")
        }
        Err(e) => {
            tracing::error!("falta el PDF {}: {e}", row.file_rel_path);
            plain(StatusCode::NOT_FOUND, "Documento no encontrado")
        }
    }
}
