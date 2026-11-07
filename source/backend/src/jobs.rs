//! Tareas programadas.
//!
//! Eliminación automática: cada día a las 12:00 (hora peninsular) se borran los
//! ficheros .pdf (nunca carpetas) con más de 366 días de antigüedad, si está activado
//! en Ajustes -> Servidor -> Permitir eliminación automática. La normativa obliga a
//! conservar los DeCA al menos 1 año, por eso el umbral es 366 días.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{Duration as ChronoDuration, NaiveTime, TimeZone, Utc};
use chrono_tz::Europe::Madrid;

use crate::db::{self, settings};
use crate::state::AppState;

pub const RETENTION_DAYS: u64 = 366;
const RUN_AT: NaiveTime = match NaiveTime::from_hms_opt(12, 0, 0) {
    Some(t) => t,
    None => unreachable!(),
};

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(until_next_run()).await;
            match settings::load(&state.pool).await {
                Ok(s) if s.auto_delete_enabled => match cleanup(&state).await {
                    Ok(n) => tracing::info!("eliminación automática: {n} PDF eliminados"),
                    Err(e) => tracing::error!("eliminación automática fallida: {e:#}"),
                },
                Ok(_) => tracing::info!("eliminación automática desactivada; no se borra nada"),
                Err(e) => tracing::error!("no se pudieron leer los ajustes: {e}"),
            }
            // Evita ejecutar dos veces en el mismo minuto.
            tokio::time::sleep(Duration::from_secs(61)).await;
        }
    });
}

fn until_next_run() -> Duration {
    let now = Utc::now().with_timezone(&Madrid);
    let mut day = now.date_naive();
    if now.time() >= RUN_AT {
        day += ChronoDuration::days(1);
    }
    let next = Madrid
        .from_local_datetime(&day.and_time(RUN_AT))
        .earliest()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(|| Utc::now() + ChronoDuration::hours(24));
    (next - Utc::now()).to_std().unwrap_or(Duration::from_secs(60))
}

/// Antigüedad del fichero según su fecha de creación (o modificación si el
/// sistema de ficheros no guarda la de creación).
fn file_age(path: &Path) -> Option<Duration> {
    let meta = std::fs::metadata(path).ok()?;
    let born = meta.created().or_else(|_| meta.modified()).ok()?;
    SystemTime::now().duration_since(born).ok()
}

fn old_pdfs(dir: &Path, max_age: Duration, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            old_pdfs(&p, max_age, out);
        } else if p.extension().is_some_and(|e| e == "pdf") && file_age(&p).is_some_and(|a| a > max_age) {
            out.push(p);
        }
    }
}

/// Ejecuta la limpieza ahora. Devuelve cuántos PDF se borraron.
pub async fn cleanup(state: &AppState) -> anyhow::Result<usize> {
    let deca_dir = state.config.deca_dir();
    let max_age = Duration::from_secs(RETENTION_DAYS * 24 * 3600);
    let dir = deca_dir.clone();
    let files = tokio::task::spawn_blocking(move || {
        let mut v = Vec::new();
        old_pdfs(&dir, max_age, &mut v);
        v
    })
    .await?;

    let mut deleted = 0;
    for f in files {
        if let Err(e) = tokio::fs::remove_file(&f).await {
            tracing::warn!("no se pudo borrar {}: {e}", f.display());
            continue;
        }
        deleted += 1;
        let rel = f.strip_prefix(&deca_dir).unwrap_or(&f).to_string_lossy().replace('\\', "/");
        sqlx::query("UPDATE decas SET pdf_deleted = 1 WHERE file_rel_path = ?")
            .bind(&rel)
            .execute(&state.pool)
            .await?;
    }
    if deleted > 0 {
        db::audit(&state.pool, None, "eliminacion_automatica", None, format!("{deleted} PDF eliminados")).await;
    }
    Ok(deleted)
}
