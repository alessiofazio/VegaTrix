use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use openpay_application::RepositoryError;
use openpay_domain::{
    ConnectorId, MerchantId, RoutingPolicy, RoutingPolicyId, TenantId, WebhookEndpoint,
    WebhookEndpointId,
};

use crate::map::{EndpointRow, PolicyRow};
use crate::store::PgStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorSettings {
    pub default_currency: String,
    pub qr_ttl_seconds: i64,
    pub webhook_timeout_ms: u64,
    pub rate_limit_per_minute: u64,
    pub cors_allow_origins: Vec<String>,
    pub webhook_url_allowlist: Vec<String>,
    pub feature_connector_mock: bool,
    pub feature_connector_open_banking: bool,
    pub telemetry_opt_in: bool,
}

impl OperatorSettings {
    pub fn overlay_json(&mut self, value: &Value) {
        if let Some(v) = value.get("default_currency").and_then(Value::as_str) {
            if v.len() == 3 {
                self.default_currency = v.to_ascii_uppercase();
            }
        }
        if let Some(v) = value.get("qr_ttl_seconds").and_then(Value::as_i64) {
            self.qr_ttl_seconds = v.clamp(30, 3600);
        }
        if let Some(v) = value.get("webhook_timeout_ms").and_then(Value::as_u64) {
            self.webhook_timeout_ms = v.clamp(500, 60_000);
        }
        if let Some(v) = value.get("rate_limit_per_minute").and_then(Value::as_u64) {
            self.rate_limit_per_minute = v.clamp(1, 10_000);
        }
        if let Some(arr) = value.get("cors_allow_origins").and_then(Value::as_array) {
            self.cors_allow_origins = string_list(arr);
        }
        if let Some(arr) = value.get("webhook_url_allowlist").and_then(Value::as_array) {
            self.webhook_url_allowlist = string_list(arr);
        }
        if let Some(features) = value.get("features") {
            if let Some(v) = features.get("connector_mock").and_then(Value::as_bool) {
                self.feature_connector_mock = v;
            }
            if let Some(v) = features
                .get("connector_open_banking")
                .and_then(Value::as_bool)
            {
                self.feature_connector_open_banking = v;
            }
            if let Some(v) = features.get("telemetry_opt_in").and_then(Value::as_bool) {
                self.telemetry_opt_in = v;
            }
        } else {
            if let Some(v) = value.get("feature_connector_mock").and_then(Value::as_bool) {
                self.feature_connector_mock = v;
            }
            if let Some(v) = value
                .get("feature_connector_open_banking")
                .and_then(Value::as_bool)
            {
                self.feature_connector_open_banking = v;
            }
            if let Some(v) = value.get("telemetry_opt_in").and_then(Value::as_bool) {
                self.telemetry_opt_in = v;
            }
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "default_currency": self.default_currency,
            "qr_ttl_seconds": self.qr_ttl_seconds,
            "webhook_timeout_ms": self.webhook_timeout_ms,
            "rate_limit_per_minute": self.rate_limit_per_minute,
            "cors_allow_origins": self.cors_allow_origins,
            "webhook_url_allowlist": self.webhook_url_allowlist,
            "features": {
                "connector_mock": self.feature_connector_mock,
                "connector_open_banking": self.feature_connector_open_banking,
                "telemetry_opt_in": self.telemetry_opt_in
            }
        })
    }
}

fn string_list(arr: &[Value]) -> Vec<String> {
    arr.iter()
        .filter_map(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[derive(Debug, Clone)]
pub struct ConnectorAdminRow {
    pub id: ConnectorId,
    pub key: String,
    pub name: String,
    pub connector_type: String,
    pub status: String,
    pub configuration_ref: String,
    pub capabilities: Value,
    pub priority: i32,
    pub health_status: String,
}

#[derive(FromRow)]
struct ConnectorAdminSql {
    id: Uuid,
    key: String,
    name: String,
    connector_type: String,
    status: String,
    configuration_ref: String,
    capabilities: Value,
    priority: i32,
    health_status: String,
}

impl From<ConnectorAdminSql> for ConnectorAdminRow {
    fn from(row: ConnectorAdminSql) -> Self {
        Self {
            id: ConnectorId::from_uuid(row.id),
            key: row.key,
            name: row.name,
            connector_type: row.connector_type,
            status: row.status,
            configuration_ref: row.configuration_ref,
            capabilities: row.capabilities,
            priority: row.priority,
            health_status: row.health_status,
        }
    }
}

fn infra(err: sqlx::Error) -> RepositoryError {
    RepositoryError::Infra(err.to_string())
}

impl PgStore {
    pub async fn get_tenant_settings_json(
        &self,
        tenant_id: TenantId,
    ) -> Result<Option<Value>, RepositoryError> {
        let row: Option<(Value,)> =
            sqlx::query_as("SELECT settings FROM tenant_settings WHERE tenant_id = $1")
                .bind(tenant_id.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(infra)?;
        Ok(row.map(|r| r.0))
    }

    pub async fn load_latest_tenant_settings_json(&self) -> Result<Option<Value>, RepositoryError> {
        let row: Option<(Value,)> = sqlx::query_as(
            "SELECT settings FROM tenant_settings ORDER BY updated_at DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        Ok(row.map(|r| r.0))
    }

    pub async fn upsert_tenant_settings_json(
        &self,
        tenant_id: TenantId,
        settings: &Value,
    ) -> Result<(), RepositoryError> {
        let now = OffsetDateTime::now_utc();
        sqlx::query(
            "INSERT INTO tenant_settings (tenant_id, settings, updated_at)
             VALUES ($1,$2,$3)
             ON CONFLICT (tenant_id) DO UPDATE
             SET settings = EXCLUDED.settings, updated_at = EXCLUDED.updated_at",
        )
        .bind(tenant_id.as_uuid())
        .bind(settings)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }

    pub async fn revoke_api_key(
        &self,
        tenant_id: TenantId,
        key_id: Uuid,
    ) -> Result<bool, RepositoryError> {
        let result = sqlx::query(
            "UPDATE api_keys SET revoked = TRUE WHERE id = $1 AND tenant_id = $2 AND revoked = FALSE",
        )
        .bind(key_id)
        .bind(tenant_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn insert_webhook_endpoint(
        &self,
        endpoint: &WebhookEndpoint,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "INSERT INTO webhook_endpoints
             (id, tenant_id, merchant_id, url, event_types, signing_secret_ref, status, failure_count, created_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,0,$8,$9)",
        )
        .bind(endpoint.id.as_uuid())
        .bind(endpoint.tenant_id.as_uuid())
        .bind(endpoint.merchant_id.as_uuid())
        .bind(&endpoint.url)
        .bind(json!(endpoint.event_types))
        .bind(&endpoint.signing_secret_ref)
        .bind(match endpoint.status {
            openpay_domain::WebhookEndpointStatus::Active => "active",
            openpay_domain::WebhookEndpointStatus::Disabled => "disabled",
        })
        .bind(endpoint.created_at)
        .bind(endpoint.updated_at)
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        Ok(())
    }

    pub async fn update_webhook_endpoint(
        &self,
        tenant_id: TenantId,
        endpoint_id: WebhookEndpointId,
        url: Option<&str>,
        event_types: Option<&[String]>,
        status: Option<&str>,
        signing_secret_ref: Option<&str>,
    ) -> Result<WebhookEndpoint, RepositoryError> {
        let now = OffsetDateTime::now_utc();
        let events = event_types.map(|e| json!(e));
        sqlx::query(
            "UPDATE webhook_endpoints SET
                url = COALESCE($3, url),
                event_types = COALESCE($4, event_types),
                status = COALESCE($5, status),
                signing_secret_ref = COALESCE($6, signing_secret_ref),
                updated_at = $7
             WHERE id = $1 AND tenant_id = $2",
        )
        .bind(endpoint_id.as_uuid())
        .bind(tenant_id.as_uuid())
        .bind(url)
        .bind(events)
        .bind(status)
        .bind(signing_secret_ref)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        let row: EndpointRow = sqlx::query_as(
            "SELECT * FROM webhook_endpoints WHERE id = $1 AND tenant_id = $2",
        )
        .bind(endpoint_id.as_uuid())
        .bind(tenant_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?
        .ok_or(RepositoryError::NotFound)?;
        row.try_into()
    }

    pub async fn update_routing_policy(
        &self,
        tenant_id: TenantId,
        policy_id: RoutingPolicyId,
        name: Option<&str>,
        rules_json: Option<&Value>,
        fallback_policy: Option<&Value>,
        status: Option<&str>,
    ) -> Result<RoutingPolicy, RepositoryError> {
        let now = OffsetDateTime::now_utc();
        sqlx::query(
            "UPDATE routing_policies SET
                name = COALESCE($3, name),
                rules_json = COALESCE($4, rules_json),
                fallback_policy = COALESCE($5, fallback_policy),
                status = COALESCE($6, status),
                updated_at = $7
             WHERE id = $1 AND tenant_id = $2",
        )
        .bind(policy_id.as_uuid())
        .bind(tenant_id.as_uuid())
        .bind(name)
        .bind(rules_json)
        .bind(fallback_policy)
        .bind(status)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        let row: PolicyRow = sqlx::query_as(
            "SELECT * FROM routing_policies WHERE id = $1 AND tenant_id = $2",
        )
        .bind(policy_id.as_uuid())
        .bind(tenant_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?
        .ok_or(RepositoryError::NotFound)?;
        Ok(row.into())
    }

    pub async fn list_connectors_admin(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<ConnectorAdminRow>, RepositoryError> {
        let rows: Vec<ConnectorAdminSql> = sqlx::query_as(
            "SELECT id, key, name, connector_type, status, configuration_ref, capabilities, priority, health_status
             FROM connectors
             WHERE tenant_id IS NULL OR tenant_id = $1
             ORDER BY priority DESC, key",
        )
        .bind(tenant_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn update_connector_admin(
        &self,
        tenant_id: TenantId,
        key: &str,
        status: Option<&str>,
        configuration_ref: Option<&str>,
    ) -> Result<ConnectorAdminRow, RepositoryError> {
        let now = OffsetDateTime::now_utc();
        sqlx::query(
            "UPDATE connectors SET
                status = COALESCE($3, status),
                configuration_ref = COALESCE($4, configuration_ref),
                updated_at = $5
             WHERE key = $1 AND (tenant_id IS NULL OR tenant_id = $2)",
        )
        .bind(key)
        .bind(tenant_id.as_uuid())
        .bind(status)
        .bind(configuration_ref)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        let row: ConnectorAdminSql = sqlx::query_as(
            "SELECT id, key, name, connector_type, status, configuration_ref, capabilities, priority, health_status
             FROM connectors
             WHERE key = $1 AND (tenant_id IS NULL OR tenant_id = $2)
             LIMIT 1",
        )
        .bind(key)
        .bind(tenant_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?
        .ok_or(RepositoryError::NotFound)?;
        Ok(row.into())
    }

    pub async fn first_merchant_id(
        &self,
        tenant_id: TenantId,
    ) -> Result<MerchantId, RepositoryError> {
        let row: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM merchants WHERE tenant_id = $1 ORDER BY created_at ASC LIMIT 1",
        )
        .bind(tenant_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        row.map(|(id,)| MerchantId::from_uuid(id))
            .ok_or(RepositoryError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::OperatorSettings;
    use serde_json::json;

    #[test]
    fn overlay_clamps_and_features() {
        let mut s = OperatorSettings {
            default_currency: "EUR".into(),
            qr_ttl_seconds: 300,
            webhook_timeout_ms: 5000,
            rate_limit_per_minute: 120,
            cors_allow_origins: vec!["http://localhost:3001".into()],
            webhook_url_allowlist: vec!["localhost".into()],
            feature_connector_mock: true,
            feature_connector_open_banking: false,
            telemetry_opt_in: false,
        };
        s.overlay_json(&json!({
            "default_currency": "usd",
            "qr_ttl_seconds": 10,
            "features": { "connector_mock": false, "telemetry_opt_in": true }
        }));
        assert_eq!(s.default_currency, "USD");
        assert_eq!(s.qr_ttl_seconds, 30);
        assert!(!s.feature_connector_mock);
        assert!(s.telemetry_opt_in);
    }
}
