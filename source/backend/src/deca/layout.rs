//! Pone los PDF ya guardados con la organización actual (ver `storage::Location`):
//! `DeCA/Matricula/Año/NombreMes/Día/Nº<n>-Trabajador-Matricula-Día-NombreMes-Año-Hora-Minuto-Segundo.pdf`.
//!
//! Sirve para los PDF de versiones anteriores (`DeCA/Año/Mes/Día/…` o
//! `DeCA/Matricula/Año/Mes/Día/…` con otro nombre de fichero). Se ejecuta al arrancar y
//! después de restaurar una copia, y es idempotente: solo mueve los que no están donde
//! deben. Todo sale de la propia fila: número, trabajador y fecha de emisión; la
//! matrícula, del nombre del fichero (la del momento de la emisión).
//! El enlace público no cambia: la tabla guarda la URL aparte de la ruta en disco.

use std::path::Path;

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use super::storage::{self, Location};

/// Matrícula (ya saneada) que aparece en el nombre de un PDF de cualquier versión:
/// `Nº<n>-Trabajador-Matricula-…` (actual) o `Trabajador-Matricula-…` (anterior).
/// Los nombres saneados no contienen `-`, así que basta con contar segmentos.
pub fn plate_from_file_name(file_name: &str) -> &str {
    let index = if file_name.starts_with("Nº") { 2 } else { 1 };
    file_name.split('-').nth(index).unwrap_or("")
}

/// Ruta de una versión anterior archivada de un DeCA modificado.
pub fn version_rel_path(current_rel: &str, version: i64) -> String {
    current_rel.replace(".pdf", &format!("-v{version}.pdf"))
}

/// Mueve los PDF que no siguen la organización actual y actualiza sus rutas.
/// Devuelve cuántos ficheros se han movido.
pub async fn migrate(pool: &SqlitePool, deca_dir: &Path) -> anyhow::Result<usize> {
    let mut moved = 0;
    let rows: Vec<(i64, String, String, String, DateTime<Utc>)> =
        sqlx::query_as("SELECT id, user_name, file_name, file_rel_path, created_at FROM decas WHERE file_rel_path != ''")
            .fetch_all(pool)
            .await?;
    for (id, worker, file_name, old_rel, created) in rows {
        let target = Location::new(id, &worker, plate_from_file_name(&file_name), created);
        let new_rel = target.rel_path();
        if old_rel != new_rel {
            let update = sqlx::query("UPDATE decas SET file_rel_path = ?, file_name = ? WHERE id = ?")
                .bind(&new_rel)
                .bind(&target.file_name)
                .bind(id)
                .execute(pool);
            moved += relocate(deca_dir, &old_rel, &new_rel, update).await?;
        }
        // Versiones anteriores (DeCA modificados): junto al PDF vigente.
        let versions: Vec<(i64, i64, String)> =
            sqlx::query_as("SELECT id, version, file_rel_path FROM deca_versions WHERE deca_id = ?")
                .bind(id)
                .fetch_all(pool)
                .await?;
        for (version_id, version, old_version_rel) in versions {
            let new_version_rel = version_rel_path(&new_rel, version);
            if old_version_rel != new_version_rel {
                let update = sqlx::query("UPDATE deca_versions SET file_rel_path = ? WHERE id = ?")
                    .bind(&new_version_rel)
                    .bind(version_id)
                    .execute(pool);
                moved += relocate(deca_dir, &old_version_rel, &new_version_rel, update).await?;
            }
        }
    }
    if moved > 0 {
        tracing::info!("{moved} PDF reorganizados en DeCA/Matrícula/Año/Mes/Día con el nombre Nº<número>-…");
    }
    Ok(moved)
}

/// Mueve un fichero y después ejecuta `update` (la consulta que guarda su nueva ruta;
/// no se ejecuta hasta aquí). Si la consulta falla, el fichero vuelve a su sitio.
/// Devuelve 1 si había fichero que mover (puede faltar si la limpieza automática ya
/// lo borró; la ruta se actualiza igualmente).
async fn relocate<T>(
    deca_dir: &Path,
    from: &str,
    to: &str,
    update: impl std::future::Future<Output = Result<T, sqlx::Error>>,
) -> anyhow::Result<usize> {
    let had_file = move_file(deca_dir, from, to).await?;
    if let Err(e) = update.await {
        if had_file {
            let _ = move_file(deca_dir, to, from).await;
        }
        return Err(e.into());
    }
    Ok(usize::from(had_file))
}

/// Mueve un fichero dentro de `deca_dir`. `false` si no existía.
/// `rename` conserva la fecha del fichero, que usa la limpieza automática.
async fn move_file(deca_dir: &Path, from: &str, to: &str) -> std::io::Result<bool> {
    let src = storage::absolute(deca_dir, from);
    if !tokio::fs::try_exists(&src).await? {
        return Ok(false);
    }
    let dest = storage::absolute(deca_dir, to);
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::rename(&src, &dest).await?;
    remove_empty_parents(deca_dir, &src).await;
    Ok(true)
}

/// Borra las carpetas antiguas que se hayan quedado vacías.
/// `remove_dir` falla si la carpeta aún tiene algo, así que nunca se pierde nada.
async fn remove_empty_parents(deca_dir: &Path, moved: &Path) {
    let mut dir = moved.parent();
    while let Some(d) = dir.filter(|d| *d != deca_dir && d.starts_with(deca_dir)) {
        if tokio::fs::remove_dir(d).await.is_err() {
            break;
        }
        dir = d.parent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plate_is_found_in_every_name_format() {
        assert_eq!(plate_from_file_name("Pepe_Perez-1234BCD-12-11-2026-15-41-29.pdf"), "1234BCD");
        assert_eq!(plate_from_file_name("Pepe-1234BCD-12-11-2026-15-41-29-v2.pdf"), "1234BCD");
        assert_eq!(plate_from_file_name("Nº7-Pepe-1234BCD-14-Noviembre-2026-13-05-09.pdf"), "1234BCD");
    }

    #[test]
    fn versions_sit_next_to_the_current_pdf() {
        assert_eq!(version_rel_path("A/2026/Mayo/01/Nº3-x.pdf", 2), "A/2026/Mayo/01/Nº3-x-v2.pdf");
    }
}
