//! Códigos de verificación de 6 dígitos (TOTP, RFC 6238) compatibles con
//! Google Authenticator, Microsoft Authenticator, Aegis, etc.

use hmac::{Hmac, Mac};
use rand::RngCore;
use sha1::Sha1;

const DIGITS: u32 = 6;
const STEP_SECS: u64 = 30;

/// Genera un secreto nuevo codificado en Base32 (160 bits).
pub fn generate_secret() -> String {
    let mut bytes = [0u8; 20];
    rand::thread_rng().fill_bytes(&mut bytes);
    data_encoding::BASE32_NOPAD.encode(&bytes)
}

/// URL `otpauth://` para dar de alta el secreto en la app de autenticación (vía QR).
pub fn otpauth_url(secret: &str, issuer: &str, account: &str) -> String {
    let enc = |s: &str| -> String {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
                _ => format!("%{b:02X}"),
            })
            .collect()
    };
    format!(
        "otpauth://totp/{}:{}?secret={}&issuer={}&digits={DIGITS}&period={STEP_SECS}",
        enc(issuer),
        enc(account),
        secret,
        enc(issuer)
    )
}

fn code_at(secret: &[u8], counter: u64) -> u32 {
    let mut mac = Hmac::<Sha1>::new_from_slice(secret).expect("HMAC acepta cualquier longitud");
    mac.update(&counter.to_be_bytes());
    let h = mac.finalize().into_bytes();
    let offset = (h[h.len() - 1] & 0x0f) as usize;
    let bin = ((h[offset] as u32 & 0x7f) << 24)
        | ((h[offset + 1] as u32) << 16)
        | ((h[offset + 2] as u32) << 8)
        | (h[offset + 3] as u32);
    bin % 10u32.pow(DIGITS)
}

/// Comprueba un código admitiendo ±1 intervalo de 30 s por desfases de reloj.
pub fn verify(secret_b32: &str, code: &str) -> bool {
    let code = code.trim().replace(' ', "");
    if code.len() != DIGITS as usize || !code.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let Ok(expected) = code.parse::<u32>() else { return false };
    let Ok(secret) = data_encoding::BASE32_NOPAD.decode(secret_b32.as_bytes()) else {
        return false;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let counter = now / STEP_SECS;
    [counter.saturating_sub(1), counter, counter + 1]
        .iter()
        .any(|c| code_at(&secret, *c) == expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6238_vector() {
        // Vector de prueba RFC 6238 (SHA1, T = 59 s) -> 94287082, últimos 6 dígitos 287082
        assert_eq!(code_at(b"12345678901234567890", 59 / 30), 287082);
    }
}
