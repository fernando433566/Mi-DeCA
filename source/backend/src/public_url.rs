//! Dirección pública del servidor (la que va dentro del QR de cada DeCA).
//!
//! Prioridad: dirección escrita en Ajustes -> Servidor > `DECA_PUBLIC_URL` >
//! dirección detectada por Cloudflare Tunnel.
//!
//! Cloudflare Tunnel reenvía las peticiones `https://` al puerto 8134 con las cabeceras
//! `X-Forwarded-Proto: https`, `Cf-Visitor` y el `Host` público. Si no hay dirección
//! configurada, el servidor la aprende de ellas, pero **solo de peticiones de un
//! administrador** ([`AdminRequest`]): esas cabeceras las puede escribir cualquier
//! cliente, y un trabajador no debe poder cambiar el enlace que llevan los QR de todos.
//! Antes de guardarla se comprueba de ida y vuelta (`/verificacion/<código>`), lo que
//! confirma que el enlace funciona por HTTPS (diagnóstico de conectividad).

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Duration;

use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::auth::random_token;
use crate::db::settings;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Una comprobación correcta se da por buena durante este tiempo.
const CHECK_TTL_MINUTES: i64 = 60;

#[derive(Default)]
pub struct Checker {
    pending: Mutex<HashSet<String>>,
    results: Mutex<HashMap<String, CheckStatus>>,
}

impl Checker {
    /// ¿Es un código de comprobación que estamos esperando?
    pub fn is_pending(&self, nonce: &str) -> bool {
        self.pending.lock().unwrap().contains(nonce)
    }

    pub fn last(&self, url: &str) -> Option<CheckStatus> {
        self.results.lock().unwrap().get(url).cloned()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct CheckStatus {
    pub url: String,
    /// La comprobación de ida y vuelta por HTTPS ha funcionado.
    pub ok: bool,
    pub fecha: DateTime<Utc>,
    pub detalle: String,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Origen {
    /// Escrita por el administrador en Ajustes -> Servidor
    Manual,
    /// Variable de entorno `DECA_PUBLIC_URL`
    Entorno,
    /// Aprendida de una petición de administrador llegada por Cloudflare Tunnel
    Detectada,
}

/// Cabeceras de una petición hecha por un administrador: las únicas de las que
/// el servidor puede aprender su dirección pública.
pub struct AdminRequest<'a>(&'a HeaderMap);

impl<'a> AdminRequest<'a> {
    pub fn of(user: &crate::auth::CurrentUser, headers: &'a HeaderMap) -> Option<Self> {
        user.is_admin().then_some(Self(headers))
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Resolved {
    pub url: String,
    pub origen: Origen,
    pub verificada: bool,
    pub aviso: Option<String>,
}

fn header<'a>(h: &'a HeaderMap, name: &str) -> Option<&'a str> {
    h.get(name)?.to_str().ok().map(str::trim).filter(|s| !s.is_empty())
}

/// Esquema con el que llegó la petición original (antes del túnel).
fn forwarded_scheme(h: &HeaderMap) -> Option<String> {
    if let Some(p) = header(h, "x-forwarded-proto") {
        return Some(p.split(',').next().unwrap_or(p).trim().to_ascii_lowercase());
    }
    let visitor = header(h, "cf-visitor")?;
    let v: serde_json::Value = serde_json::from_str(visitor).ok()?;
    v.get("scheme")?.as_str().map(str::to_ascii_lowercase)
}

fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'))
}

fn forwarded_host(h: &HeaderMap) -> Option<String> {
    let host = header(h, "x-forwarded-host").or_else(|| header(h, "host"))?;
    let host = host.split(',').next()?.trim().to_ascii_lowercase();
    valid_host(&host).then_some(host)
}

/// Dirección `https://` deducida de una petición que llegó por el túnel.
pub fn https_from_headers(h: &HeaderMap) -> Option<String> {
    (forwarded_scheme(h)? == "https").then(|| format!("https://{}", forwarded_host(h).unwrap_or_default()))
        .filter(|u| u.len() > "https://".len())
}

/// Dirección con la que el cliente ha llegado (cualquier esquema), p. ej. la de la red local.
pub fn request_base(h: &HeaderMap) -> Option<String> {
    let scheme = forwarded_scheme(h).filter(|s| s == "https" || s == "http").unwrap_or_else(|| "http".into());
    Some(format!("{scheme}://{}", forwarded_host(h)?))
}

/// Comprueba de ida y vuelta que `url` llega a este servidor por HTTPS.
pub async fn verify(state: &AppState, url: &str) -> CheckStatus {
    let status = |ok: bool, detalle: String| CheckStatus { url: url.to_string(), ok, fecha: Utc::now(), detalle };
    let result = if !url.starts_with("https://") {
        status(false, "la dirección no empieza por https://".into())
    } else {
        let nonce = random_token();
        state.public_url.pending.lock().unwrap().insert(nonce.clone());
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15)).build();
        let res = match client {
            Ok(client) => client.get(format!("{url}/verificacion/{nonce}"))
            .timeout(Duration::from_secs(15))
            .send().await,
            Err(e) => Err(e),
        };
        state.public_url.pending.lock().unwrap().remove(&nonce);
        match res {
            Ok(mut r) if r.status().is_success() => {
                let mut body = Vec::new();
                let mut valid = true;
                loop {
                    match r.chunk().await {
                        Ok(Some(chunk)) if body.len() + chunk.len() <= 128 => body.extend_from_slice(&chunk),
                        Ok(None) => break,
                        _ => { valid = false; break; }
                    }
                }
                if valid && body == nonce.as_bytes() {
                    status(true, "El enlace responde al diagnóstico HTTPS".into())
                } else { status(false, "la dirección no responde al diagnóstico esperado".into()) }
            },
            Ok(r) => status(false, format!("la dirección respondió con el código {}", r.status().as_u16())),
            Err(e) => {
                tracing::warn!("comprobación de {url} fallida: {e}");
                status(false, "no se pudo conectar con esa dirección".into())
            }
        }
    };
    let mut results = state.public_url.results.lock().unwrap();
    if results.len() >= 16 && !results.contains_key(url) { results.clear(); }
    results.insert(url.to_string(), result.clone());
    result
}

/// Usa la última comprobación si es reciente; si no, comprueba ahora.
async fn verified(state: &AppState, url: &str) -> CheckStatus {
    match state.public_url.last(url) {
        Some(s) if s.ok && Utc::now() - s.fecha < chrono::Duration::minutes(CHECK_TTL_MINUTES) => s,
        _ => verify(state, url).await,
    }
}

fn clean(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

/// Aprende una dirección nueva de la petición de un administrador, si se confirma.
async fn learn(state: &AppState, mut s: settings::ServerSettings, from: Option<AdminRequest<'_>>) -> AppResult<Option<Resolved>> {
    let Some(candidate) = from.and_then(|r| https_from_headers(r.0)) else { return Ok(None) };
    if s.public_base_url_detectada.as_deref() == Some(candidate.as_str()) || !verified(state, &candidate).await.ok {
        return Ok(None);
    }
    s.public_base_url_detectada = Some(candidate.clone());
    settings::save(&state.pool, &s).await?;
    tracing::info!("dirección pública detectada y confirmada: {candidate}");
    Ok(Some(Resolved { url: candidate, origen: Origen::Detectada, verificada: true, aviso: None }))
}

/// Calcula la dirección pública que se usará en los QR.
///
/// Prioridad: manual > variable de entorno > detectada (confirmada) > última detectada guardada.
/// `learn_from` solo existe en peticiones de administrador (ver [`AdminRequest`]).
pub async fn resolve(state: &AppState, learn_from: Option<AdminRequest<'_>>) -> AppResult<Resolved> {
    let s = settings::load(&state.pool).await?;
    let configured = s
        .public_base_url
        .as_deref()
        .map(|u| (clean(u), Origen::Manual))
        .or_else(|| state.config.public_base_url.as_deref().map(|u| (clean(u), Origen::Entorno)));
    let (url, origen) = match configured {
        Some(c) => c,
        None => {
            let stored = s.public_base_url_detectada.clone();
            if let Some(found) = learn(state, s, learn_from).await? {
                return Ok(found);
            }
            let stored = stored.ok_or_else(|| {
                AppError::bad(
                    "Todavía no se conoce la dirección pública del servidor. El administrador debe entrar una vez \
                     usando la dirección de Cloudflare Tunnel (https://...) o escribirla en Ajustes -> Servidor.",
                )
            })?;
            (stored, Origen::Detectada)
        }
    };

    if !url.starts_with("https://") {
        if state.config.allow_insecure_public_url {
            return Ok(Resolved {
                url,
                origen,
                verificada: false,
                aviso: Some("Modo de pruebas: el enlace no es HTTPS y no serviría en un control.".into()),
            });
        }
        return Err(AppError::bad(format!(
            "La dirección pública {url} no es https://. Con ella el QR no serviría en un control. \
             Cámbiala en Ajustes -> Servidor por la dirección https:// de Cloudflare Tunnel."
        )));
    }

    let check = verified(state, &url).await;
    Ok(Resolved {
        aviso: (!check.ok).then(|| {
            format!(
                "No se ha podido confirmar que el enlace {url} funcione ({}). El DeCA se ha creado; \
                 comprueba el QR con otro móvil antes de salir.",
                check.detalle
            )
        }),
        url,
        origen,
        verificada: check.ok,
    })
}

/// Comprueba la dirección pública al arrancar y cada 30 minutos.
pub fn spawn_periodic_check(state: AppState) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        loop {
            if let Ok(r) = resolve(&state, None).await {
                if let Some(a) = r.aviso {
                    tracing::warn!("{a}");
                }
            }
            tokio::time::sleep(Duration::from_secs(30 * 60)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut m = HeaderMap::new();
        for (k, v) in pairs {
            m.insert(*k, v.parse().unwrap());
        }
        m
    }

    #[test]
    fn detects_cloudflare_tunnel() {
        let m = h(&[("host", "deca.ejemplo.es"), ("x-forwarded-proto", "https")]);
        assert_eq!(https_from_headers(&m).as_deref(), Some("https://deca.ejemplo.es"));
        let m = h(&[("host", "deca.ejemplo.es"), ("cf-visitor", r#"{"scheme":"https"}"#)]);
        assert_eq!(https_from_headers(&m).as_deref(), Some("https://deca.ejemplo.es"));
    }

    #[test]
    fn plain_lan_request_is_not_https() {
        let m = h(&[("host", "192.168.1.20:8134")]);
        assert_eq!(https_from_headers(&m), None);
        assert_eq!(request_base(&m).as_deref(), Some("http://192.168.1.20:8134"));
    }

    #[test]
    fn rejects_weird_hosts() {
        let m = h(&[("host", "evil.com/x?y"), ("x-forwarded-proto", "https")]);
        assert_eq!(https_from_headers(&m), None);
    }
}
