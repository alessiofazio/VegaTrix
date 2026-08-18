use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

use crate::ids::{AuditId, TenantId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: AuditId,
    pub tenant_id: TenantId,
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
