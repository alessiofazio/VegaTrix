use std::sync::Arc;

use crate::ApplicationError;
use crate::payments::PaymentService;
use crate::ports::PaymentRepository;
use openpay_connectors::{CreatePaymentAttemptInput, GetPaymentAttemptInput, PaymentConnector};
use openpay_domain::{
    AttemptId, AttemptStatus, PaymentAttempt, PaymentId, PaymentMethod, PaymentRequest,
    PaymentStatus, TenantId,
};
use time::OffsetDateTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayerDecision {
    Approve,
    Reject,
}

#[derive(Debug, Clone)]
pub struct AuthorizeOutcome {
    pub payment: PaymentRequest,
    pub connector_key: String,
    pub explanation: String,
    pub idempotent_replay: bool,
}

fn terminal(status: PaymentStatus) -> bool {
    matches!(
        status,
        PaymentStatus::Settled
            | PaymentStatus::Failed
            | PaymentStatus::Cancelled
            | PaymentStatus::Expired
            | PaymentStatus::Refunded
    )
}

/// Authorize or reject a payment from the demo wallet / public API.
pub async fn authorize_payment<P, M, A, O, R, C, K, Cl>(
    payments: &PaymentService<P, M, A, O, R, C, K, Cl>,
    connectors: &dyn ConnectorLookup,
    tenant_id: TenantId,
    payment_id: PaymentId,
    decision: PayerDecision,
    scenario: Option<String>,
) -> Result<AuthorizeOutcome, ApplicationError>
where
    P: PaymentRepository,
    M: crate::ports::MerchantRepository,
    A: crate::ports::AuditRepository,
    O: crate::ports::OutboxRepository,
    R: crate::ports::RoutingRepository,
    C: crate::ports::ConnectorCatalog,
    K: crate::ports::QrNonceStore,
    Cl: crate::ports::Clock,
{
    let payment = payments.get_payment(tenant_id, payment_id).await?;
    if terminal(payment.status) {
        return Ok(AuthorizeOutcome {
            payment,
            connector_key: String::new(),
            explanation: "payment already terminal".into(),
            idempotent_replay: true,
        });
    }

    if decision == PayerDecision::Reject {
        let updated = payments
            .apply_status(
                &payment,
                PaymentStatus::Failed,
                "payer",
                "demo-wallet",
                "payment.failed",
            )
            .await?;
        return Ok(AuthorizeOutcome {
            payment: updated,
            connector_key: String::new(),
            explanation: "payer rejected".into(),
            idempotent_replay: false,
        });
    }

    let processing = if payment.status == PaymentStatus::Processing {
        payment.clone()
    } else {
        payments
            .apply_status(
                &payment,
                PaymentStatus::Processing,
                "payer",
                "demo-wallet",
                "payment.processing",
            )
            .await?
    };

    let route = payments.decide_route(&processing).await?;
    let fallback = crate::routing::fallback_spec_from_decision(&route);
    let mut candidates = route.ranked_connector_keys.clone();
    if candidates.is_empty() {
        candidates.push(route.selected_connector_key.clone());
    }
    let max_attempts = if fallback.enabled {
        (fallback.max_attempts as usize).clamp(1, 8)
    } else {
        1
    };
    let keys: Vec<String> = candidates.into_iter().take(max_attempts).collect();

    let scenario = scenario.or_else(|| {
        processing
            .metadata
            .get("scenario")
            .and_then(|v| v.as_str())
            .map(str::to_string)
    });

    let mut last_status = PaymentStatus::Processing;
    let mut last_key = route.selected_connector_key.clone();
    let mut tried: Vec<String> = Vec::new();

    for (index, key) in keys.iter().enumerate() {
        last_key = key.clone();
        tried.push(key.clone());
        let connector = match connectors.get(key) {
            Some(c) => c,
            None => {
                last_status = PaymentStatus::Failed;
                let remaining = keys.len().saturating_sub(index + 1);
                if fallback.should_try_next("CONNECTOR_UNAVAILABLE", (index as u8) + 1, remaining) {
                    continue;
                }
                break;
            }
        };
        let connector_id = payments
            .connectors
            .connector_id_by_key(tenant_id, connector.key())
            .await?;

        let now = OffsetDateTime::now_utc();
        let attempt_result = connector
            .create_attempt(CreatePaymentAttemptInput {
                payment_id: processing.id,
                amount_minor: processing.amount_minor,
                currency: processing.currency,
                method: processing
                    .allowed_methods
                    .first()
                    .copied()
                    .unwrap_or(PaymentMethod::AccountToAccount),
                scenario: scenario.clone(),
                idempotency_key: format!("attempt-{}-{}", processing.id, connector.key()),
            })
            .await;

        let mut attempt = PaymentAttempt {
            id: AttemptId::new(),
            tenant_id: processing.tenant_id,
            payment_request_id: processing.id,
            connector_id,
            connector_key: connector.key().to_string(),
            rail_type: "UNKNOWN".into(),
            provider_reference: None,
            status: AttemptStatus::Processing,
            failure_code: None,
            failure_message_safe: None,
            amount_minor: processing.amount_minor,
            currency: processing.currency,
            requested_at: now,
            authorized_at: None,
            settled_at: None,
            created_at: now,
            updated_at: now,
        };

        let (next_status, failure_code) = match attempt_result {
            Ok(out) => {
                attempt.provider_reference = Some(out.provider_reference);
                attempt.status = out.status;
                attempt.rail_type = out.rail_type;
                if attempt.status == AttemptStatus::Settled {
                    attempt.settled_at = Some(now);
                }
                let status = if attempt.status == AttemptStatus::RequiresAction {
                    PaymentStatus::RequiresAction
                } else {
                    attempt.status.into_payment_status()
                };
                (status, None)
            }
            Err(err) => {
                if err.failure_code() == "TIMEOUT" {
                    attempt.provider_reference = Some(format!("timeout_{}", processing.id));
                }
                attempt.status = if err.failure_code() == "TIMEOUT" {
                    AttemptStatus::Ambiguous
                } else {
                    AttemptStatus::Failed
                };
                attempt.failure_code = Some(err.failure_code().into());
                attempt.failure_message_safe = Some(err.safe_message());
                (
                    attempt.status.into_payment_status(),
                    Some(err.failure_code().to_string()),
                )
            }
        };

        payments.payments.insert_attempt(attempt).await?;
        last_status = next_status;

        let remaining = keys.len().saturating_sub(index + 1);
        if let Some(code) = failure_code.as_deref() {
            if fallback.should_try_next(code, (index as u8) + 1, remaining) {
                continue;
            }
        }
        break;
    }

    let event = last_status.webhook_event().unwrap_or("payment.processing");
    let updated = payments
        .apply_status(&processing, last_status, "connector", &last_key, event)
        .await?;

    Ok(AuthorizeOutcome {
        payment: updated,
        connector_key: last_key,
        explanation: format!(
            "{} Tried connectors [{}].",
            route.explanation,
            tried.join(", ")
        ),
        idempotent_replay: false,
    })
}

/// Replay a connector callback for an already-settled attempt (duplicate callback demo).
pub async fn replay_connector_callback<P, M, A, O, R, C, K, Cl>(
    payments: &PaymentService<P, M, A, O, R, C, K, Cl>,
    connectors: &dyn ConnectorLookup,
    tenant_id: TenantId,
    payment_id: PaymentId,
) -> Result<AuthorizeOutcome, ApplicationError>
where
    P: PaymentRepository,
    M: crate::ports::MerchantRepository,
    A: crate::ports::AuditRepository,
    O: crate::ports::OutboxRepository,
    R: crate::ports::RoutingRepository,
    C: crate::ports::ConnectorCatalog,
    K: crate::ports::QrNonceStore,
    Cl: crate::ports::Clock,
{
    let _payment = payments.get_payment(tenant_id, payment_id).await?;
    let attempts = payments
        .payments
        .list_attempts(tenant_id, payment_id)
        .await?;
    let attempt = attempts
        .last()
        .ok_or(ApplicationError::Connector("no attempt to replay".into()))?;
    let provider_ref = attempt
        .provider_reference
        .clone()
        .ok_or(ApplicationError::Connector(
            "missing provider reference".into(),
        ))?;

    let connector = connectors
        .get(&attempt.connector_key)
        .ok_or_else(|| ApplicationError::Connector("connector missing".into()))?;

    let fetched = connector
        .fetch_attempt(GetPaymentAttemptInput {
            provider_reference: provider_ref.clone(),
        })
        .await
        .map_err(|e| ApplicationError::Connector(e.to_string()))?;

    let _ = fetched;
    let current = payments.get_payment(tenant_id, payment_id).await?;
    Ok(AuthorizeOutcome {
        payment: current,
        connector_key: attempt.connector_key.clone(),
        explanation: format!("duplicate callback ignored for {provider_ref}"),
        idempotent_replay: true,
    })
}

/// Poll ambiguous/processing attempts and finalize payment state.
pub async fn reconcile_payment<P, M, A, O, R, C, K, Cl>(
    payments: &PaymentService<P, M, A, O, R, C, K, Cl>,
    connectors: &dyn ConnectorLookup,
    tenant_id: TenantId,
    payment_id: PaymentId,
) -> Result<PaymentRequest, ApplicationError>
where
    P: PaymentRepository,
    M: crate::ports::MerchantRepository,
    A: crate::ports::AuditRepository,
    O: crate::ports::OutboxRepository,
    R: crate::ports::RoutingRepository,
    C: crate::ports::ConnectorCatalog,
    K: crate::ports::QrNonceStore,
    Cl: crate::ports::Clock,
{
    let payment = payments.get_payment(tenant_id, payment_id).await?;
    if payment.status != PaymentStatus::Processing {
        return Ok(payment);
    }
    let attempts = payments
        .payments
        .list_attempts(tenant_id, payment_id)
        .await?;
    let attempt = reconcilable_attempt(&attempts)
        .ok_or(ApplicationError::Connector("nothing to reconcile".into()))?;

    let provider_ref = attempt
        .provider_reference
        .clone()
        .ok_or(ApplicationError::Connector(
            "missing provider reference".into(),
        ))?;
    let connector = connectors
        .get(&attempt.connector_key)
        .ok_or_else(|| ApplicationError::Connector("connector missing".into()))?;

    let fetched = connector
        .fetch_attempt(GetPaymentAttemptInput {
            provider_reference: provider_ref,
        })
        .await
        .map_err(|e| ApplicationError::Connector(e.to_string()))?;

    let next = fetched.status.into_payment_status();
    if next == payment.status {
        return Ok(payment);
    }
    let event = next.webhook_event().unwrap_or("payment.processing");
    payments
        .apply_status(&payment, next, "reconciler", "worker", event)
        .await
}

pub async fn reconcile_stale_attempts<P, M, A, O, R, C, K, Cl>(
    payments: &PaymentService<P, M, A, O, R, C, K, Cl>,
    connectors: &dyn ConnectorLookup,
    limit: i64,
) -> Result<u32, ApplicationError>
where
    P: PaymentRepository,
    M: crate::ports::MerchantRepository,
    A: crate::ports::AuditRepository,
    O: crate::ports::OutboxRepository,
    R: crate::ports::RoutingRepository,
    C: crate::ports::ConnectorCatalog,
    K: crate::ports::QrNonceStore,
    Cl: crate::ports::Clock,
{
    let rows = payments.payments.list_reconcilable_payments(limit).await?;
    let mut n = 0u32;
    for (tenant_id, payment_id) in rows {
        if reconcile_payment(payments, connectors, tenant_id, payment_id)
            .await
            .is_ok()
        {
            n += 1;
        }
    }
    Ok(n)
}

pub trait ConnectorLookup: Send + Sync {
    fn get(&self, key: &str) -> Option<Arc<dyn PaymentConnector>>;
}

impl ConnectorLookup for openpay_connectors::ConnectorRegistry {
    fn get(&self, key: &str) -> Option<Arc<dyn PaymentConnector>> {
        openpay_connectors::ConnectorRegistry::get(self, key)
    }
}

/// Latest AMBIGUOUS or PROCESSING attempt — expire must not steal these; worker polls them.
pub fn reconcilable_attempt(attempts: &[PaymentAttempt]) -> Option<&PaymentAttempt> {
    attempts.iter().rev().find(|a| {
        matches!(
            a.status,
            AttemptStatus::Ambiguous | AttemptStatus::Processing
        )
    })
}

#[cfg(test)]
mod tests {
    use super::reconcilable_attempt;
    use openpay_domain::{
        AmountMinor, AttemptId, AttemptStatus, ConnectorId, Currency, PaymentAttempt, PaymentId,
        TenantId,
    };
    use time::OffsetDateTime;

    fn attempt(status: AttemptStatus) -> PaymentAttempt {
        let now = OffsetDateTime::now_utc();
        PaymentAttempt {
            id: AttemptId::new(),
            tenant_id: TenantId::new(),
            payment_request_id: PaymentId::new(),
            connector_id: ConnectorId::new(),
            connector_key: "mock-instant".into(),
            rail_type: "MOCK".into(),
            provider_reference: Some("timeout_pay_1".into()),
            status,
            failure_code: Some("TIMEOUT".into()),
            failure_message_safe: None,
            amount_minor: AmountMinor::new(1200).unwrap(),
            currency: Currency::EUR,
            requested_at: now,
            authorized_at: None,
            settled_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn processing_ambiguous_is_reconcilable() {
        let rows = vec![
            attempt(AttemptStatus::Failed),
            attempt(AttemptStatus::Ambiguous),
        ];
        let found = reconcilable_attempt(&rows).expect("ambiguous");
        assert_eq!(found.status, AttemptStatus::Ambiguous);
    }

    #[test]
    fn settled_only_is_not_reconcilable() {
        assert!(reconcilable_attempt(&[attempt(AttemptStatus::Settled)]).is_none());
    }
}
