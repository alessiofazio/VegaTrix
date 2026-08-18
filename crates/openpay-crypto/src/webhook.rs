use time::OffsetDateTime;

use crate::hmac_util::{CryptoError, hmac_sha256_hex, verify_hmac_sha256};

/// `OpenPay-Signature: t=<unix>,v1=<hex>`
pub fn sign_webhook(secret: &[u8], timestamp: i64, raw_body: &[u8]) -> Result<String, CryptoError> {
    let mut message = timestamp.to_string().into_bytes();
    message.push(b'.');
    message.extend_from_slice(raw_body);
    let sig = hmac_sha256_hex(secret, &message)?;
    Ok(format!("t={timestamp},v1={sig}"))
}

pub fn verify_webhook(
    secret: &[u8],
    header: &str,
    raw_body: &[u8],
    now: OffsetDateTime,
    tolerance_secs: i64,
) -> Result<(), CryptoError> {
    let mut timestamp: Option<i64> = None;
    let mut signature: Option<&str> = None;
    for part in header.split(',') {
        let part = part.trim();
        if let Some(t) = part.strip_prefix("t=") {
            timestamp = t.parse().ok();
        } else if let Some(v) = part.strip_prefix("v1=") {
            signature = Some(v);
        }
    }
    let ts = timestamp.ok_or(CryptoError::Malformed)?;
    let sig = signature.ok_or(CryptoError::Malformed)?;
    let now_ts = now.unix_timestamp();
    if (now_ts - ts).abs() > tolerance_secs {
        return Err(CryptoError::Expired);
    }
    let mut message = ts.to_string().into_bytes();
    message.push(b'.');
    message.extend_from_slice(raw_body);
    verify_hmac_sha256(secret, &message, sig)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;

    #[test]
    fn webhook_sign_and_verify() {
        let secret = b"whsec_test_secret_value________";
        let body = br#"{"id":"evt_1","type":"payment.settled"}"#;
        let now = OffsetDateTime::now_utc();
        let header = sign_webhook(secret, now.unix_timestamp(), body).unwrap();
        verify_webhook(secret, &header, body, now, 300).unwrap();
    }

    #[test]
    fn webhook_replay_outside_tolerance() {
        let secret = b"whsec_test_secret_value________";
        let body = b"{}";
        let now = OffsetDateTime::now_utc();
        let header = sign_webhook(
            secret,
            (now - Duration::seconds(400)).unix_timestamp(),
            body,
        )
        .unwrap();
        assert!(verify_webhook(secret, &header, body, now, 300).is_err());
    }
}
