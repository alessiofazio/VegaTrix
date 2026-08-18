use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

use crate::ids::{EventId, MerchantId, TenantId, WebhookDeliveryId, WebhookEndpointId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEndpointStatus {
    Active,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    Pending,
    Delivered,
    Retrying,
    DeadLettered,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEndpoint {
    pub id: WebhookEndpointId,
    pub tenant_id: TenantId,
    pub merchant_id: MerchantId,
    pub url: String,
    pub event_types: Vec<String>,
    pub signing_secret_ref: String,
    pub status: WebhookEndpointStatus,
    pub failure_count: i32,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookDelivery {
    pub id: WebhookDeliveryId,
    pub webhook_endpoint_id: WebhookEndpointId,
    pub event_id: EventId,
    pub payload_version: String,
    pub status: DeliveryStatus,
    pub attempt_count: i32,
    pub next_retry_at: Option<OffsetDateTime>,
    pub response_code: Option<i32>,
    pub last_error_safe: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainEvent {
    pub id: EventId,
    pub event_type: String,
    pub api_version: String,
    pub created_at: OffsetDateTime,
    pub data: Value,
}
