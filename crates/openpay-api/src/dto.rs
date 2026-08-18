use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use validator::Validate;

use openpay_domain::{PaymentMethod, PaymentRequest, PaymentStatus};

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreatePaymentBody {
    #[validate(length(min = 1, max = 128))]
    pub merchant_order_id: String,
    #[validate(range(min = 1))]
    pub amount_minor: i64,
    #[validate(length(equal = 3))]
    pub currency: String,
    pub description: Option<String>,
    pub allowed_methods: Option<Vec<String>>,
    #[validate(range(min = 30, max = 3600))]
    pub expires_in_seconds: Option<u32>,
    pub return_url: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub scenario: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaymentCreatedResponse {
    pub id: String,
    pub status: String,
    pub amount_minor: i64,
    pub currency: String,
    pub payment_url: String,
    pub qr_payload: String,
    pub qr_svg: String,
    pub expires_at: OffsetDateTime,
    pub created_at: OffsetDateTime,
    pub replayed: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PaymentView {
    pub id: String,
    pub status: String,
    pub amount_minor: i64,
    pub currency: String,
    pub merchant_order_id: String,
    pub merchant_id: String,
    pub expires_at: OffsetDateTime,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub description: Option<String>,
    pub metadata: serde_json::Value,
}

impl From<&PaymentRequest> for PaymentView {
    fn from(p: &PaymentRequest) -> Self {
        Self {
            id: p.id.as_prefixed(),
            status: p.status.as_str().into(),
            amount_minor: p.amount_minor.get(),
            currency: p.currency.as_str().into(),
            merchant_order_id: p.merchant_order_id.as_str().into(),
            merchant_id: p.merchant_id.as_prefixed(),
            expires_at: p.expires_at,
            created_at: p.created_at,
            updated_at: p.updated_at,
            description: p.description.clone(),
            metadata: p.metadata.clone(),
        }
    }
}

pub fn parse_methods(raw: &Option<Vec<String>>) -> Result<Vec<PaymentMethod>, crate::error::ApiError> {
    let Some(items) = raw else {
        return Ok(vec![PaymentMethod::AccountToAccount]);
    };
    let mut out = Vec::new();
    for item in items {
        out.push(match item.as_str() {
            "ACCOUNT_TO_ACCOUNT" => PaymentMethod::AccountToAccount,
            "CARD" => PaymentMethod::Card,
            "WALLET" => PaymentMethod::Wallet,
            "MANUAL" => PaymentMethod::Manual,
            other => {
                return Err(crate::error::ApiError::new(
                    axum::http::StatusCode::BAD_REQUEST,
                    "validation",
                    "Validation failed",
                    format!("unknown method {other}"),
                ))
            }
        });
    }
    Ok(out)
}

pub fn status_label(status: PaymentStatus) -> &'static str {
    status.as_str()
}
