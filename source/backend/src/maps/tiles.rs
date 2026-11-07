//! Teselas raster de OpenStreetMap para ver el mapa con conexión (se guardan en
//! caché en el servidor) y utilidades de coordenadas de tesela.
//!
//! Los mapas sin conexión por regiones están en [`super::regions`].

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::db::settings;
use crate::error::{AppError, AppResult};
use crate::state::AppState;


/// Coordenadas de tesela (esquema XYZ "slippy map").
pub fn tile_xy(lat: f64, lon: f64, z: u8) -> (u32, u32) {
    let n = 2f64.powi(z as i32);
    let lat = lat.clamp(-85.0511, 85.0511).to_radians();
    let x = ((lon + 180.0) / 360.0 * n).floor().clamp(0.0, n - 1.0);
    let y = ((1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * n)
        .floor()
        .clamp(0.0, n - 1.0);
    (x as u32, y as u32)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BBox {
    pub min_lat: f64,
    pub min_lon: f64,
    pub max_lat: f64,
    pub max_lon: f64,
}

/// Rango de teselas `(x0, x1, y0, y1)` que cubre la zona en un nivel de zoom.
pub fn tile_range(b: &BBox, z: u8) -> (u32, u32, u32, u32) {
    let (x0, y0) = tile_xy(b.max_lat, b.min_lon, z);
    let (x1, y1) = tile_xy(b.min_lat, b.max_lon, z);
    (x0.min(x1), x0.max(x1), y0.min(y1), y0.max(y1))
}

pub fn tile_path(state: &AppState, z: u8, x: u32, y: u32) -> PathBuf {
    state.config.tiles_dir().join(z.to_string()).join(x.to_string()).join(format!("{y}.png"))
}

/// Devuelve una tesela desde la caché o la descarga del servidor configurado.
pub async fn get_tile(state: &AppState, z: u8, x: u32, y: u32) -> AppResult<Vec<u8>> {
    if z > 19 || x >= (1u32 << z) || y >= (1u32 << z) {
        return Err(AppError::bad("Tesela fuera de rango"));
    }
    let path = tile_path(state, z, x, y);
    if let Ok(bytes) = tokio::fs::read(&path).await {
        return Ok(bytes);
    }
    let s = settings::load(&state.pool).await?;
    let url = s
        .tile_server_url
        .replace("{z}", &z.to_string())
        .replace("{x}", &x.to_string())
        .replace("{y}", &y.to_string());
    let resp = state
        .http
        .get(&url)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("mapa no disponible: {e}")))?;
    if !resp.status().is_success() {
        return Err(AppError::not_found("Mapa no disponible para esta zona"));
    }
    let bytes = resp.bytes().await.map_err(|e| AppError::Internal(e.into()))?.to_vec();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, &bytes).await?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn madrid_tile() {
        // Puerta del Sol, zoom 10
        assert_eq!(tile_xy(40.4168, -3.7038, 10), (501, 386));
    }
}
