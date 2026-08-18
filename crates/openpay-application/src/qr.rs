use crate::ports::{QrNonceStore, RepositoryError};
use openpay_crypto::{parse_and_verify_qr_token, CryptoError};
use openpay_domain::{MerchantId, PaymentId, TenantId};
use time::OffsetDateTime;

use crate::payments::ApplicationError;

pub struct VerifiedQr {
    pub payment_id: PaymentId,
    pub tenant_id: TenantId,
    pub merchant_id: MerchantId,
    pub nonce: String,
}

pub async fn verify_qr_token<S: QrNonceStore>(
    store: &S,
    secret: &[u8],
    token: &str,
    now: OffsetDateTime,
    expected_payment: Option<PaymentId>,
    expected_merchant: Option<MerchantId>,
    consume: bool,
) -> Result<VerifiedQr, ApplicationError> {
    let claims = parse_and_verify_qr_token(secret, token, now).map_err(|e| match e {
        CryptoError::Expired => ApplicationError::Expired,
        CryptoError::Replay => ApplicationError::Replay,
        CryptoError::SignatureMismatch | CryptoError::Malformed | CryptoError::InvalidKey => {
            ApplicationError::Forbidden
        }
        CryptoError::Hashing => ApplicationError::Connector(e.to_string()),
    })?;

    let payment_id: PaymentId = claims
        .payment_id
        .parse()
        .map_err(ApplicationError::Domain)?;
    let tenant_id: TenantId = claims.tenant_id.parse().map_err(ApplicationError::Domain)?;
    let merchant_id: MerchantId = claims
        .merchant_id
        .parse()
        .map_err(ApplicationError::Domain)?;

    if let Some(expected) = expected_payment {
        if expected != payment_id {
            return Err(ApplicationError::Forbidden);
        }
    }
    if let Some(expected) = expected_merchant {
        if expected != merchant_id {
            return Err(ApplicationError::Forbidden);
        }
    }

    if consume {
        let inserted = store
            .remember_nonce(&claims.nonce, 900)
            .await
            .map_err(ApplicationError::Repository)?;
        if !inserted {
            return Err(ApplicationError::Replay);
        }
    }

    let _ = RepositoryError::NotFound;
    Ok(VerifiedQr {
        payment_id,
        tenant_id,
        merchant_id,
        nonce: claims.nonce,
    })
}
