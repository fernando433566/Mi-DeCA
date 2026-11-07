//! Catálogo de regiones descargables (como el `countries.json` de Organic Maps) y
//! la cola que las genera en el servidor.
//!
//! Cada región se recorta de la versión diaria del mapa mundial de Protomaps
//! (datos de OpenStreetMap) y se guarda en `mapas/regiones/<versión>/<región>.pmtiles`.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use tokio::sync::mpsc;

use super::extract::{self, Remote};
use super::BBox;
use crate::db::{self, settings};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub struct Region {
    pub id: &'static str,
    pub nombre: &'static str,
    pub grupo: &'static str,
    /// min_lon, min_lat, max_lon, max_lat (límites de OpenStreetMap con un pequeño margen)
    pub bbox: [f64; 4],
}

impl Region {
    pub fn bbox(&self) -> BBox {
        let m = 0.02;
        BBox {
            min_lon: self.bbox[0] - m,
            min_lat: self.bbox[1] - m,
            max_lon: self.bbox[2] + m,
            max_lat: self.bbox[3] + m,
        }
    }
}

pub const GROUPS: &[(&str, &str)] = &[("espana", "España"), ("vecinos", "Países vecinos")];

macro_rules! region {
    ($id:literal, $nombre:literal, $grupo:literal, $min_lat:literal, $max_lat:literal, $min_lon:literal, $max_lon:literal) => {
        Region { id: $id, nombre: $nombre, grupo: $grupo, bbox: [$min_lon, $min_lat, $max_lon, $max_lat] }
    };
}

/// Límites obtenidos de OpenStreetMap (Nominatim).
pub static REGIONS: &[Region] = &[
    region!("es-andalucia", "Andalucía", "espana", 35.9376, 38.7291, -7.5227, -1.6298),
    region!("es-aragon", "Aragón", "espana", 39.8468, 42.9245, -2.1737, 0.7713),
    region!("es-asturias", "Asturias", "espana", 42.8825, 43.6665, -7.1832, -4.5106),
    region!("es-baleares", "Illes Balears", "espana", 38.6404, 40.0946, 1.1573, 4.3280),
    region!("es-canarias", "Canarias", "espana", 27.6377, 29.4161, -18.1612, -13.3320),
    region!("es-cantabria", "Cantabria", "espana", 42.7580, 43.5137, -4.8517, -3.1497),
    region!("es-castilla-la-mancha", "Castilla-La Mancha", "espana", 38.0224, 41.3276, -5.4062, -0.9158),
    region!("es-castilla-y-leon", "Castilla y León", "espana", 40.0819, 43.2382, -7.0771, -1.7754),
    region!("es-cataluna", "Cataluña", "espana", 40.5230, 42.8614, 0.1594, 3.3223),
    region!("es-ceuta", "Ceuta", "espana", 35.8709, 35.9180, -5.4235, -5.2783),
    region!("es-valenciana", "Comunitat Valenciana", "espana", 37.8438, 40.7886, -1.5289, 0.6903),
    region!("es-extremadura", "Extremadura", "espana", 37.9410, 40.4867, -7.5417, -4.6476),
    region!("es-galicia", "Galicia", "espana", 41.8074, 43.7904, -9.3015, -6.7340),
    region!("es-la-rioja", "La Rioja", "espana", 41.9190, 42.6443, -3.1343, -1.6787),
    region!("es-madrid", "Comunidad de Madrid", "espana", 39.8846, 41.1657, -4.5790, -3.0530),
    region!("es-melilla", "Melilla", "espana", 35.2656, 35.3201, -2.9702, -2.9231),
    region!("es-murcia", "Región de Murcia", "espana", 37.3739, 38.7551, -2.3444, -0.6479),
    region!("es-navarra", "Navarra", "espana", 41.9099, 43.3149, -2.4999, -0.7233),
    region!("es-pais-vasco", "País Vasco", "espana", 42.4713, 43.4569, -3.4509, -1.7293),
    region!("ad-andorra", "Andorra", "vecinos", 42.4288, 42.6559, 1.4078, 1.7866),
    region!("pt-continental", "Portugal (continental)", "vecinos", 36.9300, 42.1543, -9.5300, -6.1892),
];

pub fn find(id: &str) -> Option<&'static Region> {
    REGIONS.iter().find(|r| r.id == id)
}

/// Estado de una región en el servidor (tabla `map_packages`).
#[derive(Clone, Debug, Serialize, FromRow)]
pub struct Package {
    pub region_id: String,
    /// `en_cola`, `generando`, `lista` o `error`
    pub estado: String,
    /// 0.0 – 1.0
    pub progreso: f64,
    pub version: Option<String>,
    pub max_zoom: Option<i64>,
    pub size: Option<i64>,
    pub sha256: Option<String>,
    pub file_rel: Option<String>,
    pub error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

pub async fn packages(state: &AppState) -> AppResult<Vec<Package>> {
    Ok(sqlx::query_as("SELECT * FROM map_packages").fetch_all(&state.pool).await?)
}

pub async fn package(state: &AppState, id: &str) -> AppResult<Option<Package>> {
    Ok(sqlx::query_as("SELECT * FROM map_packages WHERE region_id = ?").bind(id).fetch_optional(&state.pool).await?)
}

/// Versión más reciente del mapa mundial (p. ej. `20261107`).
pub async fn latest_version(state: &AppState) -> anyhow::Result<String> {
    let s = settings::load(&state.pool).await?;
    let builds: Vec<serde_json::Value> = state
        .http
        .get(&s.maps_builds_url)
        .timeout(Duration::from_secs(30))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    builds
        .iter()
        .filter_map(|b| b.get("key")?.as_str()?.strip_suffix(".pmtiles"))
        .filter(|k| k.len() == 8 && k.chars().all(|c| c.is_ascii_digit()))
        .max()
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("no hay versiones de mapa disponibles"))
}

async fn set_state(state: &AppState, id: &str, estado: &str, progreso: f64, error: Option<&str>) {
    let r = sqlx::query(
        "INSERT INTO map_packages (region_id, estado, progreso, error, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(region_id) DO UPDATE SET estado = ?2, progreso = ?3, error = ?4, updated_at = ?5",
    )
    .bind(id)
    .bind(estado)
    .bind(progreso)
    .bind(error)
    .bind(Utc::now())
    .execute(&state.pool)
    .await;
    if let Err(e) = r {
        tracing::error!("no se pudo actualizar el estado del mapa {id}: {e}");
    }
}

/// Pone una región en la cola de generación.
pub async fn enqueue(state: &AppState, id: &str) -> AppResult<()> {
    find(id).ok_or_else(|| AppError::not_found("Región de mapa no encontrada"))?;
    if let Some(p) = package(state, id).await? {
        if p.estado == "en_cola" || p.estado == "generando" {
            return Ok(());
        }
    }
    set_state(state, id, "en_cola", 0.0, None).await;
    state
        .map_queue
        .send(id.to_string())
        .map_err(|_| AppError::Internal(anyhow::anyhow!("la cola de mapas no está activa")))
}

/// Genera las regiones de una en una (para no saturar el servidor de origen).
pub fn spawn_worker(state: AppState, mut rx: mpsc::UnboundedReceiver<String>) {
    tokio::spawn(async move {
        // Reanuda lo que quedó a medias si el servidor se reinició.
        if let Ok(pending) = sqlx::query_scalar::<_, String>(
            "SELECT region_id FROM map_packages WHERE estado IN ('en_cola', 'generando')",
        )
        .fetch_all(&state.pool)
        .await
        {
            for id in pending {
                let _ = state.map_queue.send(id);
            }
        }
        while let Some(id) = rx.recv().await {
            if let Err(e) = generate(&state, &id).await {
                tracing::error!("error generando el mapa {id}: {e:#}");
                set_state(&state, &id, "error", 0.0, Some(&format!("{e:#}"))).await;
            }
        }
    });
}

async fn generate(state: &AppState, id: &str) -> anyhow::Result<()> {
    let region = find(id).ok_or_else(|| anyhow::anyhow!("región desconocida"))?;
    let s = settings::load(&state.pool).await?;
    let version = latest_version(state).await?;
    let zoom = s.maps_max_zoom;

    let current = package(state, id).await?;
    let rel = format!("regiones/{version}/{id}.pmtiles");
    let dest = state.config.tiles_dir().join(&rel);
    if let Some(p) = &current {
        if p.version.as_deref() == Some(&version) && p.max_zoom == Some(zoom as i64) && dest.exists() {
            set_state(state, id, "lista", 1.0, None).await;
            return Ok(());
        }
    }

    set_state(state, id, "generando", 0.0, None).await;
    tokio::fs::create_dir_all(dest.parent().unwrap()).await?;
    tracing::info!("generando mapa {id} (versión {version}, zoom máx. {zoom})");

    let progress = Arc::new(AtomicU32::new(0));
    let ticker = {
        let (state, id, progress) = (state.clone(), id.to_string(), progress.clone());
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let p = progress.load(Ordering::Relaxed) as f64 / 1000.0;
                let _ = sqlx::query("UPDATE map_packages SET progreso = ? WHERE region_id = ? AND estado = 'generando'")
                    .bind(p)
                    .bind(&id)
                    .execute(&state.pool)
                    .await;
            }
        })
    };
    let remote = Remote { http: state.http.clone(), url: format!("{}/{version}.pmtiles", s.maps_source_url.trim_end_matches('/')) };
    let result = extract::extract(&remote, &region.bbox(), zoom, &dest, &progress).await;
    ticker.abort();
    let result = result?;

    sqlx::query(
        "UPDATE map_packages SET estado = 'lista', progreso = 1, version = ?, max_zoom = ?, size = ?, sha256 = ?,
            file_rel = ?, error = NULL, updated_at = ? WHERE region_id = ?",
    )
    .bind(&version)
    .bind(zoom as i64)
    .bind(result.size as i64)
    .bind(&result.sha256)
    .bind(&rel)
    .bind(Utc::now())
    .bind(id)
    .execute(&state.pool)
    .await?;
    // Borra la versión anterior de la región.
    if let Some(old) = current.and_then(|p| p.file_rel).filter(|old| *old != rel) {
        let _ = tokio::fs::remove_file(state.config.tiles_dir().join(old)).await;
    }
    db::audit(&state.pool, None, "mapa_generado", None, format!("{id} {version} · {} teselas · {} bytes", result.tiles, result.size)).await;
    tracing::info!("mapa {id} listo: {} teselas, {} MB", result.tiles, result.size / 1_000_000);
    Ok(())
}

/// Cada 6 horas comprueba si hay una versión nueva del mapa y regenera las regiones
/// cuya versión tenga más días de los configurados.
pub fn spawn_auto_update(state: AppState) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
            let Ok(s) = settings::load(&state.pool).await else { continue };
            if s.maps_auto_update_days <= 0 {
                continue;
            }
            let Ok(latest) = latest_version(&state).await else { continue };
            let limit = Utc::now() - chrono::Duration::days(s.maps_auto_update_days);
            let Ok(list) = packages(&state).await else { continue };
            for p in list {
                if p.estado == "lista" && p.version.as_deref() != Some(&latest) && p.updated_at < limit {
                    let _ = enqueue(&state, &p.region_id).await;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maps::extract::verify_local;

    /// Prueba real contra Protomaps (requiere Internet): `cargo test -- --ignored ceuta`
    #[tokio::test]
    #[ignore]
    async fn ceuta_real_extract() {
        let http = reqwest::Client::builder().user_agent(crate::maps::USER_AGENT).build().unwrap();
        let builds: Vec<serde_json::Value> = http
            .get("https://build-metadata.protomaps.dev/builds.json")
            .send().await.unwrap().json().await.unwrap();
        let key = builds.last().unwrap()["key"].as_str().unwrap().to_string();
        let remote = Remote { http, url: format!("https://build.protomaps.com/{key}") };
        let dir = std::env::temp_dir().join("deca-ceuta-test");
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("ceuta.pmtiles");
        let progress = AtomicU32::new(0);
        let r = extract::extract(&remote, &find("es-ceuta").unwrap().bbox(), 15, &dest, &progress).await.unwrap();
        let bytes = std::fs::read(&dest).unwrap();
        assert_eq!(verify_local(&bytes).unwrap(), r.tiles);
        println!("Ceuta: {} teselas, {} bytes -> {}", r.tiles, r.size, dest.display());
    }
}
