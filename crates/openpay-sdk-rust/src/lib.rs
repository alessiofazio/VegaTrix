use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

#[derive(Debug, Error)]
pub enum SdkError {
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("url: {0}")]
    Url(#[from] url::ParseError),
    #[error("api status {status}: {body}")]
    Api { status: u16, body: String },
}

#[derive(Clone)]
pub struct OpenPayClient {
    http: Client,
    base: Url,
    token: String,
}

impl OpenPayClient {
    pub fn new(base_url: &str, bearer_token: impl Into<String>) -> Result<Self, SdkError> {
        Ok(Self {
            http: Client::builder().use_rustls_tls().build()?,
            base: Url::parse(base_url)?,
            token: bearer_token.into(),
        })
    }

    pub async fn create_payment(
        &self,
        idempotency_key: &str,
        body: CreatePaymentRequest,
    ) -> Result<CreatePaymentResponse, SdkError> {
        let url = self.base.join("/v1/payment-requests")?;
        let response = self
            .http
            .post(url)
            .bearer_auth(&self.token)
            .header("Idempotency-Key", idempotency_key)
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        let bytes = response.bytes().await?;
        if !status.is_success() {
            return Err(SdkError::Api {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&bytes).into(),
            });
        }
        serde_json::from_slice(&bytes).map_err(|e| SdkError::Api {
            status: 0,
            body: e.to_string(),
        })
    }

    pub async fn get_payment(&self, payment_id: &str) -> Result<CreatePaymentResponse, SdkError> {
        let url = self
            .base
            .join(&format!("/v1/payment-requests/{payment_id}"))?;
        let response = self.http.get(url).bearer_auth(&self.token).send().await?;
        let status = response.status();
        let bytes = response.bytes().await?;
        if !status.is_success() {
            return Err(SdkError::Api {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&bytes).into(),
            });
        }
        serde_json::from_slice(&bytes).map_err(|e| SdkError::Api {
            status: 0,
            body: e.to_string(),
        })
    }
}

#[derive(Debug, Serialize)]
pub struct CreatePaymentRequest {
    pub merchant_order_id: String,
    pub amount_minor: i64,
    pub currency: String,
    pub description: Option<String>,
    pub allowed_methods: Option<Vec<String>>,
    pub expires_in_seconds: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePaymentResponse {
    pub id: String,
    pub status: String,
    pub amount_minor: i64,
    pub currency: String,
    pub payment_url: Option<String>,
    pub qr_payload: Option<String>,
    pub expires_at: Option<String>,
}
