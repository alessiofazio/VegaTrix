//! Open banking connector skeleton.
//!
//! This crate is **not** a bank or PSP integration. It exists so the plugin
//! surface is documented and feature-gated. Enabling it without a licensed,
//! contracted provider must never claim a live rail.

use async_trait::async_trait;
use openpay_connectors::{
    CancelPaymentAttemptInput, CancelPaymentAttemptOutput, ConnectorError,
    CreatePaymentAttemptInput, CreatePaymentAttemptOutput, GetPaymentAttemptInput,
    NormalizedAttemptStatus, PaymentConnector, RefundPaymentAttemptInput,
    RefundPaymentAttemptOutput,
};
use openpay_domain::{ConnectorCapabilities, ConnectorHealth, PaymentMethod};

pub struct OpenBankingStubConnector;

#[async_trait]
impl PaymentConnector for OpenBankingStubConnector {
    fn key(&self) -> &str {
        "open-banking-stub"
    }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities {
            methods: vec![PaymentMethod::AccountToAccount],
            refunds: false,
            delayed_capture: false,
            webhooks: true,
            sandbox_only: true,
        }
    }

    async fn health_check(&self) -> Result<ConnectorHealth, ConnectorError> {
        Ok(ConnectorHealth::Unknown)
    }

    async fn create_attempt(
        &self,
        _input: CreatePaymentAttemptInput,
    ) -> Result<CreatePaymentAttemptOutput, ConnectorError> {
        Err(ConnectorError::NotSupported)
    }

    async fn fetch_attempt(
        &self,
        _input: GetPaymentAttemptInput,
    ) -> Result<NormalizedAttemptStatus, ConnectorError> {
        Err(ConnectorError::NotSupported)
    }

    async fn cancel_attempt(
        &self,
        _input: CancelPaymentAttemptInput,
    ) -> Result<CancelPaymentAttemptOutput, ConnectorError> {
        Err(ConnectorError::NotSupported)
    }

    async fn refund_attempt(
        &self,
        _input: RefundPaymentAttemptInput,
    ) -> Result<RefundPaymentAttemptOutput, ConnectorError> {
        Err(ConnectorError::NotSupported)
    }
}
