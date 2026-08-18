use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rand::RngCore;
use rand::rngs::OsRng;

use crate::hmac_util::CryptoError;

/// Unresolved connector configuration reference stored in the catalog.
pub const SECRET_REF_PREFIX: &str = "secret://";
/// AES-256-GCM envelope produced by [`encrypt_secret`].
pub const ENVELOPE_PREFIX: &str = "enc:v1:";

const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

pub fn is_secret_ref(value: &str) -> bool {
    value.starts_with(SECRET_REF_PREFIX)
}

pub fn is_encrypted_envelope(value: &str) -> bool {
    value.starts_with(ENVELOPE_PREFIX)
}

/// Accept 32 raw bytes, or standard Base64 that decodes to 32 bytes.
pub fn decode_master_key(raw: &str) -> Result<[u8; KEY_LEN], CryptoError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(CryptoError::InvalidMasterKey);
    }
    if let Ok(decoded) = STANDARD.decode(raw) {
        if decoded.len() == KEY_LEN {
            return decoded
                .try_into()
                .map_err(|_| CryptoError::InvalidMasterKey);
        }
    }
    if raw.len() == KEY_LEN {
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(raw.as_bytes());
        return Ok(key);
    }
    Err(CryptoError::InvalidMasterKey)
}

/// Encrypt plaintext with AES-256-GCM. Output is `enc:v1:` + base64(nonce || ciphertext).
pub fn encrypt_secret(master_key: &[u8; KEY_LEN], plaintext: &[u8]) -> Result<String, CryptoError> {
    let cipher =
        Aes256Gcm::new_from_slice(master_key).map_err(|_| CryptoError::InvalidMasterKey)?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .map_err(|_| CryptoError::Encryption)?;
    let mut packed = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    packed.extend_from_slice(&nonce_bytes);
    packed.extend_from_slice(&ciphertext);
    Ok(format!("{ENVELOPE_PREFIX}{}", STANDARD.encode(packed)))
}

pub fn decrypt_secret(master_key: &[u8; KEY_LEN], encoded: &str) -> Result<Vec<u8>, CryptoError> {
    let payload = encoded
        .strip_prefix(ENVELOPE_PREFIX)
        .ok_or(CryptoError::Malformed)?;
    let packed = STANDARD
        .decode(payload)
        .map_err(|_| CryptoError::Malformed)?;
    if packed.len() <= NONCE_LEN {
        return Err(CryptoError::Malformed);
    }
    let (nonce, ciphertext) = packed.split_at(NONCE_LEN);
    let cipher =
        Aes256Gcm::new_from_slice(master_key).map_err(|_| CryptoError::InvalidMasterKey)?;
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| CryptoError::Decryption)
}

/// Encrypt a connector `configuration_ref` if it is still plaintext.
pub fn seal_if_plaintext(master_key: &[u8; KEY_LEN], value: &str) -> Result<String, CryptoError> {
    if is_encrypted_envelope(value) {
        return Ok(value.to_string());
    }
    encrypt_secret(master_key, value.as_bytes())
}

/// Decrypt `enc:v1:` envelopes; leave `secret://` and other plaintext refs as-is.
pub fn open_secret_value(master_key: &[u8; KEY_LEN], value: &str) -> Result<String, CryptoError> {
    if is_encrypted_envelope(value) {
        let bytes = decrypt_secret(master_key, value)?;
        String::from_utf8(bytes).map_err(|_| CryptoError::Malformed)
    } else {
        Ok(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_raw_32_byte_key() {
        let key = decode_master_key("openpay-master-key-32-bytes-ok!!").expect("key");
        let enc = encrypt_secret(&key, b"psp-client-secret").expect("encrypt");
        assert!(is_encrypted_envelope(&enc));
        let dec = decrypt_secret(&key, &enc).expect("decrypt");
        assert_eq!(dec, b"psp-client-secret");
    }

    #[test]
    fn roundtrip_base64_key() {
        let raw = [7u8; 32];
        let b64 = STANDARD.encode(raw);
        let key = decode_master_key(&b64).expect("key");
        let enc = encrypt_secret(&key, b"hello").unwrap();
        assert_eq!(decrypt_secret(&key, &enc).unwrap(), b"hello");
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = decode_master_key("openpay-master-key-32-bytes-ok!!").unwrap();
        let mut enc = encrypt_secret(&key, b"payload").unwrap();
        enc.push('x');
        assert!(decrypt_secret(&key, &enc).is_err());
    }

    #[test]
    fn short_key_rejected() {
        assert!(decode_master_key("too-short").is_err());
        assert!(decode_master_key("").is_err());
    }

    #[test]
    fn secret_ref_detected() {
        assert!(is_secret_ref("secret://connectors/mock-instant"));
        assert!(!is_secret_ref("enc:v1:abc"));
    }

    #[test]
    fn seal_plaintext_and_open_roundtrip() {
        let key = decode_master_key("openpay-master-key-32-bytes-ok!!").unwrap();
        let sealed = seal_if_plaintext(&key, "secret://connectors/mock-instant").unwrap();
        assert!(is_encrypted_envelope(&sealed));
        assert_eq!(
            open_secret_value(&key, &sealed).unwrap(),
            "secret://connectors/mock-instant"
        );
        assert_eq!(seal_if_plaintext(&key, &sealed).unwrap(), sealed);
        assert_eq!(
            open_secret_value(&key, "secret://plain").unwrap(),
            "secret://plain"
        );
    }
}
