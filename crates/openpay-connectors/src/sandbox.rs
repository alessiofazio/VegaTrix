use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use openpay_domain::AttemptStatus;

use crate::ConnectorError;

/// Shared sandbox attempt ledger used by mock/manual connectors.
///
/// Server and worker must share the same backend (PostgreSQL in compose).
/// In-memory is only for unit tests — process restart loses decisions.
#[async_trait]
pub trait SandboxAttemptStore: Send + Sync {
    async fn put(
        &self,
        connector_key: &str,
        provider_reference: &str,
        status: AttemptStatus,
    ) -> Result<(), ConnectorError>;

    async fn get(
        &self,
        connector_key: &str,
        provider_reference: &str,
    ) -> Result<Option<AttemptStatus>, ConnectorError>;
}

#[derive(Default)]
pub struct MemorySandboxStore {
    inner: Mutex<HashMap<(String, String), AttemptStatus>>,
}

impl MemorySandboxStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SandboxAttemptStore for MemorySandboxStore {
    async fn put(
        &self,
        connector_key: &str,
        provider_reference: &str,
        status: AttemptStatus,
    ) -> Result<(), ConnectorError> {
        self.inner
            .lock()
            .map_err(|_| ConnectorError::Message("sandbox store poisoned".into()))?
            .insert(
                (connector_key.to_string(), provider_reference.to_string()),
                status,
            );
        Ok(())
    }

    async fn get(
        &self,
        connector_key: &str,
        provider_reference: &str,
    ) -> Result<Option<AttemptStatus>, ConnectorError> {
        let map = self
            .inner
            .lock()
            .map_err(|_| ConnectorError::Message("sandbox store poisoned".into()))?;
        Ok(map
            .get(&(connector_key.to_string(), provider_reference.to_string()))
            .copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn memory_store_roundtrip() {
        let store = Arc::new(MemorySandboxStore::new());
        store
            .put("manual-test", "man_1", AttemptStatus::RequiresAction)
            .await
            .unwrap();
        assert_eq!(
            store.get("manual-test", "man_1").await.unwrap(),
            Some(AttemptStatus::RequiresAction)
        );
        store
            .put("manual-test", "man_1", AttemptStatus::Settled)
            .await
            .unwrap();
        assert_eq!(
            store.get("manual-test", "man_1").await.unwrap(),
            Some(AttemptStatus::Settled)
        );
    }
}
