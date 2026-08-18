use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

use openpay_domain::{
    AttemptId, AuditEvent, ConnectorHealth, ConnectorId, Merchant, MerchantId, PaymentAttempt,
    PaymentId, PaymentRequest, RoutingDecision, Tenant, TenantId, TransitionPaymentCommand,
    WebhookDelivery, WebhookEndpoint,
};

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("idempotency replay mismatch")]
    IdempotencyMismatch,
    #[error("optimistic lock conflict")]
    VersionConflict,
    #[error("infrastructure: {0}")]
    Infra(String),
}

#[derive(Debug, Clone)]
pub struct IdempotencyContext {
    pub tenant_id: TenantId,
    pub key: String,
    pub request_fingerprint: String,
}

#[derive(Debug, Clone)]
pub struct CreatePaymentResult {
    pub payment: PaymentRequest,
    pub replayed: bool,
}

#[async_trait]
pub trait PaymentRepository: Send + Sync {
    async fn create_idempotent(
        &self,
        payment: PaymentRequest,
        idempotency: IdempotencyContext,
        audit: AuditEvent,
        outbox: OutboxRecord,
    ) -> Result<CreatePaymentResult, RepositoryError>;

    async fn get_by_id(
        &self,
        tenant_id: TenantId,
        payment_id: PaymentId,
    ) -> Result<PaymentRequest, RepositoryError>;

    async fn list_by_merchant(
        &self,
        tenant_id: TenantId,
        merchant_id: MerchantId,
        limit: i64,
    ) -> Result<Vec<PaymentRequest>, RepositoryError>;

    async fn transition_payment(
        &self,
        command: TransitionPaymentCommand,
        now: OffsetDateTime,
        audit: Option<AuditEvent>,
        outbox: Option<OutboxRecord>,
    ) -> Result<PaymentRequest, RepositoryError>;

    async fn insert_attempt(
        &self,
        attempt: PaymentAttempt,
    ) -> Result<PaymentAttempt, RepositoryError>;

    async fn list_attempts(
        &self,
        tenant_id: TenantId,
        payment_id: PaymentId,
    ) -> Result<Vec<PaymentAttempt>, RepositoryError>;

    async fn get_attempt(
        &self,
        tenant_id: TenantId,
        attempt_id: AttemptId,
    ) -> Result<PaymentAttempt, RepositoryError>;

    async fn update_attempt(
        &self,
        attempt: PaymentAttempt,
    ) -> Result<PaymentAttempt, RepositoryError>;

    async fn list_reconcilable_payments(
        &self,
        limit: i64,
    ) -> Result<Vec<(TenantId, PaymentId)>, RepositoryError>;

    async fn list_expirable_payments(
        &self,
        now: OffsetDateTime,
        limit: i64,
    ) -> Result<Vec<PaymentRequest>, RepositoryError>;
}

#[async_trait]
pub trait MerchantRepository: Send + Sync {
    async fn get_merchant(
        &self,
        tenant_id: TenantId,
        merchant_id: MerchantId,
    ) -> Result<Merchant, RepositoryError>;

    async fn get_tenant(&self, tenant_id: TenantId) -> Result<Tenant, RepositoryError>;

    async fn list_merchants(&self, tenant_id: TenantId) -> Result<Vec<Merchant>, RepositoryError>;
}

#[async_trait]
pub trait AuditRepository: Send + Sync {
    async fn record(&self, event: AuditEvent) -> Result<(), RepositoryError>;
    async fn list_for_payment(
        &self,
        tenant_id: TenantId,
        payment_id: PaymentId,
    ) -> Result<Vec<AuditEvent>, RepositoryError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxRecord {
    pub id: String,
    pub tenant_id: TenantId,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub event_type: String,
    pub payload: Value,
    pub created_at: OffsetDateTime,
}

#[async_trait]
pub trait OutboxRepository: Send + Sync {
    async fn enqueue(&self, record: OutboxRecord) -> Result<(), RepositoryError>;
    async fn fetch_pending(&self, limit: i64) -> Result<Vec<OutboxRecord>, RepositoryError>;
    async fn mark_published(&self, id: &str) -> Result<(), RepositoryError>;
}

#[async_trait]
pub trait WebhookRepository: Send + Sync {
    async fn list_endpoints(
        &self,
        tenant_id: TenantId,
        merchant_id: MerchantId,
    ) -> Result<Vec<WebhookEndpoint>, RepositoryError>;

    async fn insert_delivery(&self, delivery: WebhookDelivery) -> Result<(), RepositoryError>;

    async fn list_pending_deliveries(
        &self,
        limit: i64,
    ) -> Result<Vec<WebhookDelivery>, RepositoryError>;

    async fn update_delivery(&self, delivery: WebhookDelivery) -> Result<(), RepositoryError>;

    async fn list_deliveries_for_payment(
        &self,
        tenant_id: TenantId,
        payment_id: PaymentId,
    ) -> Result<Vec<WebhookDelivery>, RepositoryError>;

    async fn get_endpoint(
        &self,
        endpoint_id: openpay_domain::WebhookEndpointId,
    ) -> Result<WebhookEndpoint, RepositoryError>;
}

#[async_trait]
pub trait QrNonceStore: Send + Sync {
    async fn remember_nonce(&self, nonce: &str, ttl_secs: i64) -> Result<bool, RepositoryError>;
}

#[derive(Debug, Clone)]
pub struct ApiKeyRecord {
    pub id: String,
    pub tenant_id: TenantId,
    pub merchant_id: Option<MerchantId>,
    pub name: String,
    pub hash: String,
    pub fingerprint: String,
    pub scopes: Vec<String>,
    pub revoked: bool,
}

#[async_trait]
pub trait ApiKeyRepository: Send + Sync {
    async fn find_by_fingerprint(
        &self,
        fingerprint: &str,
    ) -> Result<Option<ApiKeyRecord>, RepositoryError>;
    async fn list_for_merchant(
        &self,
        tenant_id: TenantId,
        merchant_id: MerchantId,
    ) -> Result<Vec<ApiKeyRecord>, RepositoryError>;
    async fn insert(&self, record: ApiKeyRecord) -> Result<(), RepositoryError>;
}

#[async_trait]
pub trait RoutingRepository: Send + Sync {
    async fn get_active_policy(
        &self,
        tenant_id: TenantId,
    ) -> Result<Option<openpay_domain::RoutingPolicy>, RepositoryError>;
}

#[derive(Debug, Clone)]
pub struct ConnectorSnapshot {
    pub key: String,
    pub health: ConnectorHealth,
    pub methods: Vec<openpay_domain::PaymentMethod>,
    pub priority: i32,
    pub enabled: bool,
}

#[async_trait]
pub trait ConnectorCatalog: Send + Sync {
    async fn list_enabled(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<ConnectorSnapshot>, RepositoryError>;

    async fn connector_id_by_key(
        &self,
        tenant_id: TenantId,
        key: &str,
    ) -> Result<ConnectorId, RepositoryError>;
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: String,
    pub tenant_id: TenantId,
    pub email: String,
    pub password_hash: String,
    pub role: String,
}

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_email(&self, email: &str) -> Result<Option<AuthUser>, RepositoryError>;
}

#[derive(Debug, Clone)]
pub struct ManualAttemptDecision {
    pub attempt_id: AttemptId,
    pub approve: bool,
}

pub trait Clock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}

pub struct SystemClock;

impl Default for SystemClock {
    fn default() -> Self {
        Self
    }
}

impl Clone for SystemClock {
    fn clone(&self) -> Self {
        Self
    }
}

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

#[derive(Debug, Clone)]
pub struct RoutingPortsView {
    pub decision: RoutingDecision,
}
