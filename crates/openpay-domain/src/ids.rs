use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use uuid::Uuid;

use crate::error::DomainError;

macro_rules! prefixed_id {
    ($name:ident, $prefix:expr) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub const PREFIX: &'static str = $prefix;

            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            pub fn from_uuid(id: Uuid) -> Self {
                Self(id)
            }

            pub fn as_uuid(self) -> Uuid {
                self.0
            }

            pub fn as_prefixed(&self) -> String {
                format!("{}_{}", $prefix, self.0.as_simple())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.as_prefixed())
            }
        }

        impl FromStr for $name {
            type Err = DomainError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                parse_prefixed($prefix, s).map(Self)
            }
        }
    };
}

fn parse_prefixed(prefix: &str, raw: &str) -> Result<Uuid, DomainError> {
    let expected = format!("{prefix}_");
    let uuid_part = if let Some(rest) = raw.strip_prefix(&expected) {
        rest
    } else if raw.len() == 32 || raw.len() == 36 {
        raw
    } else {
        return Err(DomainError::InvalidId {
            prefix: prefix.to_string(),
            value: raw.to_string(),
        });
    };
    Uuid::parse_str(uuid_part).map_err(|_| DomainError::InvalidId {
        prefix: prefix.to_string(),
        value: raw.to_string(),
    })
}

prefixed_id!(PaymentId, "pay");
prefixed_id!(TenantId, "ten");
prefixed_id!(MerchantId, "mch");
prefixed_id!(AttemptId, "att");
prefixed_id!(ConnectorId, "con");
prefixed_id!(EventId, "evt");
prefixed_id!(WebhookEndpointId, "whe");
prefixed_id!(WebhookDeliveryId, "whd");
prefixed_id!(AuditId, "aud");
prefixed_id!(RoutingPolicyId, "rpl");
prefixed_id!(ApiKeyId, "key");
prefixed_id!(UserId, "usr");
prefixed_id!(OutboxId, "otx");
prefixed_id!(RefundId, "rfd");

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.is_empty() || value.len() > 128 {
            return Err(DomainError::InvalidIdempotencyKey);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MerchantOrderId(String);

impl MerchantOrderId {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.is_empty() || value.len() > 128 {
            return Err(DomainError::InvalidMerchantOrderId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConnectorKey(String);

impl ConnectorKey {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.is_empty() || value.len() > 64 {
            return Err(DomainError::InvalidConnectorId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ConnectorKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
