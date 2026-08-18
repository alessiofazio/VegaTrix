use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::hmac_util::{CryptoError, hmac_sha256_hex, verify_hmac_sha256};
use openpay_domain::{MerchantId, PaymentId, TenantId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrClaims {
    pub payment_id: String,
    pub tenant_id: String,
    pub merchant_id: String,
    pub exp: i64,
    pub nonce: String,
    pub v: u8,
}

#[derive(Debug, Clone)]
pub struct QrToken {
    pub claims: QrClaims,
    pub signed: String,
}

impl QrClaims {
    pub fn new(
        payment_id: PaymentId,
        tenant_id: TenantId,
        merchant_id: MerchantId,
        expires_at: OffsetDateTime,
        nonce: String,
    ) -> Self {
        Self {
            payment_id: payment_id.as_prefixed(),
            tenant_id: tenant_id.as_prefixed(),
            merchant_id: merchant_id.as_prefixed(),
            exp: expires_at.unix_timestamp(),
            nonce,
            v: 1,
        }
    }

    pub fn encode(&self, secret: &[u8]) -> Result<String, CryptoError> {
        let payload = serde_json::to_vec(self).map_err(|_| CryptoError::Malformed)?;
        let b64 =
            base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, payload);
        let sig = hmac_sha256_hex(secret, b64.as_bytes())?;
        Ok(format!("{b64}.{sig}"))
    }
}

pub fn parse_and_verify_qr_token(
    secret: &[u8],
    token: &str,
    now: OffsetDateTime,
) -> Result<QrClaims, CryptoError> {
    let (payload_b64, sig) = token.split_once('.').ok_or(CryptoError::Malformed)?;
    verify_hmac_sha256(secret, payload_b64.as_bytes(), sig)?;
    let json = base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        payload_b64,
    )
    .map_err(|_| CryptoError::Malformed)?;
    let claims: QrClaims = serde_json::from_slice(&json).map_err(|_| CryptoError::Malformed)?;
    if now.unix_timestamp() > claims.exp {
        return Err(CryptoError::Expired);
    }
    Ok(claims)
}

pub fn qr_uri(payment_id: PaymentId, token: &str) -> String {
    format!("openpay://v1/pay/{payment_id}?token={token}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use openpay_domain::{MerchantId, PaymentId, TenantId};
    use time::{Duration, OffsetDateTime};

    #[test]
    fn roundtrip_qr_token() {
        let secret = b"test-secret-qr-signing-key-32b!!";
        let now = OffsetDateTime::now_utc();
        let claims = QrClaims::new(
            PaymentId::new(),
            TenantId::new(),
            MerchantId::new(),
            now + Duration::seconds(300),
            "nonce-1".into(),
        );
        let token = claims.encode(secret).unwrap();
        let parsed = parse_and_verify_qr_token(secret, &token, now).unwrap();
        assert_eq!(parsed.payment_id, claims.payment_id);
    }

    #[test]
    fn expired_token_rejected() {
        let secret = b"test-secret-qr-signing-key-32b!!";
        let now = OffsetDateTime::now_utc();
        let claims = QrClaims::new(
            PaymentId::new(),
            TenantId::new(),
            MerchantId::new(),
            now - Duration::seconds(1),
            "nonce-2".into(),
        );
        let token = claims.encode(secret).unwrap();
        assert!(matches!(
            parse_and_verify_qr_token(secret, &token, now),
            Err(CryptoError::Expired)
        ));
    }

    #[test]
    fn tampered_signature_rejected() {
        let secret = b"test-secret-qr-signing-key-32b!!";
        let now = OffsetDateTime::now_utc();
        let claims = QrClaims::new(
            PaymentId::new(),
            TenantId::new(),
            MerchantId::new(),
            now + Duration::seconds(60),
            "nonce-3".into(),
        );
        let token = claims.encode(secret).unwrap();
        let bad = format!("{token}ff");
        assert!(parse_and_verify_qr_token(secret, &bad, now).is_err());
    }
}
