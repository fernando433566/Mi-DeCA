//! Rutas HTTP. Cada módulo corresponde a un apartado de la app.
//!
//! | Prefijo                   | Apartado de la app                                         |
//! |---------------------------|------------------------------------------------------------|
//! | `/api/auth`               | Inicio de sesión manual y por QR                           |
//! | `/api/me`                 | Ajustes del usuario: contraseña, 2FA, preferencias         |
//! | `/api/setup`              | Configuración inicial (crear la primera empresa)           |
//! | `/api/companies`          | Ajustes -> <Empresa> (admin)                               |
//! | `/api/workers`            | Ajustes -> <Empresa> -> Trabajadores (admin)               |
//! | `/api/catalog`            | Base de datos compartida (camiones, remolques, CIF...)     |
//! | `/api/decas`, `/api/widget`| Crear DeCA, "DeCA más reciente", "Últimos 7 días"         |
//! | `/api/server`             | Ajustes -> Servidor (admin): copias, eliminación automática|
//! | `/api/maps`               | OpenStreetMap: direcciones y mapas sin conexión            |
//! | `/sw.js`                  | Aplicación instalable (funcionamiento sin conexión)        |
//! | `/deca/...`               | Enlace público del QR (descarga directa, sin sesión)       |

mod app;
mod auth;
mod catalog;
mod companies;
mod decas;
mod maps;
mod me;
mod public;
mod pwa;
mod server;
mod workers;

use axum::http::{header, HeaderValue};
use axum::{routing::get, Json, Router};
use serde_json::json;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    // Aplicación web: ficheros estáticos; cualquier otra ruta devuelve index.html.
    let web = &state.config.web_dir;
    let static_files = tower::ServiceBuilder::new()
        .layer(SetResponseHeaderLayer::overriding(header::CACHE_CONTROL, HeaderValue::from_static("no-cache")))
        .service(ServeDir::new(web).fallback(ServeFile::new(web.join("index.html"))));
    Router::new()
        .route("/api/info", get(info))
        .route("/sw.js", get(pwa::service_worker))
        .route("/health", get(|| async { "ok" }))
        .merge(public::router())
        .nest("/api/auth", auth::router())
        .nest("/api/me", me::router())
        .nest("/api/setup", companies::setup_router())
        .nest("/api/companies", companies::router())
        .nest("/api/workers", workers::router())
        .nest("/api/catalog", catalog::router())
        .nest("/api/decas", decas::router())
        .nest("/api/widget", decas::widget_router())
        .nest("/api/server", server::router())
        .nest("/api/maps", maps::router())
        .nest("/api/app", app::router())
        .fallback_service(static_files)
        .with_state(state)
}

/// La app comprueba con esto que la dirección escrita es un servidor DeCA.
async fn info() -> Json<serde_json::Value> {
    Json(json!({
        "servicio": "DeCA Server",
        "version": env!("CARGO_PKG_VERSION"),
        "descripcion": "Documento electrónico de Control Administrativo"
    }))
}
