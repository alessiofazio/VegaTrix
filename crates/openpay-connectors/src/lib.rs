use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use openpay_domain::{
    AmountMinor, AttemptStatus, ConnectorCapabilities, ConnectorHealth, Currency, FailureClass,
    PaymentId, PaymentMethod,
};

pub mod sandbox;
pub use sandbox::{MemorySandboxStore, SandboxAttemptStore};

#[derive(Debug, Error)]
pub enum ConnectorError {
    #[error("connector unavailable")]
    Unavailable,
    #[error("timeout")]
    Timeout,
    #[error("payer declined: {0}")]
    Declined(String),
    #[error("ambiguous outcome")]
    Ambiguous,
    #[error("not supported")]
    NotSupported,
    #[error("{0}")]
    Message(String),
}

impl ConnectorError {
    pub fn failure_code(&self) -> &'static str {
        match self {
            Self::Unavailable => "CONNECTOR_UNAVAILABLE",
            Self::Timeout => "TIMEOUT",
            Self::Declined(_) => "PAYER_DECLINED",
            Self::Ambiguous => "AMBIGUOUS",
            Self::NotSupported => "NOT_SUPPORTED",
            Self::Message(_) => "CONNECTOR_ERROR",
        }
    }

    pub fn class(&self) -> FailureClass {
        match self {
            Self::Unavailable | Self::Timeout | Self::Message(_) => FailureClass::Technical,
            Self::Declined(_) => FailureClass::PayerDeclined,
            Self::Ambiguous => FailureClass::Ambiguous,
            Self::NotSupported => FailureClass::Technical,
        }
    }

    pub fn safe_message(&self) -> String {
        match self {
            Self::Declined(m) => format!("declined:{m}"),
            other => other.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePaymentAttemptInput {
    pub payment_id: PaymentId,
    pub amount_minor: AmountMinor,
    pub currency: Currency,
    pub method: PaymentMethod,
    pub scenario: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePaymentAttemptOutput {
    pub provider_reference: String,
    pub status: AttemptStatus,
    pub action_url: Option<String>,
    pub rail_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPaymentAttemptInput {
    pub provider_reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedAttemptStatus {
    pub provider_reference: String,
    pub status: AttemptStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelPaymentAttemptInput {
    pub provider_reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelPaymentAttemptOutput {
    pub status: AttemptStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundPaymentAttemptInput {
    pub provider_reference: String,
    pub amount_minor: AmountMinor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundPaymentAttemptOutput {
    pub status: AttemptStatus,
    pub provider_reference: String,
}

#[async_trait]
pub trait PaymentConnector: Send + Sync {
    fn key(&self) -> &str;
    fn capabilities(&self) -> ConnectorCapabilities;

    async fn health_check(&self) -> Result<ConnectorHealth, ConnectorError>;

    async fn create_attempt(
        &self,
        input: CreatePaymentAttemptInput,
    ) -> Result<CreatePaymentAttemptOutput, ConnectorError>;

    async fn fetch_attempt(
        &self,
        input: GetPaymentAttemptInput,
    ) -> Result<NormalizedAttemptStatus, ConnectorError>;

    async fn cancel_attempt(
        &self,
        input: CancelPaymentAttemptInput,
    ) -> Result<CancelPaymentAttemptOutput, ConnectorError>;

    async fn refund_attempt(
        &self,
        input: RefundPaymentAttemptInput,
    ) -> Result<RefundPaymentAttemptOutput, ConnectorError>;
}

/// Connectors that require an operator action to settle (manual-test rail).
#[async_trait]
pub trait ManualAttemptResolver: Send + Sync {
    async fn resolve(&self, provider_reference: &str, approve: bool) -> Result<(), ConnectorError>;
}

#[derive(Clone, Default)]
pub struct ConnectorRegistry {
    inner: std::collections::HashMap<String, std::sync::Arc<dyn PaymentConnector>>,
}

impl ConnectorRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, connector: std::sync::Arc<dyn PaymentConnector>) {
        self.inner.insert(connector.key().to_string(), connector);
    }

    pub fn get(&self, key: &str) -> Option<std::sync::Arc<dyn PaymentConnector>> {
        self.inner.get(key).cloned()
    }

    pub fn keys(&self) -> Vec<String> {
        self.inner.keys().cloned().collect()
    }

    pub fn all(&self) -> Vec<std::sync::Arc<dyn PaymentConnector>> {
        self.inner.values().cloned().collect()
    }
}
