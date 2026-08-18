use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

use crate::error::DomainError;
use crate::ids::{
    AttemptId, ConnectorId, IdempotencyKey, MerchantId, MerchantOrderId, PaymentId,
    RoutingPolicyId, TenantId,
};
use crate::money::{AmountMinor, Currency};
use crate::status::{AttemptStatus, PaymentMethod, PaymentStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentRequest {
    pub id: PaymentId,
    pub tenant_id: TenantId,
    pub merchant_id: MerchantId,
    pub merchant_order_id: MerchantOrderId,
    pub amount_minor: AmountMinor,
    pub currency: Currency,
    pub status: PaymentStatus,
    pub allowed_methods: Vec<PaymentMethod>,
    pub description: Option<String>,
    pub expires_at: OffsetDateTime,
    pub return_url: Option<String>,
    pub metadata: Value,
    pub idempotency_key: IdempotencyKey,
    pub routing_policy_id: Option<RoutingPolicyId>,
    pub version: i32,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl PaymentRequest {
    pub fn transition(
        &mut self,
        next: PaymentStatus,
        now: OffsetDateTime,
    ) -> Result<(), DomainError> {
        self.status = self.status.transition(next)?;
        self.updated_at = now;
        self.version += 1;
        Ok(())
    }

    pub fn belongs_to(
        &self,
        tenant_id: TenantId,
        merchant_id: MerchantId,
    ) -> Result<(), DomainError> {
        if self.tenant_id != tenant_id {
            return Err(DomainError::TenantMismatch);
        }
        if self.merchant_id != merchant_id {
            return Err(DomainError::MerchantMismatch);
        }
        Ok(())
    }

    pub fn is_expired(&self, now: OffsetDateTime) -> bool {
        now >= self.expires_at && !self.status.is_terminal() && !self.status.is_settled_family()
    }

    /// Returns `Expired` when the payment is past `expires_at` and the state machine
    /// allows that transition. PROCESSING is intentionally skipped (illegal).
    pub fn expiry_target(&self, now: OffsetDateTime) -> Option<PaymentStatus> {
        if !self.is_expired(now) {
            return None;
        }
        if self.status.can_transition_to(PaymentStatus::Expired) {
            Some(PaymentStatus::Expired)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentAttempt {
    pub id: AttemptId,
    pub tenant_id: TenantId,
    pub payment_request_id: PaymentId,
    pub connector_id: ConnectorId,
    pub connector_key: String,
    pub rail_type: String,
    pub provider_reference: Option<String>,
    pub status: AttemptStatus,
    pub failure_code: Option<String>,
    pub failure_message_safe: Option<String>,
    pub amount_minor: AmountMinor,
    pub currency: Currency,
    pub requested_at: OffsetDateTime,
    pub authorized_at: Option<OffsetDateTime>,
    pub settled_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePaymentCommand {
    pub tenant_id: TenantId,
    pub merchant_id: MerchantId,
    pub merchant_order_id: MerchantOrderId,
    pub amount_minor: AmountMinor,
    pub currency: Currency,
    pub description: Option<String>,
    pub allowed_methods: Vec<PaymentMethod>,
    pub expires_in_seconds: u32,
    pub return_url: Option<String>,
    pub metadata: Value,
    pub idempotency_key: IdempotencyKey,
    pub routing_policy_id: Option<RoutingPolicyId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionPaymentCommand {
    pub tenant_id: TenantId,
    pub payment_id: PaymentId,
    pub expected_version: Option<i32>,
    pub next_status: PaymentStatus,
    pub actor_type: String,
    pub actor_id: String,
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::{Duration, OffsetDateTime};

    fn sample(status: PaymentStatus, expires_at: OffsetDateTime) -> PaymentRequest {
        PaymentRequest {
            id: PaymentId::new(),
            tenant_id: TenantId::new(),
            merchant_id: MerchantId::new(),
            merchant_order_id: MerchantOrderId::new("ord-1").unwrap(),
            amount_minor: AmountMinor::new(1200).unwrap(),
            currency: Currency::EUR,
            status,
            allowed_methods: vec![PaymentMethod::AccountToAccount],
            description: None,
            expires_at,
            return_url: None,
            metadata: serde_json::json!({}),
            idempotency_key: IdempotencyKey::new("idem-1").unwrap(),
            routing_policy_id: None,
            version: 1,
            created_at: expires_at,
            updated_at: expires_at,
        }
    }

    #[test]
    fn pending_past_expiry_becomes_expired() {
        let now = OffsetDateTime::now_utc();
        let payment = sample(PaymentStatus::Pending, now - Duration::seconds(1));
        assert_eq!(payment.expiry_target(now), Some(PaymentStatus::Expired));
    }

    #[test]
    fn processing_past_expiry_is_not_forced() {
        let now = OffsetDateTime::now_utc();
        let payment = sample(PaymentStatus::Processing, now - Duration::seconds(1));
        assert!(payment.is_expired(now));
        assert_eq!(payment.expiry_target(now), None);
    }

    #[test]
    fn pending_before_expiry_stays() {
        let now = OffsetDateTime::now_utc();
        let payment = sample(PaymentStatus::Pending, now + Duration::seconds(60));
        assert_eq!(payment.expiry_target(now), None);
    }
}
