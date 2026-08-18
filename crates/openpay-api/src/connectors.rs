use std::sync::Arc;

use openpay_connectors::{ConnectorRegistry, ManualAttemptResolver};

/// Runtime handles for connector adapters that expose admin operations.
#[derive(Clone, Default)]
pub struct ConnectorRuntime {
    pub registry: ConnectorRegistry,
    pub manual: Option<Arc<dyn ManualAttemptResolver>>,
}

impl ConnectorRuntime {
    pub fn new(registry: ConnectorRegistry) -> Self {
        Self {
            registry,
            manual: None,
        }
    }

    pub fn with_manual(mut self, manual: Arc<dyn ManualAttemptResolver>) -> Self {
        self.manual = Some(manual);
        self
    }
}
