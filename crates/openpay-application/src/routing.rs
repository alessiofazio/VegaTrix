use openpay_domain::{
    ConnectorHealth, FallbackSpec, PaymentMethod, RoutingContext, RoutingDecision, RoutingPolicy,
    RoutingRuleSpec,
};
use serde_json::Value;

use crate::ports::ConnectorSnapshot as CatalogConnector;

pub fn evaluate_policy(
    policy: Option<&RoutingPolicy>,
    context: &RoutingContext,
    connectors: &[CatalogConnector],
) -> Result<RoutingDecision, String> {
    let fallback: FallbackSpec = policy
        .and_then(|p| serde_json::from_value(p.fallback_policy.clone()).ok())
        .unwrap_or_default();

    let rules: Vec<RoutingRuleSpec> = policy
        .map(|p| parse_rules(&p.rules_json))
        .unwrap_or_default();

    let mut ranked: Vec<(i32, String, Option<String>)> = Vec::new();
    for rule in &rules {
        if rule_matches(&rule.when, context, connectors) {
            if connectors.iter().any(|c| c.key == rule.select && c.enabled) {
                ranked.push((rule.priority, rule.select.clone(), Some(format!("rule:{}", rule.select))));
            }
        }
    }

    if ranked.is_empty() {
        if let Some(best) = connectors
            .iter()
            .filter(|c| c.enabled && c.health != ConnectorHealth::Unhealthy)
            .filter(|c| {
                context
                    .allowed_methods
                    .iter()
                    .any(|m| c.methods.contains(m) || c.methods.contains(&PaymentMethod::Manual))
            })
            .max_by_key(|c| c.priority)
        {
            ranked.push((best.priority, best.key.clone(), Some("default_priority".into())));
        }
    }

    ranked.sort_by(|a, b| b.0.cmp(&a.0));
    let selected = ranked
        .first()
        .cloned()
        .ok_or_else(|| "no eligible connector".to_string())?;

    Ok(RoutingDecision {
        selected_connector_key: selected.1.clone(),
        priority: selected.0,
        rule_name: selected.2,
        explanation: format!(
            "Selected connector '{}' for {} {} using policy evaluation. Fallback enabled={}.",
            selected.1, context.amount_minor, context.currency, fallback.enabled
        ),
        fallback_enabled: fallback.enabled,
        fallback_remaining: fallback.max_attempts,
    })
}

fn parse_rules(value: &Value) -> Vec<RoutingRuleSpec> {
    if let Some(arr) = value.get("rules").and_then(|v| v.as_array()) {
        return arr
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();
    }
    serde_json::from_value(value.clone()).unwrap_or_default()
}

fn rule_matches(
    when: &openpay_domain::RoutingWhen,
    context: &RoutingContext,
    connectors: &[CatalogConnector],
) -> bool {
    if let Some(cur) = &when.currency {
        if cur != context.currency.as_str() {
            return false;
        }
    }
    if let Some(method) = &when.method_available {
        let wanted = match method.as_str() {
            "ACCOUNT_TO_ACCOUNT" => PaymentMethod::AccountToAccount,
            "CARD" => PaymentMethod::Card,
            "WALLET" => PaymentMethod::Wallet,
            "MANUAL" => PaymentMethod::Manual,
            _ => return false,
        };
        if !context.allowed_methods.contains(&wanted) {
            return false;
        }
    }
    if let Some(health) = &when.connector_health {
        let any_healthy = connectors.iter().any(|c| match health.as_str() {
            "HEALTHY" => c.health == ConnectorHealth::Healthy,
            "DEGRADED" => c.health == ConnectorHealth::Degraded,
            _ => true,
        });
        if !any_healthy {
            return false;
        }
    }
    if let Some(min) = when.min_amount_minor {
        if context.amount_minor.get() < min {
            return false;
        }
    }
    if let Some(max) = when.max_amount_minor {
        if context.amount_minor.get() > max {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use openpay_domain::{AmountMinor, ConnectorHealth, Currency, PaymentMethod, RoutingContext};
    use serde_json::json;

    fn ctx() -> RoutingContext {
        RoutingContext {
            currency: Currency::EUR,
            amount_minor: AmountMinor::new(1200).unwrap(),
            country: "IT".into(),
            allowed_methods: vec![PaymentMethod::AccountToAccount, PaymentMethod::Card],
            merchant_preferences: json!({}),
        }
    }

    #[test]
    fn selects_mock_instant_for_eur_a2a() {
        let policy = RoutingPolicy {
            id: openpay_domain::RoutingPolicyId::new(),
            tenant_id: openpay_domain::TenantId::new(),
            name: "EUR instant preferred".into(),
            status: openpay_domain::RoutingPolicyStatus::Active,
            rules_json: json!({
                "rules": [{
                    "when": { "currency": "EUR", "method_available": "ACCOUNT_TO_ACCOUNT", "connector_health": "HEALTHY" },
                    "select": "mock-instant",
                    "priority": 100
                }]
            }),
            fallback_policy: json!({ "enabled": true, "max_attempts": 2, "allowed_failure_codes": ["TIMEOUT"] }),
            created_at: time::OffsetDateTime::now_utc(),
            updated_at: time::OffsetDateTime::now_utc(),
        };
        let connectors = vec![CatalogConnector {
            key: "mock-instant".into(),
            health: ConnectorHealth::Healthy,
            methods: vec![PaymentMethod::AccountToAccount],
            priority: 100,
            enabled: true,
        }];
        let decision = evaluate_policy(Some(&policy), &ctx(), &connectors).unwrap();
        assert_eq!(decision.selected_connector_key, "mock-instant");
        assert_eq!(decision.priority, 100);
    }
}
