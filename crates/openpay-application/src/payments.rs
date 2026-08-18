use serde_json::json;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use openpay_crypto::{QrClaims, generate_nonce, qr_uri};
use openpay_domain::{
    AmountMinor, AuditEvent, CreatePaymentCommand, Currency, DomainError, EventId, MerchantOrderId,
    PaymentId, PaymentMethod, PaymentRequest, PaymentStatus, RoutingContext, TenantId,
    TransitionPaymentCommand,
};

use crate::ports::{
    AuditRepository, Clock, ConnectorCatalog, CreatePaymentResult, IdempotencyContext,
    MerchantRepository, OutboxRecord, OutboxRepository, PaymentRepository, QrNonceStore,
    RepositoryError, RoutingRepository,
};
use crate::routing::evaluate_policy;

#[derive(Debug, thiserror::Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error("routing: {0}")]
    Routing(String),
    #[error("forbidden")]
    Forbidden,
    #[error("expired token")]
    Expired,
    #[error("replayed token")]
    Replay,
    #[error("connector: {0}")]
    Connector(String),
}

#[derive(Clone)]
pub struct PaymentService<P, M, A, O, R, C, K, Cl>
where
    P: PaymentRepository,
    M: MerchantRepository,
    A: AuditRepository,
    O: OutboxRepository,
    R: RoutingRepository,
    C: ConnectorCatalog,
    K: QrNonceStore,
    Cl: Clock,
{
    pub payments: P,
    pub merchants: M,
    pub audit: A,
    pub outbox: O,
    pub routing: R,
    pub connectors: C,
    pub nonces: K,
    pub clock: Cl,
    pub qr_secret: Vec<u8>,
    pub api_base_url: String,
    pub wallet_base_url: String,
    pub qr_ttl_seconds: i64,
}

#[derive(Debug, Clone)]
pub struct CreatedPaymentView {
    pub payment: PaymentRequest,
    pub replayed: bool,
    pub payment_url: String,
    pub qr_payload: String,
    pub qr_svg: String,
}

impl<P, M, A, O, R, C, K, Cl> PaymentService<P, M, A, O, R, C, K, Cl>
where
    P: PaymentRepository,
    M: MerchantRepository,
    A: AuditRepository,
    O: OutboxRepository,
    R: RoutingRepository,
    C: ConnectorCatalog,
    K: QrNonceStore,
    Cl: Clock,
{
    pub async fn create_payment(
        &self,
        cmd: CreatePaymentCommand,
        request_id: Option<String>,
    ) -> Result<CreatedPaymentView, ApplicationError> {
        let now = self.clock.now();
        let merchant = self
            .merchants
            .get_merchant(cmd.tenant_id, cmd.merchant_id)
            .await?;
        let fingerprint = request_fingerprint(&cmd);
        let expires_at = now + Duration::seconds(cmd.expires_in_seconds.max(30) as i64);
        let payment = PaymentRequest {
            id: PaymentId::new(),
            tenant_id: cmd.tenant_id,
            merchant_id: cmd.merchant_id,
            merchant_order_id: cmd.merchant_order_id.clone(),
            amount_minor: cmd.amount_minor,
            currency: cmd.currency,
            status: PaymentStatus::Pending,
            allowed_methods: if cmd.allowed_methods.is_empty() {
                vec![PaymentMethod::AccountToAccount]
            } else {
                cmd.allowed_methods.clone()
            },
            description: cmd.description.clone(),
            expires_at,
            return_url: cmd.return_url.clone(),
            metadata: cmd.metadata.clone(),
            idempotency_key: cmd.idempotency_key.clone(),
            routing_policy_id: cmd.routing_policy_id,
            version: 1,
            created_at: now,
            updated_at: now,
        };

        let audit_event = audit_event(
            cmd.tenant_id,
            "merchant",
            &merchant.id.as_prefixed(),
            "payment.created",
            "payment_request",
            &payment.id.as_prefixed(),
            request_id,
            json!({
                "merchant_order_id": payment.merchant_order_id.as_str(),
                "amount_minor": payment.amount_minor.get(),
                "currency": payment.currency.as_str()
            }),
            now,
        );
        let outbox = payment_outbox(&payment, "payment.created", now);
        let CreatePaymentResult { payment, replayed } = self
            .payments
            .create_idempotent(
                payment,
                IdempotencyContext {
                    tenant_id: cmd.tenant_id,
                    key: cmd.idempotency_key.as_str().to_string(),
                    request_fingerprint: fingerprint,
                },
                audit_event,
                outbox,
            )
            .await?;

        Ok(self.present(&payment, replayed)?)
    }

    pub async fn get_payment(
        &self,
        tenant_id: TenantId,
        payment_id: PaymentId,
    ) -> Result<PaymentRequest, ApplicationError> {
        let payment = self.payments.get_by_id(tenant_id, payment_id).await?;
        let now = self.clock.now();
        if let Some(next) = payment.expiry_target(now) {
            return self
                .apply_status(&payment, next, "system", "expiry", "payment.expired")
                .await;
        }
        Ok(payment)
    }

    pub async fn cancel_payment(
        &self,
        tenant_id: TenantId,
        payment_id: PaymentId,
        actor: &str,
    ) -> Result<PaymentRequest, ApplicationError> {
        let payment = self.payments.get_by_id(tenant_id, payment_id).await?;
        payment
            .status
            .transition(PaymentStatus::Cancelled)
            .map_err(ApplicationError::Domain)?;
        self.apply_status(
            &payment,
            PaymentStatus::Cancelled,
            "merchant",
            actor,
            "payment.cancelled",
        )
        .await
    }

    pub async fn refund_payment(
        &self,
        tenant_id: TenantId,
        payment_id: PaymentId,
        actor: &str,
    ) -> Result<PaymentRequest, ApplicationError> {
        let payment = self.payments.get_by_id(tenant_id, payment_id).await?;
        if payment.status != PaymentStatus::Settled
            && payment.status != PaymentStatus::PartiallyRefunded
        {
            return Err(ApplicationError::Domain(DomainError::NotRefundable(
                payment.status.as_str().into(),
            )));
        }
        self.apply_status(
            &payment,
            PaymentStatus::Refunded,
            "merchant",
            actor,
            "payment.refunded",
        )
        .await
    }

    pub async fn apply_status(
        &self,
        payment: &PaymentRequest,
        next: PaymentStatus,
        actor_type: &str,
        actor_id: &str,
        event_type: &str,
    ) -> Result<PaymentRequest, ApplicationError> {
        let now = self.clock.now();
        let event_type = event_type.to_string();
        let updated = self
            .payments
            .transition_payment(
                TransitionPaymentCommand {
                    tenant_id: payment.tenant_id,
                    payment_id: payment.id,
                    expected_version: Some(payment.version),
                    next_status: next,
                    actor_type: actor_type.into(),
                    actor_id: actor_id.into(),
                    reason: Some(event_type.clone()),
                },
                now,
                Some(audit_event(
                    payment.tenant_id,
                    actor_type,
                    actor_id,
                    &event_type,
                    "payment_request",
                    &payment.id.as_prefixed(),
                    None,
                    json!({ "from": payment.status.as_str(), "to": next.as_str() }),
                    now,
                )),
                Some(payment_outbox_with_status(payment, &event_type, next, now)),
            )
            .await?;
        Ok(updated)
    }

    pub async fn expire_stale_payments(&self, limit: i64) -> Result<u32, ApplicationError> {
        let now = self.clock.now();
        let due = self.payments.list_expirable_payments(now, limit).await?;
        let mut n = 0u32;
        for payment in due {
            let Some(next) = payment.expiry_target(now) else {
                continue;
            };
            if self
                .apply_status(&payment, next, "system", "expiry", "payment.expired")
                .await
                .is_ok()
            {
                n += 1;
            }
        }
        Ok(n)
    }

    pub async fn decide_route(
        &self,
        payment: &PaymentRequest,
    ) -> Result<openpay_domain::RoutingDecision, ApplicationError> {
        let policy = self.routing.get_active_policy(payment.tenant_id).await?;
        let connectors = self.connectors.list_enabled(payment.tenant_id).await?;
        let ctx = RoutingContext {
            currency: payment.currency,
            amount_minor: payment.amount_minor,
            country: "IT".into(),
            allowed_methods: payment.allowed_methods.clone(),
            merchant_preferences: json!({}),
        };
        evaluate_policy(policy.as_ref(), &ctx, &connectors).map_err(ApplicationError::Routing)
    }

    pub fn present(
        &self,
        payment: &PaymentRequest,
        replayed: bool,
    ) -> Result<CreatedPaymentView, ApplicationError> {
        let token = QrClaims::new(
            payment.id,
            payment.tenant_id,
            payment.merchant_id,
            payment.expires_at,
            generate_nonce(),
        )
        .encode(&self.qr_secret)
        .map_err(|e| ApplicationError::Connector(e.to_string()))?;
        let qr_payload = qr_uri(payment.id, &token);
        let qr_svg = render_qr_svg(&qr_payload);
        let payment_url = format!(
            "{}/?payment={}&token={}",
            self.wallet_base_url.trim_end_matches('/'),
            payment.id.as_prefixed(),
            token
        );
        Ok(CreatedPaymentView {
            payment: payment.clone(),
            replayed,
            payment_url,
            qr_payload,
            qr_svg,
        })
    }
}

fn request_fingerprint(cmd: &CreatePaymentCommand) -> String {
    let raw = format!(
        "{}|{}|{}|{}|{}",
        cmd.merchant_id,
        cmd.merchant_order_id.as_str(),
        cmd.amount_minor.get(),
        cmd.currency.as_str(),
        cmd.idempotency_key.as_str()
    );
    openpay_crypto::sha256_hex(raw.as_bytes())
}

#[allow(clippy::too_many_arguments)]
fn audit_event(
    tenant_id: TenantId,
    actor_type: &str,
    actor_id: &str,
    event_type: &str,
    resource_type: &str,
    resource_id: &str,
    request_id: Option<String>,
    metadata_redacted: serde_json::Value,
    now: OffsetDateTime,
) -> AuditEvent {
    AuditEvent {
        id: openpay_domain::AuditId::new(),
        tenant_id,
        actor_type: actor_type.into(),
        actor_id: actor_id.into(),
        event_type: event_type.into(),
        resource_type: resource_type.into(),
        resource_id: resource_id.into(),
        request_id,
        ip_hash: None,
        metadata_redacted,
        occurred_at: now,
    }
}

pub fn payment_outbox(
    payment: &PaymentRequest,
    event_type: &str,
    now: OffsetDateTime,
) -> OutboxRecord {
    payment_outbox_with_status(payment, event_type, payment.status, now)
}

pub fn payment_outbox_with_status(
    payment: &PaymentRequest,
    event_type: &str,
    status: PaymentStatus,
    now: OffsetDateTime,
) -> OutboxRecord {
    let event_id = EventId::new();
    OutboxRecord {
        id: event_id.as_prefixed(),
        tenant_id: payment.tenant_id,
        aggregate_type: "payment_request".into(),
        aggregate_id: payment.id.as_prefixed(),
        event_type: event_type.into(),
        payload: json!({
            "id": event_id.as_prefixed(),
            "type": event_type,
            "api_version": "2026-08-18",
            "created_at": now,
            "data": {
                "payment_id": payment.id.as_prefixed(),
                "merchant_order_id": payment.merchant_order_id.as_str(),
                "status": status.as_str(),
                "amount_minor": payment.amount_minor.get(),
                "currency": payment.currency.as_str(),
                "merchant_id": payment.merchant_id.as_prefixed()
            }
        }),
        created_at: now,
    }
}

pub fn render_qr_svg(payload: &str) -> String {
    let qr = qrcode::QrCode::new(payload.as_bytes())
        .unwrap_or_else(|_| qrcode::QrCode::new(b"openpay://invalid").expect("fallback qr"));
    qr.render::<qrcode::render::svg::Color<'_>>()
        .min_dimensions(256, 256)
        .build()
}

pub fn parse_currency(raw: &str) -> Result<Currency, ApplicationError> {
    raw.parse().map_err(ApplicationError::Domain)
}

pub fn parse_amount(raw: i64) -> Result<AmountMinor, ApplicationError> {
    AmountMinor::new(raw).map_err(ApplicationError::Domain)
}

pub fn parse_order_id(raw: &str) -> Result<MerchantOrderId, ApplicationError> {
    MerchantOrderId::new(raw).map_err(ApplicationError::Domain)
}

pub fn fingerprint_for_test(
    merchant: &str,
    order: &str,
    amount: i64,
    currency: &str,
    key: &str,
) -> String {
    let raw = format!("{merchant}|{order}|{amount}|{currency}|{key}");
    openpay_crypto::sha256_hex(raw.as_bytes())
}

pub fn new_request_id() -> String {
    format!("req_{}", Uuid::now_v7().as_simple())
}
