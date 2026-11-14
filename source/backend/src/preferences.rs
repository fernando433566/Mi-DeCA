//! Preferencias: campos "fijados" que se rellenan solos al crear un DeCA.
//!
//! Prioridad (de mayor a menor):
//! 1. Lo que el administrador fija para un trabajador concreto  (bloqueado para el trabajador)
//! 2. Lo que el administrador fija para toda la empresa         (bloqueado para el trabajador)
//! 3. Lo que el propio trabajador fija en Ajustes -> Preferencias
//! 4. Sugerencias automáticas (datos de la empresa como transportista, nombre del conductor)
//!
//! Los valores rellenados automáticamente se pueden cambiar en cada DeCA antes de emitirlo.
//!
//! Las **preferencias visuales** ([`VISUAL`]) son ajustes de sí/no de la app y siguen las
//! mismas capas (1-3): si el administrador las fija, el trabajador no puede cambiarlas.
//! Sin valor en ninguna capa están desactivadas.

use std::collections::BTreeMap;

use chrono::Utc;
use chrono_tz::Europe::Madrid;
use serde::Serialize;
use serde_json::{json, Map, Value};
use sqlx::SqlitePool;

use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};

/// Campos que se pueden fijar, con su nombre para mostrar.
pub const FIELDS: &[(&str, &str)] = &[
    ("cargador_empresa", "Empresa cargadora (la que carga el camión)"),
    ("cargador_nif", "NIF/CIF de la empresa cargadora"),
    ("cargador_domicilio", "Domicilio de la empresa cargadora"),
    ("transportista_empresa", "Empresa transportista (la que transporta)"),
    ("transportista_nif", "NIF transportista (de la empresa transportista)"),
    ("transportista_domicilio", "Domicilio de la empresa transportista"),
    ("transportista_titular", "Nombre transportista (dueño de los vehículos)"),
    ("conductor", "Conductor"),
    ("mercancia", "Mercancía (p. ej. «Minerales a granel»)"),
    ("peso_kg", "Peso (kg)"),
    ("volumen_m3", "Volumen de carga del camión (m³)"),
    ("origen", "Origen (dirección completa)"),
    ("destino", "Destino (dirección completa)"),
    ("matricula_camion", "Matrícula del camión"),
    ("matricula_remolque", "Matrícula del remolque"),
    ("autorizacion_especial", "Autorización especial de circulación"),
];

/// Preferencias visuales (sí/no), con su nombre para mostrar.
pub const VISUAL: &[(&str, &str)] = &[
    ("pdf_directo", "Mostrar PDF directamente al seleccionar un DeCA"),
];

/// Nombre para mostrar de cualquier preferencia.
fn display_name(key: &str) -> &str {
    FIELDS.iter().chain(VISUAL).find(|(f, _)| *f == key).map(|(_, n)| *n).unwrap_or(key)
}

const NUMERIC: &[&str] = &["peso_kg", "volumen_m3"];

pub type Prefs = Map<String, Value>;

/// Campos que existieron en versiones anteriores (p. ej. el número de viajes de ida y
/// vuelta, eliminado en la 0.3.0). Pueden seguir guardados: se ignoran sin dar error.
const RETIRED: &[&str] = &["numero_viajes"];

pub fn parse(json_text: &str) -> Prefs {
    let mut prefs: Prefs = serde_json::from_str(json_text).unwrap_or_default();
    prefs.retain(|k, _| !RETIRED.contains(&k.as_str()));
    prefs
}

/// Valida un conjunto de preferencias recibido de la app. `null` o "" quita el campo.
pub fn validate(input: Prefs) -> AppResult<Prefs> {
    let mut out = Map::new();
    for (k, v) in input {
        if RETIRED.contains(&k.as_str()) {
            continue;
        }
        if VISUAL.iter().any(|(f, _)| *f == k) {
            match v {
                Value::Null => {}
                Value::Bool(_) => {
                    out.insert(k, v);
                }
                _ => return Err(AppError::bad(format!("«{}» debe ser sí o no", display_name(&k)))),
            }
            continue;
        }
        if !FIELDS.iter().any(|(f, _)| *f == k) {
            return Err(AppError::bad(format!("Campo de preferencias desconocido: {k}")));
        }
        let v = match v {
            Value::Null => continue,
            Value::String(s) if s.trim().is_empty() => continue,
            Value::String(s) if NUMERIC.contains(&k.as_str()) => {
                let n: f64 = s.trim().replace(',', ".").parse().map_err(|_| AppError::bad(format!("«{k}» debe ser un número")))?;
                json!(n)
            }
            Value::String(s) => Value::String(s.trim().chars().take(500).collect()),
            Value::Number(n) if NUMERIC.contains(&k.as_str()) => Value::Number(n),
            _ => return Err(AppError::bad(format!("Valor no válido para «{k}»"))),
        };
        out.insert(k, v);
    }
    Ok(out)
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    AdminTrabajador,
    Empresa,
    Propia,
    Automatica,
}

#[derive(Debug, Serialize)]
pub struct EffectiveField {
    pub campo: &'static str,
    pub nombre: &'static str,
    pub valor: Option<Value>,
    pub origen: Option<Source>,
    /// `true` si lo fijó el administrador: el trabajador no puede cambiar esa preferencia
    /// (aunque sí puede corregir el valor en un DeCA concreto).
    pub bloqueado: bool,
}

#[derive(Debug, Serialize)]
pub struct Effective {
    pub campos: Vec<EffectiveField>,
    /// Borrador de DeCA listo para rellenar el formulario.
    pub plantilla: Value,
}

/// Preferencias de la empresa y las fijadas por el admin para el usuario.
pub async fn admin_layers(pool: &SqlitePool, user_id: i64) -> AppResult<(Prefs, Prefs, Prefs)> {
    let row: Option<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT u.prefs_admin, u.prefs_own, c.preferences
         FROM users u LEFT JOIN companies c ON c.id = u.company_id WHERE u.id = ?",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    let (admin, own, company) = row.ok_or_else(|| AppError::not_found("Usuario no encontrado"))?;
    Ok((parse(&admin), parse(&company.unwrap_or_default()), parse(&own)))
}

/// Calcula las preferencias efectivas y la plantilla de DeCA para un usuario.
/// `company_id` permite al administrador elegir con qué empresa emite.
pub async fn effective(pool: &SqlitePool, user: &CurrentUser, company_id: Option<i64>) -> AppResult<Effective> {
    let (admin_worker, mut company, own) = admin_layers(pool, user.id).await?;
    let mut company_id = company_id.or(user.company_id);
    if company_id.is_none() && user.is_admin() {
        // El administrador emite por defecto con su primera empresa.
        company_id = sqlx::query_scalar("SELECT id FROM companies ORDER BY id LIMIT 1").fetch_optional(pool).await?;
    }

    // Datos de la empresa como sugerencia de transportista efectivo.
    let mut auto = Map::new();
    if let Some(cid) = company_id {
        let c: Option<(String, String, String, String, String)> =
            sqlx::query_as("SELECT name, cif, domicilio, preferences, titular FROM companies WHERE id = ?")
                .bind(cid)
                .fetch_optional(pool)
                .await?;
        if let Some((name, cif, dom, prefs, titular)) = c {
            if user.company_id != Some(cid) {
                company = parse(&prefs);
            }
            auto.insert("transportista_empresa".into(), json!(name));
            if !titular.is_empty() {
                auto.insert("transportista_titular".into(), json!(titular));
            }
            if !cif.is_empty() {
                auto.insert("transportista_nif".into(), json!(cif));
            }
            if !dom.is_empty() {
                auto.insert("transportista_domicilio".into(), json!(dom));
            }
        }
    }
    auto.insert("conductor".into(), json!(user.display_name));

    let layers = [
        (&admin_worker, Source::AdminTrabajador),
        (&company, Source::Empresa),
        (&own, Source::Propia),
        (&auto, Source::Automatica),
    ];
    let mut values: BTreeMap<&str, Value> = BTreeMap::new();
    let mut campos = Vec::new();
    for (key, name) in FIELDS {
        let found = layers.iter().find_map(|(map, src)| map.get(*key).map(|v| (v.clone(), *src)));
        if let Some((v, _)) = &found {
            values.insert(key, v.clone());
        }
        campos.push(EffectiveField {
            campo: key,
            nombre: name,
            bloqueado: matches!(found, Some((_, Source::AdminTrabajador | Source::Empresa))),
            origen: found.as_ref().map(|(_, s)| *s),
            valor: found.map(|(v, _)| v),
        });
    }

    // Si hay camión fijado pero no remolque, se usa el remolque predeterminado del camión.
    if !values.contains_key("matricula_remolque") {
        if let Some(plate) = values.get("matricula_camion").and_then(|v| v.as_str()) {
            let trailer: Option<(String,)> = sqlx::query_as(
                "SELECT r.matricula FROM trucks t JOIN trailers r ON r.id = t.default_trailer_id WHERE t.matricula = ?",
            )
            .bind(crate::deca::model::normalize_plate(plate))
            .fetch_optional(pool)
            .await?;
            if let Some((m,)) = trailer {
                values.insert("matricula_remolque", json!(m));
            }
        }
    }

    let s = |k: &str| values.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let n = |k: &str| values.get(k).and_then(|v| v.as_f64());
    let plantilla = json!({
        "empresa_id": company_id,
        "cargador": { "nombre": s("cargador_empresa"), "nif": s("cargador_nif"), "domicilio": s("cargador_domicilio") },
        "transportista": {
            "nombre": s("transportista_empresa"),
            "nif": s("transportista_nif"),
            "domicilio": s("transportista_domicilio"),
            "titular": s("transportista_titular"),
        },
        "conductor": s("conductor"),
        "envios": [{
            "origen": s("origen"),
            "destino": s("destino"),
            "mercancia": s("mercancia"),
            "peso_kg": n("peso_kg"),
            "volumen_m3": n("volumen_m3"),
        }],
        "fecha_transporte": Utc::now().with_timezone(&Madrid).date_naive().to_string(),
        "fecha_fin": null,
        "matricula_camion": s("matricula_camion"),
        "matricula_remolque": s("matricula_remolque"),
        "autorizacion_especial": s("autorizacion_especial"),
        "observaciones": "",
    });
    Ok(Effective { campos, plantilla })
}

/// El trabajador no puede fijar campos que ya fijó el administrador.
pub fn reject_locked(own: &Prefs, admin_worker: &Prefs, company: &Prefs) -> AppResult<()> {
    for k in own.keys() {
        if admin_worker.contains_key(k) || company.contains_key(k) {
            return Err(AppError::bad(format!("«{}» lo ha fijado el administrador y no se puede cambiar", display_name(k))));
        }
    }
    Ok(())
}

/// Preferencias visuales efectivas de un usuario con su origen. Sin valor: desactivadas.
pub fn visual(admin_worker: &Prefs, company: &Prefs, own: &Prefs) -> Vec<EffectiveField> {
    let layers = [(admin_worker, Source::AdminTrabajador), (company, Source::Empresa), (own, Source::Propia)];
    VISUAL
        .iter()
        .map(|(key, name)| {
            let found = layers.iter().find_map(|(map, src)| map.get(*key).filter(|v| v.is_boolean()).map(|v| (v.clone(), *src)));
            EffectiveField {
                campo: key,
                nombre: name,
                bloqueado: matches!(found, Some((_, Source::AdminTrabajador | Source::Empresa))),
                origen: found.as_ref().map(|(_, s)| *s),
                valor: Some(found.map(|(v, _)| v).unwrap_or(Value::Bool(false))),
            }
        })
        .collect()
}

/// `{ "pdf_directo": true, ... }` para la app.
pub async fn visual_values(pool: &SqlitePool, user_id: i64) -> AppResult<Map<String, Value>> {
    let (admin_worker, company, own) = admin_layers(pool, user_id).await?;
    Ok(visual(&admin_worker, &company, &own)
        .into_iter()
        .map(|f| (f.campo.to_string(), f.valor.unwrap_or(Value::Bool(false))))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefs(v: Value) -> Prefs {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn visual_preferences_follow_the_layers() {
        let none = Prefs::new();
        let on = prefs(json!({ "pdf_directo": true }));
        let off = prefs(json!({ "pdf_directo": false }));
        // Sin valor: desactivada y libre.
        let f = &visual(&none, &none, &none)[0];
        assert_eq!((f.valor.clone(), f.bloqueado), (Some(json!(false)), false));
        // El usuario elige.
        assert_eq!(visual(&none, &none, &on)[0].valor, Some(json!(true)));
        // Lo que fija el administrador (empresa o trabajador) manda y queda bloqueado.
        let f = &visual(&none, &off, &on)[0];
        assert_eq!((f.valor.clone(), f.bloqueado), (Some(json!(false)), true));
        let f = &visual(&on, &off, &none)[0];
        assert_eq!((f.valor.clone(), f.bloqueado), (Some(json!(true)), true));
    }

    #[test]
    fn visual_preferences_must_be_booleans() {
        assert!(validate(prefs(json!({ "pdf_directo": "sí" }))).is_err());
        assert_eq!(validate(prefs(json!({ "pdf_directo": true }))).unwrap()["pdf_directo"], json!(true));
    }
}
