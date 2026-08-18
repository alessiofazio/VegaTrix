use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::ids::{ConnectorId, TenantId};
use crate::status::PaymentMethod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorType {
    MockInstant,
    ManualTest,
    OpenBankingStub,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConnectorHealth {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorLifecycle {
    Enabled,
    Disabled,
    FeatureGated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorCapabilities {
    pub methods: Vec<PaymentMethod>,
    pub refunds: bool,
    pub delayed_capture: bool,
    pub webhooks: bool,
    pub sandbox_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connector {
    pub id: ConnectorId,
    pub tenant_id: Option<TenantId>,
    pub key: String,
    pub name: String,
    pub connector_type: ConnectorType,
    pub status: ConnectorLifecycle,
    pub configuration_ref: String,
    pub capabilities: ConnectorCapabilities,
    pub priority: i32,
    pub health_status: ConnectorHealth,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FailureClass {
    Technical,
    PayerDeclined,
    Ambiguous,
    DuplicateRisk,
}

impl FailureClass {
    pub fn allows_fallback(self) -> bool {
        matches!(self, Self::Technical)
    }
}
