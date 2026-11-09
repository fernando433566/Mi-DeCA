//! Aplicación web instalable (iPhone y navegadores): `/sw.js`.
//!
//! El service worker se genera aquí con la lista de ficheros de la web y una huella de
//! su contenido. Si cambia cualquier fichero cambia también `/sw.js`, y el navegador
//! instala la versión nueva sin ningún paso de compilación.

use std::path::{Path, PathBuf};

use axum::{
    extract::State,
    http::header,
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

const TEMPLATE: &str = include_str!("../pwa/sw.js");

/// Solo lo necesario para que la aplicación funcione (no licencias ni notas).
fn is_app_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("html" | "css" | "js" | "mjs" | "svg" | "png" | "pbf" | "json" | "webmanifest")
    )
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            walk(&path, out)?;
        } else if is_app_file(&path) {
            out.push(path);
        }
    }
    Ok(())
}

/// Genera el service worker para la carpeta `web_dir`.
fn generate(web_dir: &Path) -> anyhow::Result<String> {
    let mut files = Vec::new();
    walk(web_dir, &mut files)?;
    files.sort();

    let mut hash = Sha256::new();
    let mut urls = vec!["/".to_string()];
    for file in &files {
        let rel = file.strip_prefix(web_dir)?.to_string_lossy().replace('\\', "/");
        hash.update(rel.as_bytes());
        hash.update(std::fs::read(file)?);
        urls.push(format!("/{rel}"));
    }
    let version = data_encoding::HEXLOWER.encode(&hash.finalize());
    Ok(TEMPLATE
        .replace("__VERSION__", &version[..16])
        .replace("__FILES__", &serde_json::to_string(&urls)?))
}

pub async fn service_worker(State(state): State<AppState>) -> AppResult<Response> {
    let web_dir = state.config.web_dir.clone();
    let body = tokio::task::spawn_blocking(move || generate(&web_dir))
        .await
        .map_err(|e| AppError::Internal(e.into()))??;
    Ok((
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        body,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_changes_with_content_and_skips_docs() {
        let dir = std::env::temp_dir().join(format!("deca-sw-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("js")).unwrap();
        std::fs::write(dir.join("index.html"), "a").unwrap();
        std::fs::write(dir.join("js/app.js"), "b").unwrap();
        std::fs::write(dir.join("LICENSES.md"), "c").unwrap();

        let first = generate(&dir).unwrap();
        assert!(first.contains(r#"["/","/index.html","/js/app.js"]"#));
        assert!(!first.contains("LICENSES"));

        std::fs::write(dir.join("js/app.js"), "cambio").unwrap();
        assert_ne!(first, generate(&dir).unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
