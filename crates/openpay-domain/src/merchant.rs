use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::ids::{MerchantId, TenantId};
use crate::money::Currency;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MerchantStatus {
    Active,
    Suspended,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Merchant {
    pub id: MerchantId,
    pub tenant_id: TenantId,
    pub legal_name: String,
    pub display_name: String,
    pub merchant_reference: String,
    pub country: String,
    pub currency_preferences: Vec<Currency>,
    pub status: MerchantStatus,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}
