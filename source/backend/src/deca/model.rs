//! Datos de un DeCA según el art. 6 de la Orden FOM/2861/2012 y la
//! Resolución de 5 de junio de 2026 de la DGTCF.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Empresa o persona: cargador contractual o transportista efectivo.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Parte {
    pub nombre: String,
    /// NIF / CIF
    pub nif: String,
    pub domicilio: String,
}

/// Transportista efectivo: la empresa que transporta y el titular de los vehículos.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Transportista {
    /// Empresa transportista.
    pub nombre: String,
    /// NIF de la empresa transportista.
    pub nif: String,
    pub domicilio: String,
    /// "Nombre transportista": persona o sociedad dueña de los vehículos.
    pub titular: Option<String>,
}

/// Un envío. Si un DeCA agrupa varios envíos, cada uno lleva su origen,
/// destino, naturaleza y peso (apartado Sexto).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Envio {
    /// Dirección completa del lugar de carga.
    pub origen: String,
    /// Dirección completa del lugar de descarga.
    pub destino: String,
    /// Naturaleza de la mercancía.
    pub mercancia: String,
    pub peso_kg: f64,
    /// Volumen de carga del camión.
    pub volumen_m3: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DecaDatos {
    /// Cargador contractual: la empresa que carga el camión (nombre, NIF y domicilio).
    pub cargador: Parte,
    /// Transportista efectivo: la empresa que transporta (distinta de la cargadora).
    pub transportista: Transportista,
    /// Nombre del conductor que realiza el viaje (opcional).
    #[serde(default)]
    pub conductor: Option<String>,
    pub envios: Vec<Envio>,
    /// Fecha de realización del transporte (día en que se inicia la marcha).
    pub fecha_transporte: NaiveDate,
    /// Fecha prevista de fin del servicio (si se omite, el mismo día).
    #[serde(default)]
    pub fecha_fin: Option<NaiveDate>,
    pub matricula_camion: String,
    #[serde(default)]
    pub matricula_remolque: Option<String>,
    /// Autorización especial de circulación, si procede.
    #[serde(default)]
    pub autorizacion_especial: Option<String>,
    #[serde(default)]
    pub observaciones: Option<String>,
}

const MAX_TEXT: usize = 500;
pub const MAX_ENVIOS: usize = 50;

fn required(value: &str, label: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::bad(format!("Falta rellenar: {label}")));
    }
    if value.len() > MAX_TEXT {
        return Err(AppError::bad(format!("El campo «{label}» es demasiado largo")));
    }
    Ok(())
}

fn optional_len(value: &Option<String>, label: &str) -> Result<(), AppError> {
    match value {
        Some(v) if v.len() > MAX_TEXT * 4 => Err(AppError::bad(format!("El campo «{label}» es demasiado largo"))),
        _ => Ok(()),
    }
}

fn clean_opt(v: &mut Option<String>) {
    if let Some(s) = v {
        *s = s.trim().to_string();
        if s.is_empty() {
            *v = None;
        }
    }
}

impl DecaDatos {
    /// Normaliza (recorta espacios, matrículas en mayúsculas) y valida.
    pub fn normalize_and_validate(&mut self) -> Result<(), AppError> {
        let (c, t) = (&mut self.cargador, &mut self.transportista);
        for (nombre, nif, domicilio) in [(&mut c.nombre, &mut c.nif, &mut c.domicilio), (&mut t.nombre, &mut t.nif, &mut t.domicilio)] {
            *nombre = nombre.trim().to_string();
            *nif = nif.trim().to_uppercase();
            *domicilio = domicilio.trim().to_string();
        }
        clean_opt(&mut self.transportista.titular);
        self.matricula_camion = normalize_plate(&self.matricula_camion);
        if let Some(r) = &mut self.matricula_remolque {
            *r = normalize_plate(r);
        }
        clean_opt(&mut self.matricula_remolque);
        clean_opt(&mut self.conductor);
        clean_opt(&mut self.autorizacion_especial);
        clean_opt(&mut self.observaciones);

        required(&self.cargador.nombre, "Empresa cargadora")?;
        required(&self.cargador.nif, "NIF de la empresa cargadora")?;
        required(&self.cargador.domicilio, "Domicilio de la empresa cargadora")?;
        required(&self.transportista.nombre, "Empresa transportista")?;
        required(&self.transportista.nif, "NIF transportista")?;
        required(&self.transportista.domicilio, "Domicilio de la empresa transportista")?;
        optional_len(&self.transportista.titular, "Nombre transportista")?;
        required(&self.matricula_camion, "Matrícula del camión")?;
        optional_len(&self.conductor, "Conductor")?;
        optional_len(&self.autorizacion_especial, "Autorización especial")?;
        optional_len(&self.observaciones, "Observaciones")?;

        if self.envios.is_empty() {
            return Err(AppError::bad("Añade al menos un envío (origen, destino, mercancía y peso)"));
        }
        if self.envios.len() > MAX_ENVIOS {
            return Err(AppError::bad(format!("Máximo {MAX_ENVIOS} envíos por DeCA")));
        }
        for (i, e) in self.envios.iter_mut().enumerate() {
            let n = i + 1;
            e.origen = e.origen.trim().to_string();
            e.destino = e.destino.trim().to_string();
            e.mercancia = e.mercancia.trim().to_string();
            required(&e.origen, &format!("Envío {n} · Origen"))?;
            required(&e.destino, &format!("Envío {n} · Destino"))?;
            required(&e.mercancia, &format!("Envío {n} · Mercancía"))?;
            if !(e.peso_kg.is_finite() && e.peso_kg > 0.0) {
                return Err(AppError::bad(format!("Envío {n}: el peso debe ser mayor que 0 kg")));
            }
            if let Some(v) = e.volumen_m3 {
                if !(v.is_finite() && v >= 0.0) {
                    return Err(AppError::bad(format!("Envío {n}: volumen no válido")));
                }
            }
        }
        if let Some(fin) = self.fecha_fin {
            if fin < self.fecha_transporte {
                return Err(AppError::bad("La fecha de fin no puede ser anterior a la de inicio"));
            }
        }
        Ok(())
    }

    /// Datos del viaje de vuelta: cada envío con origen y destino intercambiados
    /// (y los envíos en orden inverso, para deshacer la ruta).
    pub fn reversed(&self) -> DecaDatos {
        let mut d = self.clone();
        d.envios.reverse();
        for e in &mut d.envios {
            std::mem::swap(&mut e.origen, &mut e.destino);
        }
        d
    }

    /// Datos del viaje número `n` (1, 2, 3…): los impares son la ida (A → B) y los
    /// pares la vuelta (B → A).
    pub fn for_trip(&self, n: i64) -> DecaDatos {
        if n % 2 == 0 { self.reversed() } else { self.clone() }
    }

    /// Último día del servicio (para calcular la caducidad del enlace público).
    pub fn service_end(&self) -> NaiveDate {
        self.fecha_fin.unwrap_or(self.fecha_transporte)
    }

    /// Lista plana `(campo, valor)` usada para comparar versiones al modificar.
    pub fn fields(&self) -> Vec<(String, String)> {
        let mut f = vec![
            ("Empresa cargadora".into(), self.cargador.nombre.clone()),
            ("NIF empresa cargadora".into(), self.cargador.nif.clone()),
            ("Domicilio empresa cargadora".into(), self.cargador.domicilio.clone()),
            ("Empresa transportista".into(), self.transportista.nombre.clone()),
            ("NIF transportista".into(), self.transportista.nif.clone()),
            ("Domicilio empresa transportista".into(), self.transportista.domicilio.clone()),
            ("Nombre transportista (titular vehículos)".into(), self.transportista.titular.clone().unwrap_or_default()),
            ("Conductor".into(), self.conductor.clone().unwrap_or_default()),
            ("Fecha de realización".into(), fmt_date(self.fecha_transporte)),
            ("Fecha de fin".into(), self.fecha_fin.map(fmt_date).unwrap_or_default()),
            ("Matrícula camión".into(), self.matricula_camion.clone()),
            ("Matrícula remolque".into(), self.matricula_remolque.clone().unwrap_or_default()),
            ("Autorización especial".into(), self.autorizacion_especial.clone().unwrap_or_default()),
            ("Observaciones".into(), self.observaciones.clone().unwrap_or_default()),
        ];
        for (i, e) in self.envios.iter().enumerate() {
            let n = i + 1;
            f.push((format!("Envío {n} · Origen"), e.origen.clone()));
            f.push((format!("Envío {n} · Destino"), e.destino.clone()));
            f.push((format!("Envío {n} · Mercancía"), e.mercancia.clone()));
            f.push((format!("Envío {n} · Peso"), fmt_kg(e.peso_kg)));
            f.push((format!("Envío {n} · Volumen"), e.volumen_m3.map(fmt_m3).unwrap_or_default()));
        }
        f
    }

    /// Campos que cambian entre `self` (antes) y `new` (después).
    pub fn diff(&self, new: &DecaDatos) -> Vec<Cambio> {
        let old = self.fields();
        let new_f = new.fields();
        let mut labels: Vec<&String> = old.iter().map(|(k, _)| k).collect();
        for (k, _) in &new_f {
            if !labels.contains(&k) {
                labels.push(k);
            }
        }
        let get = |list: &[(String, String)], k: &str| {
            list.iter().find(|(l, _)| l == k).map(|(_, v)| v.clone()).unwrap_or_default()
        };
        labels
            .into_iter()
            .filter_map(|k| {
                let (a, b) = (get(&old, k), get(&new_f, k));
                (a != b).then(|| Cambio { campo: k.clone(), antes: a, despues: b })
            })
            .collect()
    }
}

/// Un campo modificado: el valor `antes` queda marcado como NO VÁLIDO en el PDF.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cambio {
    pub campo: String,
    pub antes: String,
    pub despues: String,
}

/// Una modificación durante el servicio (apartado Quinto, método 1:
/// mismo PDF, misma URL y mismo QR, con los datos antiguos invalidados y el motivo).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Modificacion {
    pub fecha: DateTime<Utc>,
    pub usuario: String,
    pub motivo: String,
    pub version: i64,
    pub cambios: Vec<Cambio>,
}

pub fn normalize_plate(p: &str) -> String {
    p.chars().filter(|c| !c.is_whitespace() && *c != '-').collect::<String>().to_uppercase()
}

pub fn fmt_date(d: NaiveDate) -> String {
    d.format("%d/%m/%Y").to_string()
}

pub fn fmt_kg(v: f64) -> String {
    format!("{} kg", fmt_num(v))
}

pub fn fmt_m3(v: f64) -> String {
    format!("{} m³", fmt_num(v))
}

/// Número en formato español: 24.500,5
pub fn fmt_num(v: f64) -> String {
    let rounded = (v * 100.0).round() / 100.0;
    let int = rounded.trunc() as i64;
    let frac = ((rounded - int as f64).abs() * 100.0).round() as i64;
    let digits = int.abs().to_string();
    let mut grouped = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(ch);
    }
    let sign = if int < 0 { "-" } else { "" };
    if frac == 0 {
        format!("{sign}{grouped}")
    } else if frac % 10 == 0 {
        format!("{sign}{grouped},{}", frac / 10)
    } else {
        format!("{sign}{grouped},{frac:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_es() {
        assert_eq!(fmt_num(24500.0), "24.500");
        assert_eq!(fmt_num(1234567.5), "1.234.567,5");
        assert_eq!(fmt_num(12.25), "12,25");
    }

    #[test]
    fn round_trips_alternate() {
        let d = DecaDatos {
            cargador: Parte::default(),
            transportista: Transportista::default(),
            conductor: None,
            envios: vec![
                Envio { origen: "A".into(), destino: "B".into(), ..Default::default() },
                Envio { origen: "B".into(), destino: "C".into(), ..Default::default() },
            ],
            fecha_transporte: NaiveDate::from_ymd_opt(2026, 11, 7).unwrap(),
            fecha_fin: None,
            matricula_camion: "1234BCD".into(),
            matricula_remolque: None,
            autorizacion_especial: None,
            observaciones: None,
        };
        let ruta = |d: &DecaDatos| d.envios.iter().map(|e| format!("{}>{}", e.origen, e.destino)).collect::<Vec<_>>().join(",");
        assert_eq!(ruta(&d.for_trip(1)), "A>B,B>C");
        assert_eq!(ruta(&d.for_trip(2)), "C>B,B>A");
        assert_eq!(ruta(&d.for_trip(3)), "A>B,B>C");
    }

    #[test]
    fn plates() {
        assert_eq!(normalize_plate(" 1234-bcd "), "1234BCD");
    }
}
