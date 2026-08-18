use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use openpay_connectors::{
    CancelPaymentAttemptInput, CancelPaymentAttemptOutput, ConnectorError,
    CreatePaymentAttemptInput, CreatePaymentAttemptOutput, GetPaymentAttemptInput,
    ManualAttemptResolver, MemorySandboxStore, NormalizedAttemptStatus, PaymentConnector,
    RefundPaymentAttemptInput, RefundPaymentAttemptOutput, SandboxAttemptStore,
};
use openpay_domain::{AttemptStatus, ConnectorCapabilities, ConnectorHealth, PaymentMethod};

/// Completing a payment requires an explicit dashboard/admin action.
///
/// Decisions are stored in [`SandboxAttemptStore`] so server and worker agree
/// after restart (in-memory only for unit tests).
pub struct ManualTestConnector {
    store: Arc<dyn SandboxAttemptStore>,
}

impl ManualTestConnector {
    pub fn new() -> Self {
        Self {
            store: Arc::new(MemorySandboxStore::new()),
        }
    }

    pub fn with_store(store: Arc<dyn SandboxAttemptStore>) -> Self {
        Self { store }
    }

    pub async fn resolve(
        &self,
        provider_reference: &str,
        approve: bool,
    ) -> Result<(), ConnectorError> {
        let current = self
            .store
            .get(self.key(), provider_reference)
            .await?
            .ok_or_else(|| ConnectorError::Message("unknown reference".into()))?;
        if matches!(
            current,
            AttemptStatus::Settled | AttemptStatus::Failed | AttemptStatus::Cancelled
        ) {
            return Ok(());
        }
        let status = if approve {
            AttemptStatus::Settled
        } else {
            AttemptStatus::Failed
        };
        self.store.put(self.key(), provider_reference, status).await
    }
}

impl Default for ManualTestConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ManualAttemptResolver for ManualTestConnector {
    async fn resolve(&self, provider_reference: &str, approve: bool) -> Result<(), ConnectorError> {
        ManualTestConnector::resolve(self, provider_reference, approve).await
    }
}

#[async_trait]
impl PaymentConnector for ManualTestConnector {
    fn key(&self) -> &str {
        "manual-test"
    }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities {
            methods: vec![PaymentMethod::Manual],
            refunds: true,
            delayed_capture: true,
            webhooks: false,
            sandbox_only: true,
        }
    }

    async fn health_check(&self) -> Result<ConnectorHealth, ConnectorError> {
        Ok(ConnectorHealth::Healthy)
    }

    async fn create_attempt(
        &self,
        _input: CreatePaymentAttemptInput,
    ) -> Result<CreatePaymentAttemptOutput, ConnectorError> {
        let provider_reference = format!("man_{}", Uuid::now_v7().as_simple());
        self.store
            .put(
                self.key(),
                &provider_reference,
                AttemptStatus::RequiresAction,
            )
            .await?;
        Ok(CreatePaymentAttemptOutput {
            provider_reference,
            status: AttemptStatus::RequiresAction,
            action_url: None,
            rail_type: "MANUAL_TEST".into(),
        })
    }

    async fn fetch_attempt(
        &self,
        input: GetPaymentAttemptInput,
    ) -> Result<NormalizedAttemptStatus, ConnectorError> {
        let stored = self
            .store
            .get(self.key(), &input.provider_reference)
            .await?
            .ok_or_else(|| ConnectorError::Message("unknown reference".into()))?;
        Ok(NormalizedAttemptStatus {
            provider_reference: input.provider_reference,
            status: stored,
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

    #[tokio::test]
    async fn resolve_is_visible_to_a_new_instance() {
        let store: Arc<dyn SandboxAttemptStore> = Arc::new(MemorySandboxStore::new());
        let a = ManualTestConnector::with_store(store.clone());
        let out = a
            .create_attempt(CreatePaymentAttemptInput {
                payment_id: PaymentId::new(),
                amount_minor: AmountMinor::new(1200).unwrap(),
                currency: Currency::EUR,
                method: PaymentMethod::Manual,
                scenario: None,
                idempotency_key: "m1".into(),
            })
            .await
            .unwrap();
        let b = ManualTestConnector::with_store(store);
        b.resolve(&out.provider_reference, true).await.unwrap();
        let fetched = a
            .fetch_attempt(GetPaymentAttemptInput {
                provider_reference: out.provider_reference,
            })
            .await
            .unwrap();
        assert_eq!(fetched.status, AttemptStatus::Settled);
    }
}
