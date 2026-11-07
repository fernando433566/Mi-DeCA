//! Ajustes del servidor (Ajustes -> Servidor).

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::error::AppResult;

const KEY: &str = "server";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerSettings {
    /// Ajustes -> Servidor -> Permitir eliminación automática
    /// (borra cada día a las 12:00 los PDF con más de 366 días).
    pub auto_delete_enabled: bool,
    /// URL pública HTTPS escrita a mano. Normalmente no hace falta: se detecta sola
    /// a través de Cloudflare Tunnel (ver `public_url.rs`).
    pub public_base_url: Option<String>,
    /// Dirección pública aprendida automáticamente de Cloudflare Tunnel (y confirmada).
    pub public_base_url_detectada: Option<String>,
    /// Días naturales que el enlace público sigue activo tras el fin del servicio.
    pub public_link_days: i64,
    /// Servidor de teselas de mapa (formato `{z}/{x}/{y}`).
    pub tile_server_url: String,
    /// Servidor de búsqueda de direcciones (Nominatim).
    pub geocoder_url: String,
    /// Lista de versiones del mapa mundial vectorial (Protomaps, datos de OpenStreetMap).
    pub maps_builds_url: String,
    /// Dónde están los ficheros `<versión>.pmtiles` del mapa mundial.
    pub maps_source_url: String,
    /// Nivel de detalle máximo de los mapas descargables (15 = calles y números).
    pub maps_max_zoom: u8,
    /// Regenerar las regiones descargadas cuando tengan más de estos días (0 = nunca).
    pub maps_auto_update_days: i64,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            auto_delete_enabled: true,
            public_base_url: None,
            public_base_url_detectada: None,
            public_link_days: 7,
            tile_server_url: "https://tile.openstreetmap.org/{z}/{x}/{y}.png".into(),
            geocoder_url: "https://nominatim.openstreetmap.org".into(),
            maps_builds_url: "https://build-metadata.protomaps.dev/builds.json".into(),
            maps_source_url: "https://build.protomaps.com".into(),
            maps_max_zoom: 15,
            maps_auto_update_days: 30,
        }
    }
}

pub async fn load(pool: &SqlitePool) -> AppResult<ServerSettings> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
        .bind(KEY)
        .fetch_optional(pool)
        .await?;
    Ok(match row {
        Some((v,)) => serde_json::from_str(&v).unwrap_or_default(),
        None => ServerSettings::default(),
    })
}

pub async fn save(pool: &SqlitePool, s: &ServerSettings) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(KEY)
    .bind(serde_json::to_string(s)?)
    .execute(pool)
    .await?;
    Ok(())
}
