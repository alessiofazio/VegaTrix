use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

use crate::ids::{RoutingPolicyId, TenantId};
use crate::money::{AmountMinor, Currency};
use crate::status::PaymentMethod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingPolicyStatus {
    Active,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingPolicy {
    pub id: RoutingPolicyId,
    pub tenant_id: TenantId,
    pub name: String,
    pub status: RoutingPolicyStatus,
    pub rules_json: Value,
    pub fallback_policy: Value,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingContext {
    pub currency: Currency,
    pub amount_minor: AmountMinor,
    pub country: String,
    pub allowed_methods: Vec<PaymentMethod>,
    pub merchant_preferences: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub selected_connector_key: String,
    pub priority: i32,
    pub rule_name: Option<String>,
    pub explanation: String,
    pub fallback_enabled: bool,
    pub fallback_remaining: u8,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoutingRuleSpec {
    pub when: RoutingWhen,
    pub select: String,
    pub priority: i32,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RoutingWhen {
    pub currency: Option<String>,
    pub method_available: Option<String>,
    pub connector_health: Option<String>,
    pub min_amount_minor: Option<i64>,
    pub max_amount_minor: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FallbackSpec {
    pub enabled: bool,
    pub max_attempts: u8,
    pub allowed_failure_codes: Vec<String>,
}

impl Default for FallbackSpec {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: 2,
            allowed_failure_codes: vec![
                "CONNECTOR_UNAVAILABLE".into(),
                "TIMEOUT".into(),
            ],
        }
    }
}
