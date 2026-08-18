use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;

use openpay_connectors::{
    CancelPaymentAttemptInput, CancelPaymentAttemptOutput, ConnectorError,
    CreatePaymentAttemptInput, CreatePaymentAttemptOutput, GetPaymentAttemptInput,
    MemorySandboxStore, NormalizedAttemptStatus, PaymentConnector, RefundPaymentAttemptInput,
    RefundPaymentAttemptOutput, SandboxAttemptStore,
};
use openpay_domain::{AttemptStatus, ConnectorCapabilities, ConnectorHealth, PaymentMethod};

/// Sandbox connector. Scenario is taken from `input.scenario` or metadata convention:
/// `success` | `decline` | `timeout` | `unavailable` | `duplicate` | `delayed`.
///
/// Attempt outcomes live in [`SandboxAttemptStore`] so server and worker share state.
pub struct MockInstantConnector {
    store: Arc<dyn SandboxAttemptStore>,
}

impl MockInstantConnector {
    pub fn new() -> Self {
        Self {
            store: Arc::new(MemorySandboxStore::new()),
        }
    }

    pub fn with_store(store: Arc<dyn SandboxAttemptStore>) -> Self {
        Self { store }
    }
}

impl Default for MockInstantConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PaymentConnector for MockInstantConnector {
    fn key(&self) -> &str {
        "mock-instant"
    }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities {
            methods: vec![PaymentMethod::AccountToAccount, PaymentMethod::Wallet],
            refunds: true,
            delayed_capture: false,
            webhooks: true,
            sandbox_only: true,
        }
    }

    async fn health_check(&self) -> Result<ConnectorHealth, ConnectorError> {
        Ok(ConnectorHealth::Healthy)
    }

    async fn create_attempt(
        &self,
        input: CreatePaymentAttemptInput,
    ) -> Result<CreatePaymentAttemptOutput, ConnectorError> {
        let scenario = input.scenario.as_deref().unwrap_or("success");
        match scenario {
            "unavailable" => return Err(ConnectorError::Unavailable),
            "timeout" => {
                tokio::time::sleep(Duration::from_millis(50)).await;
                let provider_reference = format!("timeout_{}", input.payment_id);
                self.store
                    .put(self.key(), &provider_reference, AttemptStatus::Ambiguous)
                    .await?;
                return Err(ConnectorError::Timeout);
            }
            "decline" => return Err(ConnectorError::Declined("sandbox_declined".into())),
            _ => {}
        }

        let provider_reference = format!("mock_{}", Uuid::now_v7().as_simple());
        let status = match scenario {
            "delayed" => AttemptStatus::Processing,
            "duplicate" => AttemptStatus::Settled,
            _ => AttemptStatus::Settled,
        };
        self.store
            .put(self.key(), &provider_reference, status)
            .await?;
        Ok(CreatePaymentAttemptOutput {
            provider_reference,
            status,
            action_url: None,
            rail_type: "MOCK_INSTANT".into(),
        })
    }

    async fn fetch_attempt(
        &self,
        input: GetPaymentAttemptInput,
    ) -> Result<NormalizedAttemptStatus, ConnectorError> {
        // Synthetic timeout_* refs from older authorize paths, and persisted Ambiguous rows.
        if input.provider_reference.starts_with("timeout_") {
            return Ok(NormalizedAttemptStatus {
                provider_reference: input.provider_reference.clone(),
                status: AttemptStatus::Settled,
            });
        }
        let stored = self
            .store
            .get(self.key(), &input.provider_reference)
            .await?
            .ok_or_else(|| ConnectorError::Message("unknown reference".into()))?;
        let status = if stored == AttemptStatus::Processing || stored == AttemptStatus::Ambiguous {
            let settled = AttemptStatus::Settled;
            self.store
                .put(self.key(), &input.provider_reference, settled)
                .await?;
            settled
        } else {
            stored
        };
        Ok(NormalizedAttemptStatus {
            provider_reference: input.provider_reference,
            status,
        })
    }

    async fn cancel_attempt(
        &self,
        input: CancelPaymentAttemptInput,
    ) -> Result<CancelPaymentAttemptOutput, ConnectorError> {
        if self
            .store
            .get(self.key(), &input.provider_reference)
            .await?
            .is_some()
        {
            self.store
                .put(
                    self.key(),
                    &input.provider_reference,
                    AttemptStatus::Cancelled,
                )
                .await?;
            return Ok(CancelPaymentAttemptOutput {
                status: AttemptStatus::Cancelled,
            });
        }
        Err(ConnectorError::Message("unknown reference".into()))
    }

    async fn refund_attempt(
        &self,
        input: RefundPaymentAttemptInput,
    ) -> Result<RefundPaymentAttemptOutput, ConnectorError> {
        Ok(RefundPaymentAttemptOutput {
            status: AttemptStatus::Settled,
            provider_reference: format!("refund_{}", input.provider_reference),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openpay_domain::{AmountMinor, Currency, PaymentId, PaymentMethod};

    fn input(scenario: &str, key: &str) -> CreatePaymentAttemptInput {
        CreatePaymentAttemptInput {
            payment_id: PaymentId::new(),
            amount_minor: AmountMinor::new(1200).unwrap(),
            currency: Currency::EUR,
            method: PaymentMethod::AccountToAccount,
            scenario: Some(scenario.into()),
            idempotency_key: key.into(),
        }
    }

    #[tokio::test]
    async fn success_settles() {
        let c = MockInstantConnector::new();
        let out = c.create_attempt(input("success", "k1")).await.unwrap();
        assert_eq!(out.status, AttemptStatus::Settled);
    }

    #[tokio::test]
    async fn decline_maps_to_error() {
        let c = MockInstantConnector::new();
        let err = c.create_attempt(input("decline", "k2")).await.unwrap_err();
        assert_eq!(err.failure_code(), "PAYER_DECLINED");
    }

    #[tokio::test]
    async fn delayed_and_timeout_survive_new_connector_instance() {
        let store: Arc<dyn SandboxAttemptStore> = Arc::new(MemorySandboxStore::new());
        let a = MockInstantConnector::with_store(store.clone());
        let delayed = a.create_attempt(input("delayed", "k3")).await.unwrap();
        assert_eq!(delayed.status, AttemptStatus::Processing);

        let timeout_err = a.create_attempt(input("timeout", "k4")).await.unwrap_err();
        assert_eq!(timeout_err.failure_code(), "TIMEOUT");

        let b = MockInstantConnector::with_store(store);
        let fetched = b
            .fetch_attempt(GetPaymentAttemptInput {
                provider_reference: delayed.provider_reference.clone(),
            })
            .await
            .unwrap();
        assert_eq!(fetched.status, AttemptStatus::Settled);
    }
}
