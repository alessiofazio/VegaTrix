use openpay_application::{
    ApiKeyRecord, AuthUser, ConnectorSnapshot, OutboxRecord, RepositoryError,
};
use openpay_domain::{
    AmountMinor, AttemptId, AttemptStatus, AuditEvent, ConnectorHealth, ConnectorId, Currency,
    DeliveryStatus, IdempotencyKey, Merchant, MerchantId, MerchantOrderId, MerchantStatus,
    PaymentAttempt, PaymentId, PaymentMethod, PaymentRequest, PaymentStatus, Plan, RoutingPolicy,
    RoutingPolicyId, RoutingPolicyStatus, Tenant, TenantId, TenantStatus, WebhookDelivery,
    WebhookEndpoint, WebhookEndpointId, WebhookEndpointStatus,
};
use serde_json::Value;
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(FromRow)]
pub struct TenantRow {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub status: String,
    pub plan: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl TryFrom<TenantRow> for Tenant {
    type Error = RepositoryError;
    fn try_from(row: TenantRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: TenantId::from_uuid(row.id),
            name: row.name,
            slug: row.slug,
            status: parse_tenant_status(&row.status)?,
            plan: parse_plan(&row.plan)?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(FromRow)]
pub struct MerchantRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub legal_name: String,
    pub display_name: String,
    pub merchant_reference: String,
    pub country: String,
    pub currency_preferences: Value,
    pub status: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl TryFrom<MerchantRow> for Merchant {
    type Error = RepositoryError;
    fn try_from(row: MerchantRow) -> Result<Self, Self::Error> {
        let prefs: Vec<String> =
            serde_json::from_value(row.currency_preferences).unwrap_or_default();
        Ok(Self {
            id: MerchantId::from_uuid(row.id),
            tenant_id: TenantId::from_uuid(row.tenant_id),
            legal_name: row.legal_name,
            display_name: row.display_name,
            merchant_reference: row.merchant_reference,
            country: row.country,
            currency_preferences: prefs
                .iter()
                .filter_map(|c| c.parse::<Currency>().ok())
                .collect(),
            status: match row.status.as_str() {
                "active" => MerchantStatus::Active,
                "suspended" => MerchantStatus::Suspended,
                _ => MerchantStatus::Disabled,
            },
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(FromRow)]
pub struct PaymentRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub merchant_id: Uuid,
    pub merchant_order_id: String,
    pub amount_minor: i64,
    pub currency: String,
    pub status: String,
    pub allowed_methods: Value,
    pub description: Option<String>,
    pub expires_at: OffsetDateTime,
    pub return_url: Option<String>,
    pub metadata: Value,
    pub idempotency_key: String,
    pub routing_policy_id: Option<Uuid>,
    pub version: i32,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl TryFrom<PaymentRow> for PaymentRequest {
    type Error = RepositoryError;
    fn try_from(row: PaymentRow) -> Result<Self, Self::Error> {
        let methods: Vec<String> = serde_json::from_value(row.allowed_methods).unwrap_or_default();
        Ok(Self {
            id: PaymentId::from_uuid(row.id),
            tenant_id: TenantId::from_uuid(row.tenant_id),
            merchant_id: MerchantId::from_uuid(row.merchant_id),
            merchant_order_id: MerchantOrderId::new(row.merchant_order_id)
                .map_err(|e| RepositoryError::Infra(e.to_string()))?,
            amount_minor: AmountMinor::new(row.amount_minor)
                .map_err(|e| RepositoryError::Infra(e.to_string()))?,
            currency: row
                .currency
                .parse()
                .map_err(|e: openpay_domain::DomainError| RepositoryError::Infra(e.to_string()))?,
            status: parse_payment_status(&row.status)?,
            allowed_methods: methods.iter().filter_map(|m| parse_method(m)).collect(),
            description: row.description,
            expires_at: row.expires_at,
            return_url: row.return_url,
            metadata: row.metadata,
            idempotency_key: IdempotencyKey::new(row.idempotency_key)
                .map_err(|e| RepositoryError::Infra(e.to_string()))?,
            routing_policy_id: row.routing_policy_id.map(RoutingPolicyId::from_uuid),
            version: row.version,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(FromRow)]
pub struct AttemptRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub payment_request_id: Uuid,
    pub connector_id: Uuid,
    pub connector_key: String,
    pub rail_type: String,
    pub provider_reference: Option<String>,
    pub status: String,
    pub failure_code: Option<String>,
    pub failure_message_safe: Option<String>,
    pub amount_minor: i64,
    pub currency: String,
    pub requested_at: OffsetDateTime,
    pub authorized_at: Option<OffsetDateTime>,
    pub settled_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl TryFrom<AttemptRow> for PaymentAttempt {
    type Error = RepositoryError;
    fn try_from(row: AttemptRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: AttemptId::from_uuid(row.id),
            tenant_id: TenantId::from_uuid(row.tenant_id),
            payment_request_id: PaymentId::from_uuid(row.payment_request_id),
            connector_id: ConnectorId::from_uuid(row.connector_id),
            connector_key: row.connector_key,
            rail_type: row.rail_type,
            provider_reference: row.provider_reference,
            status: parse_attempt_status(&row.status)?,
            failure_code: row.failure_code,
            failure_message_safe: row.failure_message_safe,
            amount_minor: AmountMinor::new(row.amount_minor)
                .map_err(|e| RepositoryError::Infra(e.to_string()))?,
            currency: row
                .currency
                .parse()
                .map_err(|e: openpay_domain::DomainError| RepositoryError::Infra(e.to_string()))?,
            requested_at: row.requested_at,
            authorized_at: row.authorized_at,
            settled_at: row.settled_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(FromRow)]
pub struct AuditRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub actor_type: String,
    pub actor_id: String,
    pub event_type: String,
    pub resource_type: String,
    pub resource_id: String,
    pub request_id: Option<String>,
    pub ip_hash: Option<String>,
    pub metadata_redacted: Value,
    pub occurred_at: OffsetDateTime,
}

impl From<AuditRow> for AuditEvent {
    fn from(row: AuditRow) -> Self {
        Self {
            id: openpay_domain::AuditId::from_uuid(row.id),
            tenant_id: TenantId::from_uuid(row.tenant_id),
            actor_type: row.actor_type,
            actor_id: row.actor_id,
            event_type: row.event_type,
            resource_type: row.resource_type,
            resource_id: row.resource_id,
            request_id: row.request_id,
            ip_hash: row.ip_hash,
            metadata_redacted: row.metadata_redacted,
            occurred_at: row.occurred_at,
        }
    }
}

#[derive(FromRow)]
pub struct OutboxRow {
    pub id: String,
    pub tenant_id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub event_type: String,
    pub payload: Value,
    pub created_at: OffsetDateTime,
}

impl From<OutboxRow> for OutboxRecord {
    fn from(row: OutboxRow) -> Self {
        Self {
            id: row.id,
            tenant_id: TenantId::from_uuid(row.tenant_id),
            aggregate_type: row.aggregate_type,
            aggregate_id: row.aggregate_id,
            event_type: row.event_type,
            payload: row.payload,
            created_at: row.created_at,
        }
    }
}

#[derive(FromRow)]
pub struct EndpointRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub merchant_id: Uuid,
    pub url: String,
    pub event_types: Value,
    pub signing_secret_ref: String,
    pub status: String,
    pub failure_count: i32,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl TryFrom<EndpointRow> for WebhookEndpoint {
    type Error = RepositoryError;
    fn try_from(row: EndpointRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: WebhookEndpointId::from_uuid(row.id),
            tenant_id: TenantId::from_uuid(row.tenant_id),
            merchant_id: MerchantId::from_uuid(row.merchant_id),
            url: row.url,
            event_types: serde_json::from_value(row.event_types).unwrap_or_default(),
            signing_secret_ref: row.signing_secret_ref,
            status: if row.status == "active" {
                WebhookEndpointStatus::Active
            } else {
                WebhookEndpointStatus::Disabled
            },
            failure_count: row.failure_count,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(FromRow)]
pub struct DeliveryRow {
    pub id: Uuid,
    pub webhook_endpoint_id: Uuid,
    pub event_id: String,
    pub payload_version: String,
    pub status: String,
    pub attempt_count: i32,
    pub next_retry_at: Option<OffsetDateTime>,
    pub response_code: Option<i32>,
    pub last_error_safe: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl TryFrom<DeliveryRow> for WebhookDelivery {
    type Error = RepositoryError;
    fn try_from(row: DeliveryRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: openpay_domain::WebhookDeliveryId::from_uuid(row.id),
            webhook_endpoint_id: WebhookEndpointId::from_uuid(row.webhook_endpoint_id),
            event_id: row
                .event_id
                .parse()
                .unwrap_or_else(|_| openpay_domain::EventId::new()),
            payload_version: row.payload_version,
            status: parse_delivery(&row.status),
            attempt_count: row.attempt_count,
            next_retry_at: row.next_retry_at,
            response_code: row.response_code,
            last_error_safe: row.last_error_safe,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(FromRow)]
pub struct PolicyRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub status: String,
    pub rules_json: Value,
    pub fallback_policy: Value,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl From<PolicyRow> for RoutingPolicy {
    fn from(row: PolicyRow) -> Self {
        Self {
            id: RoutingPolicyId::from_uuid(row.id),
            tenant_id: TenantId::from_uuid(row.tenant_id),
            name: row.name,
            status: if row.status == "active" {
                RoutingPolicyStatus::Active
            } else {
                RoutingPolicyStatus::Disabled
            },
            rules_json: row.rules_json,
            fallback_policy: row.fallback_policy,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(FromRow)]
pub struct ConnectorCatRow {
    pub key: String,
    pub health_status: String,
    pub capabilities: Value,
    pub priority: i32,
    pub status: String,
}

impl From<ConnectorCatRow> for ConnectorSnapshot {
    fn from(row: ConnectorCatRow) -> Self {
        let methods = row
            .capabilities
            .get("methods")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().and_then(parse_method))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            key: row.key,
            health: match row.health_status.as_str() {
                "HEALTHY" | "healthy" => ConnectorHealth::Healthy,
                "DEGRADED" | "degraded" => ConnectorHealth::Degraded,
                "UNHEALTHY" | "unhealthy" => ConnectorHealth::Unhealthy,
                _ => ConnectorHealth::Unknown,
            },
            methods,
            priority: row.priority,
            enabled: row.status == "enabled",
        }
    }
}

#[derive(FromRow)]
pub struct ApiKeyRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub merchant_id: Option<Uuid>,
    pub name: String,
    pub hash: String,
    pub fingerprint: String,
    pub scopes: Value,
    pub revoked: bool,
}

impl From<ApiKeyRow> for ApiKeyRecord {
    fn from(row: ApiKeyRow) -> Self {
        Self {
            id: row.id.to_string(),
            tenant_id: TenantId::from_uuid(row.tenant_id),
            merchant_id: row.merchant_id.map(MerchantId::from_uuid),
            name: row.name,
            hash: row.hash,
            fingerprint: row.fingerprint,
            scopes: serde_json::from_value(row.scopes).unwrap_or_default(),
            revoked: row.revoked,
        }
    }
}

#[derive(FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub role: String,
}

impl From<UserRow> for AuthUser {
    fn from(row: UserRow) -> Self {
        Self {
            id: row.id.to_string(),
            tenant_id: TenantId::from_uuid(row.tenant_id),
            email: row.email,
            password_hash: row.password_hash,
            role: row.role,
        }
    }
}

pub fn parse_payment_status(raw: &str) -> Result<PaymentStatus, RepositoryError> {
    match raw {
        "CREATED" => Ok(PaymentStatus::Created),
        "PENDING" => Ok(PaymentStatus::Pending),
        "REQUIRES_ACTION" => Ok(PaymentStatus::RequiresAction),
        "AUTHORIZED" => Ok(PaymentStatus::Authorized),
        "PROCESSING" => Ok(PaymentStatus::Processing),
        "SETTLED" => Ok(PaymentStatus::Settled),
        "FAILED" => Ok(PaymentStatus::Failed),
        "CANCELLED" => Ok(PaymentStatus::Cancelled),
        "EXPIRED" => Ok(PaymentStatus::Expired),
        "REFUND_PENDING" => Ok(PaymentStatus::RefundPending),
        "REFUNDED" => Ok(PaymentStatus::Refunded),
        "PARTIALLY_REFUNDED" => Ok(PaymentStatus::PartiallyRefunded),
        other => Err(RepositoryError::Infra(format!("unknown status {other}"))),
    }
}

fn parse_attempt_status(raw: &str) -> Result<AttemptStatus, RepositoryError> {
    match raw {
        "CREATED" => Ok(AttemptStatus::Created),
        "REQUIRES_ACTION" => Ok(AttemptStatus::RequiresAction),
        "PROCESSING" => Ok(AttemptStatus::Processing),
        "AUTHORIZED" => Ok(AttemptStatus::Authorized),
        "SETTLED" => Ok(AttemptStatus::Settled),
        "FAILED" => Ok(AttemptStatus::Failed),
        "CANCELLED" => Ok(AttemptStatus::Cancelled),
        "EXPIRED" => Ok(AttemptStatus::Expired),
        "AMBIGUOUS" => Ok(AttemptStatus::Ambiguous),
        other => Err(RepositoryError::Infra(format!("unknown attempt {other}"))),
    }
}

fn parse_method(raw: &str) -> Option<PaymentMethod> {
    match raw {
        "ACCOUNT_TO_ACCOUNT" => Some(PaymentMethod::AccountToAccount),
        "CARD" => Some(PaymentMethod::Card),
        "WALLET" => Some(PaymentMethod::Wallet),
        "MANUAL" => Some(PaymentMethod::Manual),
        _ => None,
    }
}

fn parse_tenant_status(raw: &str) -> Result<TenantStatus, RepositoryError> {
    match raw {
        "active" => Ok(TenantStatus::Active),
        "suspended" => Ok(TenantStatus::Suspended),
        _ => Ok(TenantStatus::Disabled),
    }
}

fn parse_plan(raw: &str) -> Result<Plan, RepositoryError> {
    match raw {
        "community" => Ok(Plan::Community),
        "cloud" => Ok(Plan::Cloud),
        "enterprise" => Ok(Plan::Enterprise),
        other => Err(RepositoryError::Infra(format!("unknown plan {other}"))),
    }
}

fn parse_delivery(raw: &str) -> DeliveryStatus {
    match raw {
        "delivered" => DeliveryStatus::Delivered,
        "retrying" => DeliveryStatus::Retrying,
        "dead_lettered" => DeliveryStatus::DeadLettered,
        _ => DeliveryStatus::Pending,
    }
}

pub fn methods_json(methods: &[PaymentMethod]) -> Value {
    Value::Array(
        methods
            .iter()
            .map(|m| Value::String(m.as_str().to_string()))
            .collect(),
    )
}
