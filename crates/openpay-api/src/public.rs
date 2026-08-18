use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use serde_json::json;
use time::OffsetDateTime;

use openpay_application::{
    PayerDecision, authorize_payment, replay_connector_callback, verify_qr_token,
};
use openpay_domain::{PaymentId, TenantId};

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct PublicQuery {
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct AuthorizeBody {
    pub token: String,
    pub decision: String,
    pub scenario: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResolveBody {
    pub approve: bool,
}

pub async fn public_get(
    State(state): State<Arc<AppState>>,
    Path(payment_id): Path<String>,
    Query(query): Query<PublicQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_payment_id(&payment_id)?;
    let verified = verify_qr_token(
        &state.store,
        state.config.qr_signing_secret.as_bytes(),
        &query.token,
        OffsetDateTime::now_utc(),
        Some(id),
        None,
        false,
    )
    .await
    .map_err(ApiError::from)?;
    let payment = state
        .payments
        .get_payment(verified.tenant_id, verified.payment_id)
        .await
        .map_err(ApiError::from)?;
    let merchant = openpay_application::MerchantRepository::get_merchant(
        &state.store,
        payment.tenant_id,
        payment.merchant_id,
    )
    .await
    .map_err(openpay_application::ApplicationError::from)
    .map_err(ApiError::from)?;
    Ok(Json(json!({
        "id": payment.id.as_prefixed(),
        "merchant_display_name": merchant.display_name,
        "amount_minor": payment.amount_minor.get(),
        "currency": payment.currency.as_str(),
        "status": payment.status.as_str(),
        "expires_at": payment.expires_at,
        "description": payment.description
    })))
}

pub async fn public_authorize(
    State(state): State<Arc<AppState>>,
    Path(payment_id): Path<String>,
    Json(body): Json<AuthorizeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_payment_id(&payment_id)?;
    let verified = verify_qr_token(
        &state.store,
        state.config.qr_signing_secret.as_bytes(),
        &body.token,
        OffsetDateTime::now_utc(),
        Some(id),
        None,
        true,
    )
    .await
    .map_err(ApiError::from)?;

    let decision = if body.decision.eq_ignore_ascii_case("reject") {
        PayerDecision::Reject
    } else {
        PayerDecision::Approve
    };

    let outcome = authorize_payment(
        &state.payments,
        &state.connectors.registry,
        verified.tenant_id,
        verified.payment_id,
        decision,
        body.scenario,
    )
    .await
    .map_err(ApiError::from)?;

    Ok(Json(json!({
        "id": outcome.payment.id.as_prefixed(),
        "status": outcome.payment.status.as_str(),
        "idempotent_replay": outcome.idempotent_replay,
        "routing": {
            "connector": outcome.connector_key,
            "explanation": outcome.explanation
        }
    })))
}

pub async fn simulate_duplicate_callback(
    State(state): State<Arc<AppState>>,
    Path(payment_id): Path<String>,
    Json(_body): Json<AuthorizeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_payment_id(&payment_id)?;
    // Resolve tenant by loading payment with demo tenant first, then without strict tenant if needed
    let tenant_id = match state
        .payments
        .get_payment(
            TenantId::from_uuid(openpay_persistence::seed::DEMO_TENANT),
            id,
        )
        .await
    {
        Ok(p) => p.tenant_id,
        Err(_) => {
            // Any tenant: scan is expensive; demo uses fixed tenant
            TenantId::from_uuid(openpay_persistence::seed::DEMO_TENANT)
        }
    };

    let outcome =
        replay_connector_callback(&state.payments, &state.connectors.registry, tenant_id, id)
            .await
            .map_err(ApiError::from)?;

    Ok(Json(json!({
        "payment_id": outcome.payment.id.as_prefixed(),
        "status": outcome.payment.status.as_str(),
        "duplicate_ignored": outcome.idempotent_replay,
        "detail": outcome.explanation
    })))
}

pub async fn qr_png(
    State(state): State<Arc<AppState>>,
    Path(payment_id): Path<String>,
    Query(query): Query<PublicQuery>,
) -> Result<axum::response::Response, ApiError> {
    let id = parse_payment_id(&payment_id)?;
    let verified = verify_qr_token(
        &state.store,
        state.config.qr_signing_secret.as_bytes(),
        &query.token,
        OffsetDateTime::now_utc(),
        Some(id),
        None,
        false,
    )
    .await
    .map_err(ApiError::from)?;
    let payment = state
        .payments
        .get_payment(verified.tenant_id, verified.payment_id)
        .await
        .map_err(ApiError::from)?;
    let presented = state
        .payments
        .present(&payment, true)
        .map_err(ApiError::from)?;
    let svg = presented.qr_svg.into_bytes();
    let mut response = axum::response::Response::new(svg.into());
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        "image/svg+xml".parse().unwrap(),
    );
    Ok(response)
}

fn parse_payment_id(raw: &str) -> Result<PaymentId, ApiError> {
    raw.parse().map_err(|e: openpay_domain::DomainError| {
        ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "validation",
            "Validation failed",
            e.to_string(),
        )
    })
}
