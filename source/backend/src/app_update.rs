//! Actualizaciones de la app Android (Ajustes -> Servidor -> Actualizaciones de la app Android).
//!
//! El administrador elige de dónde salen las versiones nuevas (como Obtainium):
//! - las *releases* de GitHub de este proyecto ([`DEFAULT_SOURCE`]), o
//! - otra dirección suya: *releases* de GitHub o de Gitea/Forgejo (p. ej. Codeberg), o un
//!   fichero JSON propio `{ "version": "0.3.1", "apk": "https://…/DeCA.apk", "sha256": "…", "notas": "…" }`.
//!
//! El servidor consulta el origen (como mucho una vez por hora) y los móviles le preguntan
//! a él (`GET /api/app/update`). La app descarga la APK e Android solo la instala si está
//! firmada con la misma clave que la app instalada.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use serde::Serialize;
use serde_json::Value;

/// Origen predeterminado: las releases de GitHub de Mi-DeCA.
pub const DEFAULT_SOURCE: &str = "https://github.com/fernando433566/Mi-DeCA/releases";
/// Cada cuánto se vuelve a consultar el origen.
const CACHE_TTL: Duration = Duration::from_secs(3600);
/// Tamaño máximo de la respuesta del origen (JSON).
const MAX_RESPONSE: usize = 1024 * 1024;

/// De dónde se leen las versiones.
#[derive(Debug, PartialEq)]
pub enum Source {
    /// `https://github.com/<dueño>/<repositorio>[/releases…]`
    GitHub { owner: String, repo: String },
    /// `https://<servidor>/<dueño>/<repositorio>[/releases…]` (Gitea, Forgejo, Codeberg)
    Gitea { base: String, owner: String, repo: String },
    /// `https://…/algo.json`
    Manifest(String),
}

impl Source {
    pub fn parse(url: &str) -> anyhow::Result<Self> {
        let url = url.trim().trim_end_matches('/');
        let Some(rest) = url.strip_prefix("https://") else { bail!("la dirección debe empezar por https://") };
        if url.contains(char::is_whitespace) || rest.is_empty() {
            bail!("dirección no válida");
        }
        if url.ends_with(".json") {
            return Ok(Source::Manifest(url.to_string()));
        }
        let mut parts = rest.split('/');
        let host = parts.next().unwrap_or_default().to_ascii_lowercase();
        let (owner, repo) = match (parts.next(), parts.next()) {
            (Some(o), Some(r)) if valid_name(o) && valid_name(r) => (o.to_string(), r.to_string()),
            _ => bail!("indica la dirección de las releases: https://servidor/dueño/repositorio/releases o un fichero .json"),
        };
        if host == "github.com" || host == "www.github.com" {
            Ok(Source::GitHub { owner, repo })
        } else {
            Ok(Source::Gitea { base: format!("https://{host}"), owner, repo })
        }
    }

    /// Dirección de la API que da la última versión.
    fn api_url(&self) -> String {
        match self {
            Source::GitHub { owner, repo } => format!("https://api.github.com/repos/{owner}/{repo}/releases/latest"),
            Source::Gitea { base, owner, repo } => format!("{base}/api/v1/repos/{owner}/{repo}/releases/latest"),
            Source::Manifest(url) => url.clone(),
        }
    }
}

fn valid_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 100 && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)) && s != "." && s != ".."
}

/// Última versión publicada en el origen.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Release {
    /// Sin la `v` inicial: `0.3.1`
    pub version: String,
    /// Descarga directa de la APK (https)
    pub apk: String,
    /// SHA-256 de la APK en hexadecimal, si el origen lo da
    pub sha256: Option<String>,
    pub notas: Option<String>,
}

/// Lee la respuesta del origen (release de GitHub/Gitea o JSON propio).
pub fn parse_release(source: &Source, v: &Value) -> anyhow::Result<Release> {
    let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty());
    let (version, apk, sha256, notas) = match source {
        Source::Manifest(_) => (
            text("version").context("el JSON no tiene «version»")?,
            text("apk").or_else(|| text("url")).context("el JSON no tiene «apk»")?.to_string(),
            text("sha256").map(str::to_string),
            text("notas"),
        ),
        Source::GitHub { .. } | Source::Gitea { .. } => {
            let assets = v.get("assets").and_then(Value::as_array).context("la release no tiene ficheros")?;
            let asset = assets
                .iter()
                .find(|a| a.get("name").and_then(Value::as_str).is_some_and(|n| n.to_ascii_lowercase().ends_with(".apk")))
                .context("la última release no tiene ninguna APK")?;
            let apk = asset.get("browser_download_url").and_then(Value::as_str).context("APK sin dirección de descarga")?;
            // GitHub da `digest: "sha256:…"` en cada fichero.
            let sha = asset.get("digest").and_then(Value::as_str).and_then(|d| d.strip_prefix("sha256:")).map(str::to_string);
            (text("tag_name").context("la release no tiene versión")?, apk.to_string(), sha, text("body"))
        }
    };
    let version = version_from_tag(version).with_context(|| format!("versión no válida: {version}"))?;
    if !apk.starts_with("https://") {
        bail!("la APK debe descargarse por https://");
    }
    let sha256 = sha256.map(|s| s.to_ascii_lowercase()).filter(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()));
    let notas = notas.map(|n| n.chars().take(2000).collect());
    Ok(Release { version, apk, sha256, notas })
}

/// Versión dentro de una etiqueta: desde el primer número (`v0.3.1`, `version-0.3.1`,
/// `DeCA 0.3.1` → `0.3.1`), como hace Obtainium.
pub fn version_from_tag(tag: &str) -> Option<String> {
    let start = tag.find(|c: char| c.is_ascii_digit())?;
    let version: String = tag[start..].chars().take_while(|c| c.is_ascii_alphanumeric() || ".-+".contains(*c)).collect();
    (version.len() <= 40).then_some(version)
}

/// Última consulta al origen, para no preguntarle en cada petición de los móviles.
#[derive(Default)]
pub struct Cache {
    last: Mutex<Option<(String, Instant, Result<Release, String>)>>,
}

impl Cache {
    /// Última versión del origen `url`. `fresh` obliga a consultarlo ahora (botón «Comprobar»).
    pub async fn latest(&self, http: &reqwest::Client, url: &str, fresh: bool) -> Result<Release, String> {
        if !fresh {
            if let Some((u, at, r)) = self.last.lock().unwrap().as_ref() {
                if u == url && at.elapsed() < CACHE_TTL {
                    return r.clone();
                }
            }
        }
        let result = fetch(http, url).await.map_err(|e| format!("{e:#}"));
        *self.last.lock().unwrap() = Some((url.to_string(), Instant::now(), result.clone()));
        result
    }
}

async fn fetch(http: &reqwest::Client, url: &str) -> anyhow::Result<Release> {
    let source = Source::parse(url)?;
    let mut res = http
        .get(source.api_url())
        .header(reqwest::header::ACCEPT, "application/json")
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .context("no se pudo conectar con el origen de actualizaciones")?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        bail!("el origen no tiene ninguna versión publicada");
    }
    if !res.status().is_success() {
        bail!("el origen respondió con el código {}", res.status().as_u16());
    }
    let mut body = Vec::new();
    while let Some(chunk) = res.chunk().await? {
        if body.len() + chunk.len() > MAX_RESPONSE {
            bail!("la respuesta del origen es demasiado grande");
        }
        body.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&body).context("el origen no devolvió JSON")?;
    parse_release(&source, &value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sources_are_recognised() {
        assert_eq!(
            Source::parse(DEFAULT_SOURCE).unwrap(),
            Source::GitHub { owner: "fernando433566".into(), repo: "Mi-DeCA".into() }
        );
        assert_eq!(
            Source::parse("https://codeberg.org/ana/deca/releases").unwrap().api_url(),
            "https://codeberg.org/api/v1/repos/ana/deca/releases/latest"
        );
        assert_eq!(Source::parse("https://x.es/deca.json").unwrap(), Source::Manifest("https://x.es/deca.json".into()));
        assert!(Source::parse("http://github.com/a/b").is_err());
        assert!(Source::parse("https://github.com/solo-dueno").is_err());
    }

    #[test]
    fn github_release_gives_apk_and_digest() {
        let source = Source::parse(DEFAULT_SOURCE).unwrap();
        let sha = "ab".repeat(32);
        let release = json!({
            "tag_name": "v0.3.1", "body": "Cambios",
            "assets": [
                { "name": "deca-server.tar.gz", "browser_download_url": "https://github.com/x/server.tar.gz" },
                { "name": "DeCA-0.3.1.apk", "browser_download_url": "https://github.com/x/DeCA-0.3.1.apk", "digest": format!("sha256:{sha}") }
            ]
        });
        let r = parse_release(&source, &release).unwrap();
        assert_eq!(r, Release { version: "0.3.1".into(), apk: "https://github.com/x/DeCA-0.3.1.apk".into(), sha256: Some(sha), notas: Some("Cambios".into()) });
        assert!(parse_release(&source, &json!({ "tag_name": "v1", "assets": [] })).is_err());
    }

    #[test]
    fn versions_are_read_from_any_tag() {
        assert_eq!(version_from_tag("v0.3.1").as_deref(), Some("0.3.1"));
        assert_eq!(version_from_tag("version-0.2.1").as_deref(), Some("0.2.1"));
        assert_eq!(version_from_tag("DeCA 1.0 final").as_deref(), Some("1.0"));
        assert_eq!(version_from_tag("ultima"), None);
    }

    #[test]
    fn manifest_needs_https_apk() {
        let source = Source::Manifest("https://x.es/deca.json".into());
        assert_eq!(parse_release(&source, &json!({ "version": "0.4.0", "apk": "https://x.es/a.apk" })).unwrap().version, "0.4.0");
        assert!(parse_release(&source, &json!({ "version": "0.4.0", "apk": "http://x.es/a.apk" })).is_err());
    }
}
