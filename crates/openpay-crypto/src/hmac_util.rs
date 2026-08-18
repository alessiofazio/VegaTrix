use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("invalid hmac key")]
    InvalidKey,
    #[error("signature mismatch")]
    SignatureMismatch,
    #[error("token expired")]
    Expired,
    #[error("token already used")]
    Replay,
    #[error("malformed token")]
    Malformed,
    #[error("hashing failed")]
    Hashing,
}

pub type HmacSha256 = Hmac<Sha256>;

pub fn hmac_sha256(secret: &[u8], message: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| CryptoError::InvalidKey)?;
    mac.update(message);
    Ok(mac.finalize().into_bytes().to_vec())
}

pub fn hmac_sha256_hex(secret: &[u8], message: &[u8]) -> Result<String, CryptoError> {
    Ok(hex::encode(hmac_sha256(secret, message)?))
}

pub fn verify_hmac_sha256(secret: &[u8], message: &[u8], expected_hex: &str) -> Result<(), CryptoError> {
    let expected = hex::decode(expected_hex.trim()).map_err(|_| CryptoError::Malformed)?;
    let actual = hmac_sha256(secret, message)?;
    if actual.len() != expected.len() {
        return Err(CryptoError::SignatureMismatch);
    }
    let mut diff = 0u8;
    for (a, b) in actual.iter().zip(expected.iter()) {
        diff |= a ^ b;
    }
    if diff == 0 {
        Ok(())
    } else {
        Err(CryptoError::SignatureMismatch)
    }
}

pub fn sha256_hex(input: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(Sha256::digest(input))
}
