//! Configuración del servidor leída de variables de entorno.

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    /// Puerto HTTP (por defecto 8134).
    pub port: u16,
    /// Carpeta donde se guardan la base de datos, los PDF y los mapas.
    pub data_dir: PathBuf,
    /// URL pública inicial (p. ej. la de Cloudflare Tunnel). Se puede cambiar desde
    /// Ajustes -> Servidor; este valor solo se usa si no hay ninguno guardado.
    pub public_base_url: Option<String>,
    /// Permite URL públicas `http://` (solo para pruebas locales; la normativa exige HTTPS).
    pub allow_insecure_public_url: bool,
    /// Carpeta con la aplicación web (HTML/CSS/JS) que se sirve en `/`.
    pub web_dir: PathBuf,
}

impl Config {
    pub fn from_env() -> Self {
        let port = std::env::var("DECA_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8134);
        let data_dir = std::env::var("DECA_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./data"));
        let public_base_url = std::env::var("DECA_PUBLIC_URL")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let allow_insecure_public_url = std::env::var("DECA_ALLOW_INSECURE_URL")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let web_dir = std::env::var("DECA_WEB_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./web"));
        Self {
            web_dir,
            port,
            data_dir,
            public_base_url,
            allow_insecure_public_url,
        }
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("deca.db")
    }

    /// Raíz del repositorio de PDF: `<data>/DeCA/Año/Mes/Día/...`
    pub fn deca_dir(&self) -> PathBuf {
        self.data_dir.join("DeCA")
    }

    pub fn tiles_dir(&self) -> PathBuf {
        self.data_dir.join("mapas")
    }

    pub fn tmp_dir(&self) -> PathBuf {
        self.data_dir.join("tmp")
    }
}
