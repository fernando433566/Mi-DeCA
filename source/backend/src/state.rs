//! Estado compartido entre todas las peticiones.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sqlx::SqlitePool;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<Config>,
    /// Cliente HTTP para OpenStreetMap (búsqueda de direcciones y mapas).
    pub http: reqwest::Client,
    pub login_limiter: Arc<LoginLimiter>,
    /// Cola de regiones de mapa pendientes de generar.
    pub map_queue: tokio::sync::mpsc::UnboundedSender<String>,
    /// Comprobación de la dirección pública (HTTPS a través del túnel).
    pub public_url: Arc<crate::public_url::Checker>,
    /// Última versión de la app Android encontrada en el origen de actualizaciones.
    pub app_update: Arc<crate::app_update::Cache>,
}

/// Limita los intentos de inicio de sesión fallidos por usuario
/// (importante porque la cuenta inicial es "Admin" / "1234").
#[derive(Default)]
pub struct LoginLimiter {
    attempts: Mutex<HashMap<String, (u32, Instant)>>,
}

impl LoginLimiter {
    const MAX_FAILS: u32 = 8;
    const WINDOW: Duration = Duration::from_secs(10 * 60);

    pub fn is_blocked(&self, key: &str) -> bool {
        let mut map = self.attempts.lock().unwrap();
        map.retain(|_, (_, time)| time.elapsed() < Self::WINDOW);
        matches!(map.get(&key.to_lowercase()), Some((n, t)) if *n >= Self::MAX_FAILS && t.elapsed() < Self::WINDOW)
    }

    pub fn fail(&self, key: &str) {
        let mut map = self.attempts.lock().unwrap();
        map.retain(|_, (_, time)| time.elapsed() < Self::WINDOW);
        let entry = map.entry(key.to_lowercase()).or_insert((0, Instant::now()));
        if entry.1.elapsed() >= Self::WINDOW {
            *entry = (0, Instant::now());
        }
        entry.0 = entry.0.saturating_add(1);
        entry.1 = Instant::now();
    }

    pub fn success(&self, key: &str) {
        self.attempts.lock().unwrap().remove(&key.to_lowercase());
    }
}
