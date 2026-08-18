use std::collections::HashSet;

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
    let mut seen = HashSet::new();
    for rule in &rules {
        if rule_matches(&rule.when, context, connectors)
            && connectors.iter().any(|c| c.key == rule.select && c.enabled)
            && seen.insert(rule.select.clone())
        {
            ranked.push((
                rule.priority,
                rule.select.clone(),
                Some(format!("rule:{}", rule.select)),
            ));
        }
    }

    ranked.sort_by_key(|b| std::cmp::Reverse(b.0));

    if ranked.is_empty() {
        if let Some(best) = connectors
            .iter()
            .filter(|c| c.enabled && c.health != ConnectorHealth::Unhealthy)
            .filter(|c| method_eligible(c, context))
            .max_by_key(|c| c.priority)
        {
            seen.insert(best.key.clone());
            ranked.push((
                best.priority,
                best.key.clone(),
                Some("default_priority".into()),
            ));
        }
    }

    if fallback.enabled {
        let mut extras: Vec<(i32, String, Option<String>)> = connectors
            .iter()
            .filter(|c| c.enabled && c.health != ConnectorHealth::Unhealthy)
            .filter(|c| method_eligible(c, context))
            .filter(|c| !seen.contains(&c.key))
            .map(|c| (c.priority, c.key.clone(), Some("fallback_priority".into())))
            .collect();
        extras.sort_by_key(|b| std::cmp::Reverse(b.0));
        ranked.extend(extras);
    }

    let selected = ranked
        .first()
        .cloned()
        .ok_or_else(|| "no eligible connector".to_string())?;

    let ranked_connector_keys: Vec<String> = ranked.iter().map(|r| r.1.clone()).collect();

    Ok(RoutingDecision {
        selected_connector_key: selected.1.clone(),
        priority: selected.0,
        rule_name: selected.2,
        explanation: format!(
            "Selected connector '{}' for {} {} using policy evaluation. Fallback enabled={} max_attempts={} candidates={}.",
            selected.1,
            context.amount_minor,
            context.currency,
            fallback.enabled,
            fallback.max_attempts,
            ranked_connector_keys.join(",")
        ),
        fallback_enabled: fallback.enabled,
        fallback_remaining: fallback.max_attempts,
        ranked_connector_keys,
        fallback_allowed_failure_codes: fallback.allowed_failure_codes.clone(),
    })
}

fn method_eligible(connector: &CatalogConnector, context: &RoutingContext) -> bool {
    context
        .allowed_methods
        .iter()
        .any(|m| connector.methods.contains(m))
}

pub fn fallback_spec_from_decision(decision: &RoutingDecision) -> FallbackSpec {
    FallbackSpec {
        enabled: decision.fallback_enabled,
        max_attempts: decision.fallback_remaining,
        allowed_failure_codes: decision.fallback_allowed_failure_codes.clone(),
    }
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

    fn connector(key: &str, priority: i32, method: PaymentMethod) -> CatalogConnector {
        CatalogConnector {
            key: key.into(),
            health: ConnectorHealth::Healthy,
            methods: vec![method],
            priority,
            enabled: true,
        }
    }

    fn eur_policy(fallback: bool, max_attempts: u8) -> RoutingPolicy {
        RoutingPolicy {
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
            fallback_policy: json!({
                "enabled": fallback,
                "max_attempts": max_attempts,
                "allowed_failure_codes": ["TIMEOUT", "CONNECTOR_UNAVAILABLE"]
            }),
            created_at: time::OffsetDateTime::now_utc(),
            updated_at: time::OffsetDateTime::now_utc(),
        }
    }

    #[test]
    fn selects_mock_instant_for_eur_a2a() {
        let connectors = vec![connector(
            "mock-instant",
            100,
            PaymentMethod::AccountToAccount,
        )];
        let decision = evaluate_policy(Some(&eur_policy(true, 2)), &ctx(), &connectors).unwrap();
        assert_eq!(decision.selected_connector_key, "mock-instant");
        assert_eq!(decision.priority, 100);
        assert_eq!(decision.ranked_connector_keys, vec!["mock-instant"]);
    }

    #[test]
    fn ranks_next_enabled_connector_not_cheapest_rail() {
        let connectors = vec![
            connector("mock-instant", 100, PaymentMethod::AccountToAccount),
            connector("mock-backup", 40, PaymentMethod::AccountToAccount),
            connector("manual-test", 10, PaymentMethod::Manual),
        ];
        let decision = evaluate_policy(Some(&eur_policy(true, 2)), &ctx(), &connectors).unwrap();
        assert_eq!(
            decision.ranked_connector_keys,
            vec!["mock-instant", "mock-backup"]
        );
        assert!(
            !decision
                .ranked_connector_keys
                .iter()
                .any(|k| k == "manual-test"),
            "manual is not an A2A fallback"
        );
        let spec = fallback_spec_from_decision(&decision);
        assert!(spec.should_try_next("TIMEOUT", 1, 1));
        assert!(!spec.should_try_next("PAYER_DECLINED", 1, 1));
    }

    #[test]
    fn disabled_fallback_keeps_a_single_candidate() {
        let connectors = vec![
            connector("mock-instant", 100, PaymentMethod::AccountToAccount),
            connector("mock-backup", 40, PaymentMethod::AccountToAccount),
        ];
        let decision = evaluate_policy(Some(&eur_policy(false, 2)), &ctx(), &connectors).unwrap();
        assert_eq!(decision.ranked_connector_keys, vec!["mock-instant"]);
        assert!(!decision.fallback_enabled);
    }
}
