use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("invalid identifier for prefix {prefix}: {value}")]
    InvalidId { prefix: String, value: String },
    #[error("invalid currency: {0}")]
    InvalidCurrency(String),
    #[error("amount_minor must be a positive integer")]
    InvalidAmount,
    #[error("invalid idempotency key")]
    InvalidIdempotencyKey,
    #[error("invalid merchant_order_id")]
    InvalidMerchantOrderId,
    #[error("invalid connector id")]
    InvalidConnectorId,
    #[error("illegal payment transition from {from} to {to}")]
    IllegalTransition { from: String, to: String },
    #[error("payment is not refundable in status {0}")]
    NotRefundable(String),
    #[error("payment cannot be cancelled in status {0}")]
    NotCancellable(String),
    #[error("tenant mismatch")]
    TenantMismatch,
    #[error("merchant mismatch")]
    MerchantMismatch,
    #[error("token binding mismatch")]
    TokenBindingMismatch,
    #[error("validation failed: {0}")]
    Validation(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Validation,
    Conflict,
    NotFound,
    Forbidden,
    Idempotency,
    Connector,
    Transient,
    Internal,
}
