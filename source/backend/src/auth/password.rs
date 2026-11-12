//! Hash de contraseñas con Argon2id.

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;

pub fn hash(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("no se pudo cifrar la contraseña: {e}"))
}

pub fn verify(password: &str, stored_hash: &str) -> bool {
    PasswordHash::new(stored_hash)
        .map(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
        .unwrap_or(false)
}

/// Reglas mínimas para contraseñas nuevas (se permiten PIN numéricos sencillos de recordar).
pub fn validate_new(password: &str) -> Result<(), String> {
    if password == crate::db::DEFAULT_ADMIN_PASSWORD {
        return Err("Elige una contraseña distinta de la antigua clave pública".into());
    }
    if password.chars().count() < 4 {
        return Err("La contraseña debe tener al menos 4 caracteres".into());
    }
    if password.len() > 256 {
        return Err("La contraseña es demasiado larga".into());
    }
    Ok(())
}
