use async_trait::async_trait;
use time::OffsetDateTime;

use openpay_connectors::{ConnectorError, SandboxAttemptStore};
use openpay_domain::AttemptStatus;

use crate::map::parse_attempt_status;
use crate::store::PgStore;

#[async_trait]
impl SandboxAttemptStore for PgStore {
    async fn put(
        &self,
        connector_key: &str,
        provider_reference: &str,
        status: AttemptStatus,
    ) -> Result<(), ConnectorError> {
        let now = OffsetDateTime::now_utc();
        sqlx::query(
            "INSERT INTO sandbox_connector_attempts
             (connector_key, provider_reference, status, created_at, updated_at)
             VALUES ($1,$2,$3,$4,$5)
             ON CONFLICT (connector_key, provider_reference)
             DO UPDATE SET status = EXCLUDED.status, updated_at = EXCLUDED.updated_at",
        )
        .bind(connector_key)
        .bind(provider_reference)
        .bind(status.as_str())
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| ConnectorError::Message(e.to_string()))?;
        Ok(())
    }

    async fn get(
        &self,
        connector_key: &str,
        provider_reference: &str,
    ) -> Result<Option<AttemptStatus>, ConnectorError> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT status FROM sandbox_connector_attempts
             WHERE connector_key = $1 AND provider_reference = $2",
        )
        .bind(connector_key)
        .bind(provider_reference)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ConnectorError::Message(e.to_string()))?;
        match row {
            Some((raw,)) => parse_attempt_status(&raw)
                .map(Some)
                .map_err(|e| ConnectorError::Message(e.to_string())),
            None => Ok(None),
        }
    }
}
