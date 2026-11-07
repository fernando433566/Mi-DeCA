//! Errores de la API. Todos se devuelven como JSON `{ "error": "...", "codigo": "..." }`
//! con mensajes en español pensados para mostrarse directamente al conductor.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("Usuario o contraseña incorrectos")]
    InvalidCredentials,
    #[error("Se necesita el código de verificación de 6 dígitos")]
    TotpRequired,
    #[error("Sesión no válida o caducada. Vuelve a iniciar sesión")]
    Unauthorized,
    #[error("No tienes permiso para hacer esto")]
    Forbidden,
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Gone(String),
    #[error("Demasiados intentos. Espera unos minutos")]
    TooManyRequests,
    #[error("Error interno del servidor")]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    pub fn bad(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "datos_no_validos"),
            Self::InvalidCredentials => (StatusCode::UNAUTHORIZED, "credenciales"),
            Self::TotpRequired => (StatusCode::UNAUTHORIZED, "requiere_2fa"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "sesion"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "prohibido"),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "no_encontrado"),
            Self::Conflict(_) => (StatusCode::CONFLICT, "conflicto"),
            Self::Gone(_) => (StatusCode::GONE, "caducado"),
            Self::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, "demasiados_intentos"),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "interno"),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let Self::Internal(e) = &self {
            tracing::error!("error interno: {e:#}");
        }
        let (status, code) = self.status_and_code();
        (status, Json(json!({ "error": self.to_string(), "codigo": code }))).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &e {
            if db.is_unique_violation() {
                return Self::Conflict("Ya existe un registro con esos datos".into());
            }
        }
        Self::Internal(e.into())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::Internal(e.into())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::Internal(e.into())
    }
}

pub type AppResult<T> = Result<T, AppError>;
