use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use rand::RngCore;
use rand::rngs::OsRng;

use crate::hmac_util::{CryptoError, sha256_hex};

pub fn generate_api_key() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    format!("opk_{}", hex::encode(bytes))
}

pub fn api_key_fingerprint(raw: &str) -> String {
    sha256_hex(raw.as_bytes())
}

pub fn hash_secret(raw: &str) -> Result<String, CryptoError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(raw.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|_| CryptoError::Hashing)
}

pub fn verify_secret(raw: &str, hash: &str) -> Result<bool, CryptoError> {
    let parsed = PasswordHash::new(hash).map_err(|_| CryptoError::Hashing)?;
    Ok(Argon2::default()
        .verify_password(raw.as_bytes(), &parsed)
        .is_ok())
}

pub fn generate_nonce() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}
