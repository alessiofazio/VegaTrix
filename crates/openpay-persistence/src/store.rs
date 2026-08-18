use async_trait::async_trait;
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

use openpay_application::{
    ApiKeyRecord, ApiKeyRepository, AuditRepository, AuthUser, Clock, ConnectorCatalog,
    ConnectorSnapshot, CreatePaymentResult, IdempotencyContext, MerchantRepository, OutboxRecord,
    OutboxRepository, PaymentRepository, QrNonceStore, RepositoryError, RoutingRepository,
    SystemClock, UserRepository, WebhookRepository,
};
use openpay_domain::{
    AttemptId, AuditEvent, Merchant, MerchantId, PaymentAttempt, PaymentId, PaymentRequest,
    RoutingPolicy, Tenant, TenantId, TransitionPaymentCommand, WebhookDelivery, WebhookEndpoint,
    WebhookEndpointId,
};

use crate::map::{
    ApiKeyRow, AttemptRow, AuditRow, ConnectorCatRow, DeliveryRow, EndpointRow, MerchantRow,
    OutboxRow, PaymentRow, PolicyRow, TenantRow, UserRow, methods_json,
};

#[derive(Clone)]
pub struct PgStore {
    pub pool: PgPool,
}

impl PgStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn clock(&self) -> SystemClock {
        SystemClock
    }

    async fn insert_audit_tx(
        tx: &mut Transaction<'_, Postgres>,
        event: &AuditEvent,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO audit_events
             (id, tenant_id, actor_type, actor_id, event_type, resource_type, resource_id, request_id, ip_hash, metadata_redacted, occurred_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(event.id.as_uuid())
        .bind(event.tenant_id.as_uuid())
        .bind(&event.actor_type)
        .bind(&event.actor_id)
        .bind(&event.event_type)
        .bind(&event.resource_type)
        .bind(&event.resource_id)
        .bind(&event.request_id)
        .bind(&event.ip_hash)
        .bind(&event.metadata_redacted)
        .bind(event.occurred_at)
        .execute(&mut **tx)
        .await
        .map_err(infra)?;
        Ok(())
    }

    async fn insert_outbox_tx(
        tx: &mut Transaction<'_, Postgres>,
        record: &OutboxRecord,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO outbox_events
             (id, tenant_id, aggregate_type, aggregate_id, event_type, payload, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(&record.id)
        .bind(record.tenant_id.as_uuid())
        .bind(&record.aggregate_type)
        .bind(&record.aggregate_id)
        .bind(&record.event_type)
        .bind(&record.payload)
        .bind(record.created_at)
        .execute(&mut **tx)
        .await
        .map_err(infra)?;
        Ok(())
    }

    pub async fn insert_delivery_with_payload(
        &self,
        delivery: &WebhookDelivery,
        payload: &Value,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO webhook_deliveries
             (id, webhook_endpoint_id, event_id, payload_version, payload, status, attempt_count, next_retry_at, response_code, last_error_safe, created_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
        )
        .bind(delivery.id.as_uuid())
        .bind(delivery.webhook_endpoint_id.as_uuid())
        .bind(delivery.event_id.as_prefixed())
        .bind(&delivery.payload_version)
        .bind(payload)
        .bind(delivery_status(&delivery.status))
        .bind(delivery.attempt_count)
        .bind(delivery.next_retry_at)
        .bind(delivery.response_code)
        .bind(&delivery.last_error_safe)
        .bind(delivery.created_at)
        .bind(delivery.updated_at)
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }

    pub async fn load_delivery_payload(&self, id: Uuid) -> Result<Value, RepositoryError> {
        let row: (Value,) = sqlx::query_as("SELECT payload FROM webhook_deliveries WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(infra)?
            .ok_or(RepositoryError::NotFound)?;
        Ok(row.0)
    }

    pub async fn pending_deliveries_full(
        &self,
        limit: i64,
    ) -> Result<Vec<(WebhookDelivery, Value, WebhookEndpoint)>, RepositoryError> {
        let now = OffsetDateTime::now_utc();
        let rows: Vec<DeliveryRow> = sqlx::query_as(
            "SELECT id, webhook_endpoint_id, event_id, payload_version, status, attempt_count,
                    next_retry_at, response_code, last_error_safe, created_at, updated_at
             FROM webhook_deliveries
             WHERE status IN ('pending','retrying')
               AND (next_retry_at IS NULL OR next_retry_at <= $1)
             ORDER BY created_at ASC
             LIMIT $2",
        )
        .bind(now)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        let mut out = Vec::new();
        for row in rows {
            let payload = self.load_delivery_payload(row.id).await?;
            let endpoint = self
                .get_endpoint(WebhookEndpointId::from_uuid(row.webhook_endpoint_id))
                .await?;
            out.push((row.try_into()?, payload, endpoint));
        }
        Ok(out)
    }
}

fn infra(err: sqlx::Error) -> RepositoryError {
    RepositoryError::Infra(err.to_string())
}

fn delivery_status(status: &openpay_domain::DeliveryStatus) -> &'static str {
    match status {
        openpay_domain::DeliveryStatus::Pending => "pending",
        openpay_domain::DeliveryStatus::Delivered => "delivered",
        openpay_domain::DeliveryStatus::Retrying => "retrying",
        openpay_domain::DeliveryStatus::DeadLettered => "dead_lettered",
    }
}

#[async_trait]
impl PaymentRepository for PgStore {
    async fn create_idempotent(
        &self,
        payment: PaymentRequest,
        idempotency: IdempotencyContext,
        audit: AuditEvent,
        outbox: OutboxRecord,
    ) -> Result<CreatePaymentResult, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(infra)?;
        let existing: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT payment_id, request_fingerprint FROM idempotency_keys
             WHERE tenant_id = $1 AND idempotency_key = $2
             LIMIT 1",
        )
        .bind(idempotency.tenant_id.as_uuid())
        .bind(&idempotency.key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(infra)?;

        if let Some((payment_id, fingerprint)) = existing {
            if fingerprint != idempotency.request_fingerprint {
                return Err(RepositoryError::IdempotencyMismatch);
            }
            tx.commit().await.map_err(infra)?;
            let stored = self
                .get_by_id(idempotency.tenant_id, PaymentId::from_uuid(payment_id))
                .await?;
            return Ok(CreatePaymentResult {
                payment: stored,
                replayed: true,
            });
        }

        sqlx::query(
            "INSERT INTO payment_requests (
                id, tenant_id, merchant_id, merchant_order_id, amount_minor, currency, status,
                allowed_methods, description, expires_at, return_url, metadata, idempotency_key,
                routing_policy_id, version, created_at, updated_at
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)",
        )
        .bind(payment.id.as_uuid())
        .bind(payment.tenant_id.as_uuid())
        .bind(payment.merchant_id.as_uuid())
        .bind(payment.merchant_order_id.as_str())
        .bind(payment.amount_minor.get())
        .bind(payment.currency.as_str())
        .bind(payment.status.as_str())
        .bind(methods_json(&payment.allowed_methods))
        .bind(&payment.description)
        .bind(payment.expires_at)
        .bind(&payment.return_url)
        .bind(&payment.metadata)
        .bind(payment.idempotency_key.as_str())
        .bind(payment.routing_policy_id.map(|id| id.as_uuid()))
        .bind(payment.version)
        .bind(payment.created_at)
        .bind(payment.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db) = &e {
                if db.code().as_deref() == Some("23505") {
                    return RepositoryError::Conflict("idempotency".into());
                }
            }
            infra(e)
        })?;

        sqlx::query(
            "INSERT INTO idempotency_keys (tenant_id, idempotency_key, request_fingerprint, payment_id, created_at)
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(idempotency.tenant_id.as_uuid())
        .bind(&idempotency.key)
        .bind(&idempotency.request_fingerprint)
        .bind(payment.id.as_uuid())
        .bind(payment.created_at)
        .execute(&mut *tx)
        .await
        .map_err(infra)?;

        Self::insert_audit_tx(&mut tx, &audit).await?;
        Self::insert_outbox_tx(&mut tx, &outbox).await?;
        tx.commit().await.map_err(infra)?;
        Ok(CreatePaymentResult {
            payment,
            replayed: false,
        })
    }

    async fn get_by_id(
        &self,
        tenant_id: TenantId,
        payment_id: PaymentId,
    ) -> Result<PaymentRequest, RepositoryError> {
        let row: PaymentRow =
            sqlx::query_as("SELECT * FROM payment_requests WHERE id = $1 AND tenant_id = $2")
                .bind(payment_id.as_uuid())
                .bind(tenant_id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(infra)?
                .ok_or(RepositoryError::NotFound)?;
        row.try_into()
    }

    async fn list_by_merchant(
        &self,
        tenant_id: TenantId,
        merchant_id: MerchantId,
        limit: i64,
    ) -> Result<Vec<PaymentRequest>, RepositoryError> {
        let rows: Vec<PaymentRow> = sqlx::query_as(
            "SELECT * FROM payment_requests
             WHERE tenant_id = $1 AND merchant_id = $2
             ORDER BY created_at DESC LIMIT $3",
        )
        .bind(tenant_id.as_uuid())
        .bind(merchant_id.as_uuid())
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn transition_payment(
        &self,
        command: TransitionPaymentCommand,
        now: OffsetDateTime,
        audit: Option<AuditEvent>,
        outbox: Option<OutboxRecord>,
    ) -> Result<PaymentRequest, RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(infra)?;
        let row: PaymentRow = sqlx::query_as(
            "SELECT * FROM payment_requests WHERE id = $1 AND tenant_id = $2 FOR UPDATE",
        )
        .bind(command.payment_id.as_uuid())
        .bind(command.tenant_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(infra)?
        .ok_or(RepositoryError::NotFound)?;
        let mut payment: PaymentRequest = row.try_into()?;
        if let Some(expected) = command.expected_version {
            if payment.version != expected {
                return Err(RepositoryError::VersionConflict);
            }
        }
        payment
            .transition(command.next_status, now)
            .map_err(|e| RepositoryError::Conflict(e.to_string()))?;

        let result = sqlx::query(
            "UPDATE payment_requests SET status = $1, version = $2, updated_at = $3
             WHERE id = $4 AND tenant_id = $5 AND version = $6",
        )
        .bind(payment.status.as_str())
        .bind(payment.version)
        .bind(payment.updated_at)
        .bind(payment.id.as_uuid())
        .bind(payment.tenant_id.as_uuid())
        .bind(command.expected_version.unwrap_or(payment.version - 1))
        .execute(&mut *tx)
        .await
        .map_err(infra)?;
        if result.rows_affected() != 1 {
            return Err(RepositoryError::VersionConflict);
        }
        if let Some(audit) = audit {
            Self::insert_audit_tx(&mut tx, &audit).await?;
        }
        if let Some(outbox) = outbox {
            Self::insert_outbox_tx(&mut tx, &outbox).await?;
        }
        tx.commit().await.map_err(infra)?;
        Ok(payment)
    }

    async fn insert_attempt(
        &self,
        attempt: PaymentAttempt,
    ) -> Result<PaymentAttempt, RepositoryError> {
        sqlx::query(
            "INSERT INTO payment_attempts (
                id, tenant_id, payment_request_id, connector_id, connector_key, rail_type,
                provider_reference, status, failure_code, failure_message_safe, amount_minor, currency,
                requested_at, authorized_at, settled_at, created_at, updated_at
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)",
        )
        .bind(attempt.id.as_uuid())
        .bind(attempt.tenant_id.as_uuid())
        .bind(attempt.payment_request_id.as_uuid())
        .bind(attempt.connector_id.as_uuid())
        .bind(&attempt.connector_key)
        .bind(&attempt.rail_type)
        .bind(&attempt.provider_reference)
        .bind(attempt.status.as_str())
        .bind(&attempt.failure_code)
        .bind(&attempt.failure_message_safe)
        .bind(attempt.amount_minor.get())
        .bind(attempt.currency.as_str())
        .bind(attempt.requested_at)
        .bind(attempt.authorized_at)
        .bind(attempt.settled_at)
        .bind(attempt.created_at)
        .bind(attempt.updated_at)
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(attempt)
    }

    async fn list_attempts(
        &self,
        tenant_id: TenantId,
        payment_id: PaymentId,
    ) -> Result<Vec<PaymentAttempt>, RepositoryError> {
        let rows: Vec<AttemptRow> = sqlx::query_as(
            "SELECT * FROM payment_attempts WHERE tenant_id = $1 AND payment_request_id = $2 ORDER BY created_at",
        )
        .bind(tenant_id.as_uuid())
        .bind(payment_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn get_attempt(
        &self,
        tenant_id: TenantId,
        attempt_id: AttemptId,
    ) -> Result<PaymentAttempt, RepositoryError> {
        let row: AttemptRow =
            sqlx::query_as("SELECT * FROM payment_attempts WHERE id = $1 AND tenant_id = $2")
                .bind(attempt_id.as_uuid())
                .bind(tenant_id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(infra)?
                .ok_or(RepositoryError::NotFound)?;
        row.try_into()
    }

    async fn update_attempt(
        &self,
        attempt: PaymentAttempt,
    ) -> Result<PaymentAttempt, RepositoryError> {
        sqlx::query(
            "UPDATE payment_attempts SET status=$1, provider_reference=$2, failure_code=$3,
             failure_message_safe=$4, authorized_at=$5, settled_at=$6, updated_at=$7
             WHERE id=$8 AND tenant_id=$9",
        )
        .bind(attempt.status.as_str())
        .bind(&attempt.provider_reference)
        .bind(&attempt.failure_code)
        .bind(&attempt.failure_message_safe)
        .bind(attempt.authorized_at)
        .bind(attempt.settled_at)
        .bind(attempt.updated_at)
        .bind(attempt.id.as_uuid())
        .bind(attempt.tenant_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(attempt)
    }

    async fn list_reconcilable_payments(
        &self,
        limit: i64,
    ) -> Result<Vec<(TenantId, PaymentId)>, RepositoryError> {
        let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
            "SELECT DISTINCT pr.tenant_id, pr.id
             FROM payment_requests pr
             JOIN payment_attempts pa ON pa.payment_request_id = pr.id
             WHERE pr.status = 'PROCESSING'
               AND pa.status IN ('AMBIGUOUS', 'PROCESSING')
             ORDER BY pr.updated_at ASC
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows
            .into_iter()
            .map(|(t, p)| (TenantId::from_uuid(t), PaymentId::from_uuid(p)))
            .collect())
    }

    async fn list_expirable_payments(
        &self,
        now: OffsetDateTime,
        limit: i64,
    ) -> Result<Vec<PaymentRequest>, RepositoryError> {
        let rows: Vec<PaymentRow> = sqlx::query_as(
            "SELECT * FROM payment_requests
             WHERE expires_at <= $1
               AND status IN ('PENDING', 'REQUIRES_ACTION')
             ORDER BY expires_at ASC
             LIMIT $2",
        )
        .bind(now)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        rows.into_iter().map(TryInto::try_into).collect()
    }
}

#[async_trait]
impl MerchantRepository for PgStore {
    async fn get_merchant(
        &self,
        tenant_id: TenantId,
        merchant_id: MerchantId,
    ) -> Result<Merchant, RepositoryError> {
        let row: MerchantRow =
            sqlx::query_as("SELECT * FROM merchants WHERE id = $1 AND tenant_id = $2")
                .bind(merchant_id.as_uuid())
                .bind(tenant_id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(infra)?
                .ok_or(RepositoryError::NotFound)?;
        row.try_into()
    }

    async fn get_tenant(&self, tenant_id: TenantId) -> Result<Tenant, RepositoryError> {
        let row: TenantRow = sqlx::query_as("SELECT * FROM tenants WHERE id = $1")
            .bind(tenant_id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(infra)?
            .ok_or(RepositoryError::NotFound)?;
        row.try_into()
    }

    async fn list_merchants(&self, tenant_id: TenantId) -> Result<Vec<Merchant>, RepositoryError> {
        let rows: Vec<MerchantRow> =
            sqlx::query_as("SELECT * FROM merchants WHERE tenant_id = $1 ORDER BY display_name")
                .bind(tenant_id.as_uuid())
                .fetch_all(&self.pool)
                .await
                .map_err(infra)?;
        rows.into_iter().map(TryInto::try_into).collect()
    }
}

#[async_trait]
impl AuditRepository for PgStore {
    async fn record(&self, event: AuditEvent) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(infra)?;
        PgStore::insert_audit_tx(&mut tx, &event).await?;
        tx.commit().await.map_err(infra)?;
        Ok(())
    }

    async fn list_for_payment(
        &self,
        tenant_id: TenantId,
        payment_id: PaymentId,
    ) -> Result<Vec<AuditEvent>, RepositoryError> {
        let rows: Vec<AuditRow> = sqlx::query_as(
            "SELECT * FROM audit_events
             WHERE tenant_id = $1 AND resource_type = 'payment_request' AND resource_id = $2
             ORDER BY occurred_at",
        )
        .bind(tenant_id.as_uuid())
        .bind(payment_id.as_prefixed())
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
}

#[async_trait]
impl OutboxRepository for PgStore {
    async fn enqueue(&self, record: OutboxRecord) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(infra)?;
        PgStore::insert_outbox_tx(&mut tx, &record).await?;
        tx.commit().await.map_err(infra)?;
        Ok(())
    }

    async fn fetch_pending(&self, limit: i64) -> Result<Vec<OutboxRecord>, RepositoryError> {
        let rows: Vec<OutboxRow> = sqlx::query_as(
            "SELECT id, tenant_id, aggregate_type, aggregate_id, event_type, payload, created_at
             FROM outbox_events WHERE published_at IS NULL ORDER BY created_at LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn mark_published(&self, id: &str) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE outbox_events SET published_at = $1 WHERE id = $2")
            .bind(OffsetDateTime::now_utc())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        Ok(())
    }
}

#[async_trait]
impl WebhookRepository for PgStore {
    async fn list_endpoints(
        &self,
        tenant_id: TenantId,
        merchant_id: MerchantId,
    ) -> Result<Vec<WebhookEndpoint>, RepositoryError> {
        let rows: Vec<EndpointRow> = sqlx::query_as(
            "SELECT * FROM webhook_endpoints WHERE tenant_id = $1 AND merchant_id = $2 AND status = 'active'",
        )
        .bind(tenant_id.as_uuid())
        .bind(merchant_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn insert_delivery(&self, delivery: WebhookDelivery) -> Result<(), RepositoryError> {
        self.insert_delivery_with_payload(&delivery, &Value::Null)
            .await
    }

    async fn list_pending_deliveries(
        &self,
        limit: i64,
    ) -> Result<Vec<WebhookDelivery>, RepositoryError> {
        Ok(self
            .pending_deliveries_full(limit)
            .await?
            .into_iter()
            .map(|(d, _, _)| d)
            .collect())
    }

    async fn update_delivery(&self, delivery: WebhookDelivery) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE webhook_deliveries
             SET status=$1, attempt_count=$2, next_retry_at=$3, response_code=$4, last_error_safe=$5, updated_at=$6
             WHERE id=$7",
        )
        .bind(delivery_status(&delivery.status))
        .bind(delivery.attempt_count)
        .bind(delivery.next_retry_at)
        .bind(delivery.response_code)
        .bind(&delivery.last_error_safe)
        .bind(delivery.updated_at)
        .bind(delivery.id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }

    async fn list_deliveries_for_payment(
        &self,
        tenant_id: TenantId,
        payment_id: PaymentId,
    ) -> Result<Vec<WebhookDelivery>, RepositoryError> {
        let rows: Vec<DeliveryRow> = sqlx::query_as(
            "SELECT d.id, d.webhook_endpoint_id, d.event_id, d.payload_version, d.status, d.attempt_count,
                    d.next_retry_at, d.response_code, d.last_error_safe, d.created_at, d.updated_at
             FROM webhook_deliveries d
             JOIN webhook_endpoints e ON e.id = d.webhook_endpoint_id
             WHERE e.tenant_id = $1 AND d.payload->'data'->>'payment_id' = $2
             ORDER BY d.created_at",
        )
        .bind(tenant_id.as_uuid())
        .bind(payment_id.as_prefixed())
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn get_endpoint(
        &self,
        endpoint_id: WebhookEndpointId,
    ) -> Result<WebhookEndpoint, RepositoryError> {
        let row: EndpointRow = sqlx::query_as("SELECT * FROM webhook_endpoints WHERE id = $1")
            .bind(endpoint_id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(infra)?
            .ok_or(RepositoryError::NotFound)?;
        row.try_into()
    }
}

#[async_trait]
impl QrNonceStore for PgStore {
    async fn remember_nonce(&self, nonce: &str, ttl_secs: i64) -> Result<bool, RepositoryError> {
        let now = OffsetDateTime::now_utc();
        let expires = now + time::Duration::seconds(ttl_secs);
        let result = sqlx::query(
            "INSERT INTO qr_nonces (nonce, consumed_at, expires_at) VALUES ($1,$2,$3)
             ON CONFLICT (nonce) DO NOTHING",
        )
        .bind(nonce)
        .bind(now)
        .bind(expires)
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(result.rows_affected() == 1)
    }
}

#[async_trait]
impl ApiKeyRepository for PgStore {
    async fn find_by_fingerprint(
        &self,
        fingerprint: &str,
    ) -> Result<Option<ApiKeyRecord>, RepositoryError> {
        let row: Option<ApiKeyRow> = sqlx::query_as(
            "SELECT id, tenant_id, merchant_id, name, hash, fingerprint, scopes, revoked
             FROM api_keys WHERE fingerprint = $1",
        )
        .bind(fingerprint)
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        Ok(row.map(Into::into))
    }

    async fn list_for_merchant(
        &self,
        tenant_id: TenantId,
        merchant_id: MerchantId,
    ) -> Result<Vec<ApiKeyRecord>, RepositoryError> {
        let rows: Vec<ApiKeyRow> = sqlx::query_as(
            "SELECT id, tenant_id, merchant_id, name, hash, fingerprint, scopes, revoked
             FROM api_keys WHERE tenant_id = $1 AND merchant_id = $2",
        )
        .bind(tenant_id.as_uuid())
        .bind(merchant_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn insert(&self, record: ApiKeyRecord) -> Result<(), RepositoryError> {
        let id = Uuid::parse_str(&record.id).unwrap_or_else(|_| Uuid::now_v7());
        sqlx::query(
            "INSERT INTO api_keys (id, tenant_id, merchant_id, name, hash, fingerprint, scopes, revoked, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(id)
        .bind(record.tenant_id.as_uuid())
        .bind(record.merchant_id.map(|m| m.as_uuid()))
        .bind(&record.name)
        .bind(&record.hash)
        .bind(&record.fingerprint)
        .bind(serde_json::json!(record.scopes))
        .bind(record.revoked)
        .bind(OffsetDateTime::now_utc())
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }
}

#[async_trait]
impl RoutingRepository for PgStore {
    async fn get_active_policy(
        &self,
        tenant_id: TenantId,
    ) -> Result<Option<RoutingPolicy>, RepositoryError> {
        let row: Option<PolicyRow> = sqlx::query_as(
            "SELECT * FROM routing_policies WHERE tenant_id = $1 AND status = 'active' ORDER BY updated_at DESC, created_at DESC LIMIT 1",
        )
        .bind(tenant_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        Ok(row.map(Into::into))
    }
}

#[async_trait]
impl ConnectorCatalog for PgStore {
    async fn list_enabled(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<ConnectorSnapshot>, RepositoryError> {
        let rows: Vec<ConnectorCatRow> = sqlx::query_as(
            "SELECT key, health_status, capabilities, priority, status FROM connectors
             WHERE status = 'enabled' AND (tenant_id IS NULL OR tenant_id = $1)",
        )
        .bind(tenant_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn connector_id_by_key(
        &self,
        tenant_id: TenantId,
        key: &str,
    ) -> Result<openpay_domain::ConnectorId, RepositoryError> {
        let row: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM connectors WHERE key = $1 AND (tenant_id IS NULL OR tenant_id = $2) LIMIT 1",
        )
        .bind(key)
        .bind(tenant_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        row.map(|(id,)| openpay_domain::ConnectorId::from_uuid(id))
            .ok_or(RepositoryError::NotFound)
    }
}

#[async_trait]
impl UserRepository for PgStore {
    async fn find_by_email(&self, email: &str) -> Result<Option<AuthUser>, RepositoryError> {
        let row: Option<UserRow> = sqlx::query_as(
            "SELECT id, tenant_id, email, password_hash, role FROM users WHERE email = $1",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        Ok(row.map(Into::into))
    }
}

impl Clock for PgStore {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

impl PgStore {
    pub async fn record_webhook_endpoint_result(
        &self,
        endpoint_id: openpay_domain::WebhookEndpointId,
        success: bool,
    ) -> Result<(), RepositoryError> {
        if success {
            sqlx::query(
                "UPDATE webhook_endpoints SET failure_count = 0, updated_at = $1 WHERE id = $2",
            )
            .bind(OffsetDateTime::now_utc())
            .bind(endpoint_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        } else {
            sqlx::query(
                "UPDATE webhook_endpoints SET failure_count = failure_count + 1, updated_at = $1 WHERE id = $2",
            )
            .bind(OffsetDateTime::now_utc())
            .bind(endpoint_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(infra)?;
        }
        Ok(())
    }

    pub async fn list_recent_deliveries(
        &self,
        limit: i64,
    ) -> Result<Vec<WebhookDelivery>, RepositoryError> {
        let rows: Vec<DeliveryRow> = sqlx::query_as(
            "SELECT id, webhook_endpoint_id, event_id, payload_version, status, attempt_count,
                    next_retry_at, response_code, last_error_safe, created_at, updated_at
             FROM webhook_deliveries ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    pub async fn list_api_keys(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<ApiKeyRecord>, RepositoryError> {
        let rows: Vec<ApiKeyRow> = sqlx::query_as(
            "SELECT id, tenant_id, merchant_id, name, hash, fingerprint, scopes, revoked
             FROM api_keys WHERE tenant_id = $1 ORDER BY created_at DESC",
        )
        .bind(tenant_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn list_webhook_endpoints_admin(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<WebhookEndpoint>, RepositoryError> {
        let rows: Vec<EndpointRow> = sqlx::query_as(
            "SELECT * FROM webhook_endpoints WHERE tenant_id = $1 ORDER BY created_at DESC",
        )
        .bind(tenant_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    pub async fn list_routing_policies(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<openpay_domain::RoutingPolicy>, RepositoryError> {
        let rows: Vec<PolicyRow> = sqlx::query_as(
            "SELECT * FROM routing_policies WHERE tenant_id = $1 ORDER BY created_at DESC",
        )
        .bind(tenant_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Encrypt plaintext `secret://` (and `env:`) connector refs at rest with AES-256-GCM.
    pub async fn encrypt_connector_secret_refs(
        &self,
        master_key: &[u8; 32],
    ) -> Result<u32, RepositoryError> {
        let rows: Vec<(Uuid, String)> =
            sqlx::query_as("SELECT id, configuration_ref FROM connectors")
                .fetch_all(&self.pool)
                .await
                .map_err(infra)?;
        let mut n = 0u32;
        for (id, refer) in rows {
            if openpay_crypto::is_encrypted_envelope(&refer) {
                continue;
            }
            if !openpay_crypto::is_secret_ref(&refer) && !refer.starts_with("env:") {
                continue;
            }
            let sealed = openpay_crypto::seal_if_plaintext(master_key, &refer)
                .map_err(|e| RepositoryError::Infra(e.to_string()))?;
            sqlx::query(
                "UPDATE connectors SET configuration_ref = $1, updated_at = $2 WHERE id = $3",
            )
            .bind(&sealed)
            .bind(OffsetDateTime::now_utc())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(infra)?;
            n += 1;
        }
        Ok(n)
    }
}
