//! Paso de los PDF guardados con la estructura antigua `DeCA/Año/Mes/Día/fichero.pdf`
//! a la actual `DeCA/Matricula/Año/Mes/Día/fichero.pdf` (ver `storage`).
//!
//! Se ejecuta al arrancar y después de restaurar una copia. Es idempotente: solo toca
//! rutas con la forma antigua. La matrícula se toma del propio nombre del fichero
//! (`Trabajador-Matricula-fecha.pdf`), que es la del momento de la emisión.
//! El enlace público no cambia: la tabla guarda la URL aparte de la ruta en disco.

use std::path::Path;

use sqlx::SqlitePool;

use super::storage;

/// Tablas con una columna `file_rel_path` que apunta a un PDF.
const TABLES: &[&str] = &["decas", "deca_versions"];

/// Ruta nueva para una ruta con la estructura antigua, o `None` si ya es la actual.
pub fn new_rel_path(old: &str) -> Option<String> {
    let parts: Vec<&str> = old.split('/').collect();
    let [year, month, day, file] = parts.as_slice() else { return None };
    let is_date = year.len() == 4 && month.len() == 2 && day.len() == 2
        && [year, month, day].iter().all(|p| p.chars().all(|c| c.is_ascii_digit()));
    if !is_date {
        return None;
    }
    let plate = file.split('-').nth(1).unwrap_or("");
    Some(format!("{}/{year}/{month}/{day}/{file}", storage::plate_dir(plate)))
}

/// Mueve los PDF antiguos y actualiza sus rutas. Devuelve cuántos se han movido.
pub async fn migrate(pool: &SqlitePool, deca_dir: &Path) -> anyhow::Result<usize> {
    let mut moved = 0;
    for table in TABLES {
        let rows: Vec<(i64, String)> = sqlx::query_as(&format!("SELECT id, file_rel_path FROM {table}"))
            .fetch_all(pool)
            .await?;
        for (id, old) in rows {
            let Some(new) = new_rel_path(&old) else { continue };
            if move_file(deca_dir, &old, &new).await? {
                moved += 1;
            }
            let updated = sqlx::query(&format!("UPDATE {table} SET file_rel_path = ? WHERE id = ?"))
                .bind(&new)
                .bind(id)
                .execute(pool)
                .await;
            if let Err(e) = updated {
                // Sin la ruta nueva en la base de datos, el fichero vuelve a su sitio.
                let _ = move_file(deca_dir, &new, &old).await;
                return Err(e.into());
            }
        }
    }
    if moved > 0 {
        tracing::info!("{moved} PDF movidos a la estructura DeCA/Matrícula/Año/Mes/Día");
    }
    Ok(moved)
}

/// Mueve un fichero dentro de `deca_dir`. `false` si no existía (por ejemplo, ya borrado
/// por la limpieza automática). `rename` conserva la fecha, que usa esa limpieza.
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

/// Borra las carpetas `Día`, `Mes` y `Año` antiguas que se hayan quedado vacías.
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
    use super::new_rel_path;

    #[test]
    fn old_paths_move_under_the_plate() {
        assert_eq!(
            new_rel_path("2026/11/12/Pepe_Perez-1234BCD-12-11-2026-15-41-29.pdf").as_deref(),
            Some("1234BCD/2026/11/12/Pepe_Perez-1234BCD-12-11-2026-15-41-29.pdf")
        );
        assert_eq!(
            new_rel_path("2026/11/12/Pepe-1234BCD-12-11-2026-15-41-29-v2.pdf").as_deref(),
            Some("1234BCD/2026/11/12/Pepe-1234BCD-12-11-2026-15-41-29-v2.pdf")
        );
        assert_eq!(new_rel_path("1234BCD/2026/11/12/Pepe-1234BCD-x.pdf"), None);
    }
}
