use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;

use openpay_connectors::{
    CancelPaymentAttemptInput, CancelPaymentAttemptOutput, ConnectorError,
    CreatePaymentAttemptInput, CreatePaymentAttemptOutput, GetPaymentAttemptInput,
    NormalizedAttemptStatus, PaymentConnector, RefundPaymentAttemptInput,
    RefundPaymentAttemptOutput,
};
use openpay_domain::{AttemptStatus, ConnectorCapabilities, ConnectorHealth, PaymentMethod};

#[derive(Clone, Debug)]
struct StoredAttempt {
    status: AttemptStatus,
    rail_type: String,
}

/// Sandbox connector. Scenario is taken from `input.scenario` or metadata convention:
/// `success` | `decline` | `timeout` | `unavailable` | `duplicate` | `delayed`.
pub struct MockInstantConnector {
    store: Arc<Mutex<HashMap<String, StoredAttempt>>>,
}

impl MockInstantConnector {
    pub fn new() -> Self {
        Self {
            store: Arc::new(Mutex::new(HashMap::new())),
        }
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
        self.store.lock().await.insert(
            provider_reference.clone(),
            StoredAttempt {
                status,
                rail_type: "MOCK_INSTANT".into(),
            },
        );
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
        if input.provider_reference.starts_with("timeout_") {
            return Ok(NormalizedAttemptStatus {
                provider_reference: input.provider_reference.clone(),
                status: AttemptStatus::Settled,
            });
        }
        let store = self.store.lock().await;
        let stored = store
            .get(&input.provider_reference)
            .ok_or_else(|| ConnectorError::Message("unknown reference".into()))?;
        Ok(NormalizedAttemptStatus {
            provider_reference: input.provider_reference,
            status: stored.status,
        })
    }

    async fn cancel_attempt(
        &self,
        input: CancelPaymentAttemptInput,
    ) -> Result<CancelPaymentAttemptOutput, ConnectorError> {
        let mut store = self.store.lock().await;
        if let Some(stored) = store.get_mut(&input.provider_reference) {
            stored.status = AttemptStatus::Cancelled;
            Ok(CancelPaymentAttemptOutput {
                status: AttemptStatus::Cancelled,
            })
        } else {
            Err(ConnectorError::Message("unknown reference".into()))
        }
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

    #[tokio::test]
    async fn success_settles() {
        let c = MockInstantConnector::new();
        let out = c
            .create_attempt(CreatePaymentAttemptInput {
                payment_id: PaymentId::new(),
                amount_minor: AmountMinor::new(1200).unwrap(),
                currency: Currency::EUR,
                method: PaymentMethod::AccountToAccount,
                scenario: Some("success".into()),
                idempotency_key: "k1".into(),
            })
            .await
            .unwrap();
        assert_eq!(out.status, AttemptStatus::Settled);
    }

    #[tokio::test]
    async fn decline_maps_to_error() {
        let c = MockInstantConnector::new();
        let err = c
            .create_attempt(CreatePaymentAttemptInput {
                payment_id: PaymentId::new(),
                amount_minor: AmountMinor::new(1200).unwrap(),
                currency: Currency::EUR,
                method: PaymentMethod::AccountToAccount,
                scenario: Some("decline".into()),
                idempotency_key: "k2".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(err.failure_code(), "PAYER_DECLINED");
    }
}
