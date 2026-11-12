//! Copias de seguridad (Ajustes -> Servidor -> Crear copia de seguridad):
//! - "Copia de seguridad de configuración": un .json con todas las tablas
//!   (empresas, usuarios, preferencias, camiones, registros de DeCA...) excepto los PDF.
//! - "Copia de seguridad completa": un .zip con `config.json` + la carpeta `DeCA/`.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use chrono::Utc;
use serde_json::{json, Map, Value};
use sqlx::{Column, Row, SqlitePool, TypeInfo, ValueRef};

/// Tablas incluidas, en orden de dependencias. Las sesiones no se copian.
const TABLES: &[&str] = &[
    "companies", "users", "trailers", "trucks", "parties", "places", "party_places", "goods", "decas",
    "deca_versions", "audit_log", "settings",
];
const FORMAT: &str = "deca-backup";
const CONFIG_NAME: &str = "config.json";

pub async fn export_json(pool: &SqlitePool) -> anyhow::Result<Value> {
    let mut tables = Map::new();
    for t in TABLES {
        let rows = sqlx::query(&format!("SELECT * FROM {t}")).fetch_all(pool).await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let mut obj = Map::new();
            for (i, col) in row.columns().iter().enumerate() {
                let raw = row.try_get_raw(i)?;
                let v = if raw.is_null() {
                    Value::Null
                } else {
                    match raw.type_info().name() {
                        "INTEGER" | "BOOLEAN" => json!(row.try_get::<i64, _>(i)?),
                        "REAL" => json!(row.try_get::<f64, _>(i)?),
                        _ => json!(row.try_get::<String, _>(i)?),
                    }
                };
                obj.insert(col.name().to_string(), v);
            }
            out.push(Value::Object(obj));
        }
        tables.insert(t.to_string(), Value::Array(out));
    }
    Ok(json!({
        "formato": FORMAT,
        "version": 1,
        "creado": Utc::now(),
        "servidor": env!("CARGO_PKG_VERSION"),
        "tablas": tables,
    }))
}

/// Sustituye todo el contenido de la base de datos por el de la copia.
pub async fn import_json(pool: &SqlitePool, backup: &Value) -> anyhow::Result<()> {
    if backup.get("formato").and_then(Value::as_str) != Some(FORMAT) {
        bail!("el fichero no es una copia de seguridad de DeCA");
    }
    let tables = backup.get("tablas").and_then(Value::as_object).context("copia sin tablas")?;
    let mut tx = pool.begin().await?;
    sqlx::query("PRAGMA defer_foreign_keys = ON").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM sessions").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM qr_logins").execute(&mut *tx).await?;
    for t in TABLES.iter().rev() {
        sqlx::query(&format!("DELETE FROM {t}")).execute(&mut *tx).await?;
    }
    for t in TABLES {
        let Some(rows) = tables.get(*t).and_then(Value::as_array) else { continue };
        // Solo se aceptan columnas que existen en el esquema actual (evita inyección SQL).
        let valid: Vec<String> = sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{t}')"))
            .fetch_all(&mut *tx)
            .await?;
        for row in rows {
            let obj = row.as_object().context("fila no válida")?;
            let cols: Vec<&String> = obj.keys().filter(|k| valid.contains(k)).collect();
            if cols.is_empty() {
                continue;
            }
            let names = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
            let marks = vec!["?"; cols.len()].join(", ");
            let sql = format!("INSERT INTO {t} ({names}) VALUES ({marks})");
            let mut q = sqlx::query(&sql);
            for c in cols {
                q = match &obj[c] {
                    Value::Null => q.bind(None::<String>),
                    Value::Bool(b) => q.bind(*b as i64),
                    Value::Number(n) if n.is_i64() => q.bind(n.as_i64()),
                    Value::Number(n) => q.bind(n.as_f64()),
                    Value::String(s) => q.bind(s.clone()),
                    other => q.bind(other.to_string()),
                };
            }
            q.execute(&mut *tx).await.with_context(|| format!("error restaurando la tabla {t}"))?;
        }
    }
    // Sin un administrador activo nadie podría volver a entrar.
    let admins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin' AND active = 1")
        .fetch_one(&mut *tx).await?;
    if admins == 0 {
        bail!("la copia no contiene ningún administrador activo");
    }
    // Los administradores restaurados con la contraseña inicial solo podrán cambiarla.
    crate::db::bootstrap::flag_default_passwords(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let p = entry?.path();
        if p.is_dir() {
            walk_files(&p, out)?;
        } else if p.extension().is_some_and(|e| e == "pdf") {
            out.push(p);
        }
    }
    Ok(())
}

/// Crea el .zip completo en `dest` (operación bloqueante: usar en `spawn_blocking`).
pub fn write_full_zip(dest: &Path, config: &Value, deca_dir: &Path) -> anyhow::Result<()> {
    let file = std::fs::File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .large_file(true);
    zip.start_file(CONFIG_NAME, opts)?;
    zip.write_all(serde_json::to_string_pretty(config)?.as_bytes())?;
    let mut files = Vec::new();
    walk_files(deca_dir, &mut files)?;
    for f in files {
        let rel = f.strip_prefix(deca_dir)?.to_string_lossy().replace('\\', "/");
        zip.start_file(format!("DeCA/{rel}"), opts)?;
        std::io::copy(&mut std::fs::File::open(&f)?, &mut zip)?;
    }
    zip.finish()?;
    Ok(())
}

/// Lee un .zip completo: extrae los PDF en `deca_dir` y devuelve el `config.json`.
/// (Operación bloqueante: usar en `spawn_blocking`.)
pub fn read_full_zip(src: &Path, deca_dir: &Path) -> anyhow::Result<Value> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(src)?).context("el fichero no es un .zip válido")?;
    let mut config = None;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(name) = entry.enclosed_name() else { continue }; // protege contra rutas "../"
        if name == Path::new(CONFIG_NAME) {
            let mut s = String::new();
            entry.read_to_string(&mut s)?;
            config = Some(serde_json::from_str(&s).context("config.json no válido")?);
            continue;
        }
        let Ok(rel) = name.strip_prefix("DeCA") else { continue };
        if entry.is_dir() || rel.extension().is_none_or(|e| e != "pdf") {
            continue;
        }
        let out = deca_dir.join(rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::io::copy(&mut entry, &mut std::fs::File::create(&out)?)?;
    }
    config.context("el .zip no contiene config.json")
}
