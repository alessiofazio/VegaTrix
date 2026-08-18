use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;

use openpay_connectors::{
    CancelPaymentAttemptInput, CancelPaymentAttemptOutput, ConnectorError, CreatePaymentAttemptInput,
    CreatePaymentAttemptOutput, GetPaymentAttemptInput, ManualAttemptResolver,
    NormalizedAttemptStatus, PaymentConnector, RefundPaymentAttemptInput, RefundPaymentAttemptOutput,
};
use openpay_domain::{AttemptStatus, ConnectorCapabilities, ConnectorHealth, PaymentMethod};

#[derive(Clone)]
struct ManualAttempt {
    status: AttemptStatus,
}

/// Completing a payment requires an explicit dashboard/admin action.
pub struct ManualTestConnector {
    store: Arc<Mutex<HashMap<String, ManualAttempt>>>,
}

impl ManualTestConnector {
    pub fn new() -> Self {
        Self {
            store: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn resolve(&self, provider_reference: &str, approve: bool) -> Result<(), ConnectorError> {
        let mut store = self.store.lock().await;
        let attempt = store
            .get_mut(provider_reference)
            .ok_or_else(|| ConnectorError::Message("unknown reference".into()))?;
        attempt.status = if approve {
            AttemptStatus::Settled
        } else {
            AttemptStatus::Failed
        };
        Ok(())
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
        self.store.lock().await.insert(
            provider_reference.clone(),
            ManualAttempt {
                status: AttemptStatus::RequiresAction,
            },
        );
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
