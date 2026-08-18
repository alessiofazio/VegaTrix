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
    /// Policy-ranked then catalog-priority connectors (not cost/"cheapest rail").
    #[serde(default)]
    pub ranked_connector_keys: Vec<String>,
    #[serde(default)]
    pub fallback_allowed_failure_codes: Vec<String>,
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
            allowed_failure_codes: vec!["CONNECTOR_UNAVAILABLE".into(), "TIMEOUT".into()],
        }
    }
}

impl FallbackSpec {
    pub fn allows_failure(&self, failure_code: &str) -> bool {
        self.allowed_failure_codes
            .iter()
            .any(|c| c.eq_ignore_ascii_case(failure_code))
    }

    /// Bounded retry onto the next ranked connector. Not a cheapest-rail picker.
    pub fn should_try_next(
        &self,
        failure_code: &str,
        attempts_used: u8,
        remaining_candidates: usize,
    ) -> bool {
        self.enabled
            && remaining_candidates > 0
            && attempts_used < self.max_attempts.max(1)
            && self.allows_failure(failure_code)
    }
}

#[cfg(test)]
mod fallback_tests {
    use super::FallbackSpec;

    #[test]
    fn timeout_may_advance_when_candidates_remain() {
        let spec = FallbackSpec::default();
        assert!(spec.should_try_next("TIMEOUT", 1, 1));
        assert!(spec.should_try_next("CONNECTOR_UNAVAILABLE", 1, 2));
        assert!(!spec.should_try_next("PAYER_DECLINED", 1, 1));
        assert!(!spec.should_try_next("TIMEOUT", 2, 1));
        assert!(!spec.should_try_next("TIMEOUT", 1, 0));
    }

    #[test]
    fn disabled_fallback_never_advances() {
        let spec = FallbackSpec {
            enabled: false,
            max_attempts: 3,
            allowed_failure_codes: vec!["TIMEOUT".into()],
        };
        assert!(!spec.should_try_next("TIMEOUT", 1, 2));
    }
}
