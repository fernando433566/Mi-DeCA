//! Ubicación de los PDF en disco y construcción de la URL pública.
//!
//! Estructura: `DeCA/MatriculaDelCamion/Año/NombreMes/Día/Nº<n>-NombreTrabajador-MatriculaDelCamion-Día-NombreMes-Año-Hora-Minuto-Segundo.pdf`
//! URL pública: `<base>/deca/Año/Mes/Día/<token>/<nombre del fichero>.pdf` (sin la matrícula,
//! para que los QR emitidos antes del cambio de carpetas sigan funcionando).

use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, Utc};
use chrono_tz::Europe::Madrid;

/// Convierte un texto a un nombre de fichero seguro (ASCII, sin tildes ni espacios).
pub fn sanitize(s: &str) -> String {
    let mut out = String::new();
    for c in s.trim().chars() {
        let mapped = match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'Á' | 'À' | 'Ä' | 'Â' => 'A',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'É' | 'È' | 'Ë' | 'Ê' => 'E',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'Í' | 'Ì' | 'Ï' | 'Î' => 'I',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'Ó' | 'Ò' | 'Ö' | 'Ô' => 'O',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'Ú' | 'Ù' | 'Ü' | 'Û' => 'U',
            'ñ' => 'n',
            'Ñ' => 'N',
            'ç' => 'c',
            'Ç' => 'C',
            c if c.is_ascii_alphanumeric() => c,
            _ => '_',
        };
        if !(mapped == '_' && out.ends_with('_')) {
            out.push(mapped);
        }
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() { "SinNombre".into() } else { out.chars().take(60).collect() }
}

/// Nombres de los meses en las carpetas y en los nombres de los PDF.
pub const MONTHS: [&str; 12] = [
    "Enero", "Febrero", "Marzo", "Abril", "Mayo", "Junio",
    "Julio", "Agosto", "Septiembre", "Octubre", "Noviembre", "Diciembre",
];

/// Nombre del mes `1..=12`.
pub fn month_name(month: u32) -> &'static str {
    MONTHS[(month.clamp(1, 12) - 1) as usize]
}

/// Nombre de la carpeta (y del fragmento del fichero) de una matrícula.
pub fn plate_dir(plate: &str) -> String {
    sanitize(if plate.trim().is_empty() { "SinMatricula" } else { plate })
}

/// Dónde se guarda el PDF de un DeCA:
/// `Matricula/Año/NombreMes/Día/Nº<n>-Trabajador-Matricula-Día-NombreMes-Año-Hora-Minuto-Segundo.pdf`.
/// El número es el identificador del DeCA (1 el primero emitido, 2 el segundo…),
/// así que el nombre es único y no hace falta buscar uno libre.
pub struct Location {
    pub plate_dir: String,
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub file_name: String,
}

impl Location {
    pub fn new(number: i64, worker: &str, plate: &str, at: DateTime<Utc>) -> Self {
        let t = at.with_timezone(&Madrid);
        let plate_dir = plate_dir(plate);
        let file_name = format!(
            "Nº{number}-{}-{plate_dir}-{:02}-{}-{}-{}.pdf",
            sanitize(worker),
            t.day(),
            month_name(t.month()),
            t.year(),
            t.format("%H-%M-%S"),
        );
        Location { plate_dir, year: t.year(), month: t.month(), day: t.day(), file_name }
    }

    /// `Matricula/Año/NombreMes/Día/fichero.pdf`
    pub fn rel_path(&self) -> String {
        format!("{}/{}/{}/{:02}/{}", self.plate_dir, self.year, month_name(self.month), self.day, self.file_name)
    }

    /// Ruta del enlace público, sin codificar: `/deca/Año/Mes/Día/<token>/<fichero>`.
    /// Usa el mes en número, igual que los enlaces de versiones anteriores, para que
    /// la ruta pública no dependa de cómo se organizan las carpetas.
    pub fn url_path(&self, token: &str) -> String {
        format!("/deca/{}/{:02}/{:02}/{token}/{}", self.year, self.month, self.day, self.file_name)
    }
}

/// Codifica una ruta para usarla en una URL (la «º» de «Nº» pasa a `%C2%BA`).
pub fn encode_url_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Fichero del PDF creado de antemano. Si la emisión falla antes de
/// [`Reserved::keep`], se borra solo, así no quedan ficheros sin DeCA.
pub struct Reserved {
    path: PathBuf,
    kept: bool,
}

impl Reserved {
    pub fn new(deca_dir: &Path, location: &Location) -> std::io::Result<Self> {
        let path = absolute(deca_dir, &location.rel_path());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::File::create(&path)?;
        Ok(Reserved { path, kept: false })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// La emisión se ha confirmado: el fichero ya contiene el PDF y se conserva.
    pub fn keep(mut self) {
        self.kept = true;
    }
}

impl Drop for Reserved {
    fn drop(&mut self) {
        if !self.kept {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Ruta absoluta segura a partir de una ruta relativa guardada en la base de datos.
pub fn absolute(deca_dir: &Path, rel_path: &str) -> PathBuf {
    let clean: PathBuf = Path::new(rel_path)
        .components()
        .filter(|c| matches!(c, std::path::Component::Normal(_)))
        .collect();
    deca_dir.join(clean)
}

/// Escritura atómica: primero a un fichero temporal y después `rename`,
/// para que nunca se sirva un PDF a medio escribir.
pub async fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    use tokio::io::AsyncWriteExt;
    let result = async {
        let mut file = tokio::fs::OpenOptions::new().write(true).create_new(true).open(&tmp).await?;
        file.write_all(bytes).await?;
        file.sync_all().await?;
        tokio::fs::rename(&tmp, path).await
    }.await;
    if result.is_err() { let _ = tokio::fs::remove_file(&tmp).await; }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_names() {
        assert_eq!(sanitize("José Peña Ruiz"), "Jose_Pena_Ruiz");
        assert_eq!(sanitize("../../etc"), "etc");
    }

    #[test]
    fn absolute_blocks_traversal() {
        let p = absolute(Path::new("/data/DeCA"), "../../etc/passwd");
        assert_eq!(p, PathBuf::from("/data/DeCA/etc/passwd"));
    }

    #[test]
    fn names_use_number_and_month_name() {
        // 14/11/2026 13:05:09 en Madrid (12:05:09 UTC)
        let at = "2026-11-14T12:05:09Z".parse().unwrap();
        let loc = Location::new(7, "Pepe Pérez", "1234BCD", at);
        assert_eq!(loc.file_name, "Nº7-Pepe_Perez-1234BCD-14-Noviembre-2026-13-05-09.pdf");
        assert_eq!(loc.rel_path(), "1234BCD/2026/Noviembre/14/Nº7-Pepe_Perez-1234BCD-14-Noviembre-2026-13-05-09.pdf");
        assert_eq!(loc.url_path("tok"), "/deca/2026/11/14/tok/Nº7-Pepe_Perez-1234BCD-14-Noviembre-2026-13-05-09.pdf");
        assert_eq!(encode_url_path("/deca/x/Nº7-a.pdf"), "/deca/x/N%C2%BA7-a.pdf");
    }
}
