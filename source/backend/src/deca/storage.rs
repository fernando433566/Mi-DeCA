//! Ubicación de los PDF en disco y construcción de la URL pública.
//!
//! Estructura: `DeCA/Año/Mes/Día/NombreTrabajador-MatriculaDelCamion-Dia-Mes-Año-Hora-Minuto-Segundo.pdf`
//! URL pública: `<base>/deca/Año/Mes/Día/<token>/<nombre del fichero>.pdf`

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
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

/// Ruta relativa (dentro de la carpeta `DeCA/`) y nombre del fichero.
pub struct Location {
    /// `Año/Mes/Día`
    pub rel_dir: String,
    pub file_name: String,
}

impl Location {
    pub fn rel_path(&self) -> String {
        format!("{}/{}", self.rel_dir, self.file_name)
    }

    pub fn url_path(&self, token: &str) -> String {
        format!("/deca/{}/{}/{}", self.rel_dir, token, self.file_name)
    }
}

/// Calcula una ubicación libre (si ya existe un fichero con ese nombre, por ejemplo
/// varios viajes en el mismo segundo, añade `-2`, `-3`...).
pub fn new_location(deca_dir: &Path, worker: &str, plate: &str, at: DateTime<Utc>) -> Location {
    let t = at.with_timezone(&Madrid);
    let rel_dir = t.format("%Y/%m/%d").to_string();
    let base = format!(
        "{}-{}-{}",
        sanitize(worker),
        sanitize(if plate.is_empty() { "SinMatricula" } else { plate }),
        t.format("%d-%m-%Y-%H-%M-%S")
    );
    let dir = deca_dir.join(&rel_dir);
    let mut file_name = format!("{base}.pdf");
    let mut n = 2;
    while dir.join(&file_name).exists() {
        file_name = format!("{base}-{n}.pdf");
        n += 1;
    }
    Location { rel_dir, file_name }
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
    let tmp = path.with_extension("pdf.tmp");
    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(&tmp, path).await
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
}
