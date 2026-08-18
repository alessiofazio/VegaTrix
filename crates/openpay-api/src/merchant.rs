use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::json;
use time::OffsetDateTime;
use validator::Validate;

use openpay_application::{parse_amount, parse_currency, parse_order_id};
use openpay_domain::{CreatePaymentCommand, IdempotencyKey, MerchantId, PaymentId};
use openpay_persistence::seed::DEMO_MERCHANT;

use crate::auth::AuthContext;
use crate::dto::{CreatePaymentBody, PaymentCreatedResponse, PaymentView, parse_methods};
use crate::error::ApiError;
use crate::state::AppState;

fn merchant_from_auth(auth: &AuthContext) -> Result<MerchantId, ApiError> {
    auth.merchant_id
        .or_else(|| Some(MerchantId::from_uuid(DEMO_MERCHANT)))
        .filter(|_| true)
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::FORBIDDEN,
                "forbidden",
                "Forbidden",
                "merchant scope required",
            )
        })
}

#[utoipa::path(
    post,
    path = "/v1/payment-requests",
    tag = "merchant",
    responses((status = 201, description = "Payment created", body = PaymentCreatedResponse))
)]
pub async fn create_payment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    headers: HeaderMap,
    Json(body): Json<CreatePaymentBody>,
) -> Result<(StatusCode, Json<PaymentCreatedResponse>), ApiError> {
    body.validate().map_err(|e| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "validation",
            "Validation failed",
            e.to_string(),
        )
    })?;
    let idem = headers
        .get("Idempotency-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                "Idempotency-Key header is required",
            )
        })?;
    let merchant_id = merchant_from_auth(&auth)?;
    let cmd = CreatePaymentCommand {
        tenant_id: auth.tenant_id,
        merchant_id,
        merchant_order_id: parse_order_id(&body.merchant_order_id).map_err(ApiError::from)?,
        amount_minor: parse_amount(body.amount_minor).map_err(ApiError::from)?,
        currency: parse_currency(&body.currency).map_err(ApiError::from)?,
        description: body.description.clone(),
        allowed_methods: parse_methods(&body.allowed_methods)?,
        expires_in_seconds: body.expires_in_seconds.unwrap_or(300),
        return_url: body.return_url.clone(),
        metadata: {
            let mut meta = body.metadata.clone().unwrap_or_else(|| json!({}));
            if let Some(scenario) = &body.scenario {
                if let serde_json::Value::Object(map) = &mut meta {
                    map.insert("scenario".into(), json!(scenario));
                }
            }
            meta
        },
        idempotency_key: IdempotencyKey::new(idem).map_err(|e| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                e.to_string(),
            )
        })?,
        routing_policy_id: None,
    };
    let created = state
        .payments
        .create_payment(
            cmd,
            headers
                .get("x-request-id")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string),
        )
        .await
        .map_err(ApiError::from)?;
    let status = if created.replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((
        status,
        Json(PaymentCreatedResponse {
            id: created.payment.id.as_prefixed(),
            status: created.payment.status.as_str().into(),
            amount_minor: created.payment.amount_minor.get(),
            currency: created.payment.currency.as_str().into(),
            payment_url: created.payment_url,
            qr_payload: created.qr_payload,
            qr_svg: created.qr_svg,
            expires_at: created.payment.expires_at,
            created_at: created.payment.created_at,
            replayed: created.replayed,
        }),
    ))
}

#[utoipa::path(get, path = "/v1/payment-requests/{payment_id}", tag = "merchant")]
pub async fn get_payment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(payment_id): Path<String>,
) -> Result<Json<PaymentView>, ApiError> {
    let id: PaymentId = payment_id
        .parse()
        .map_err(|e: openpay_domain::DomainError| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                e.to_string(),
            )
        })?;
    let payment = state
        .payments
        .get_payment(auth.tenant_id, id)
        .await
        .map_err(ApiError::from)?;
    if let Some(mid) = auth.merchant_id {
        payment.belongs_to(auth.tenant_id, mid).map_err(|e| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "not-found",
                "Not found",
                e.to_string(),
            )
        })?;
    }
    Ok(Json(PaymentView::from(&payment)))
}

pub async fn cancel_payment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(payment_id): Path<String>,
) -> Result<Json<PaymentView>, ApiError> {
    let id: PaymentId = payment_id
        .parse()
        .map_err(|e: openpay_domain::DomainError| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                e.to_string(),
            )
        })?;
    let payment = state
        .payments
        .cancel_payment(auth.tenant_id, id, &auth.actor_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(PaymentView::from(&payment)))
}

pub async fn refund_payment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(payment_id): Path<String>,
) -> Result<Json<PaymentView>, ApiError> {
    let id: PaymentId = payment_id
        .parse()
        .map_err(|e: openpay_domain::DomainError| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                e.to_string(),
            )
        })?;
    let payment = state
        .payments
        .refund_payment(auth.tenant_id, id, &auth.actor_id)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(PaymentView::from(&payment)))
}

pub async fn list_attempts(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(payment_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: PaymentId = payment_id
        .parse()
        .map_err(|e: openpay_domain::DomainError| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                e.to_string(),
            )
        })?;
    let attempts =
        openpay_application::PaymentRepository::list_attempts(&state.store, auth.tenant_id, id)
            .await
            .map_err(openpay_application::ApplicationError::from)
            .map_err(ApiError::from)?;
    Ok(Json(json!(
        attempts
            .iter()
            .map(|a| json!({
                "id": a.id.as_prefixed(),
                "connector_key": a.connector_key,
                "rail_type": a.rail_type,
                "status": a.status.as_str(),
                "provider_reference": a.provider_reference,
                "failure_code": a.failure_code,
                "created_at": a.created_at
            }))
            .collect::<Vec<_>>()
    )))
}

pub async fn list_events(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(payment_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id: PaymentId = payment_id
        .parse()
        .map_err(|e: openpay_domain::DomainError| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                e.to_string(),
            )
        })?;
    let events =
        openpay_application::AuditRepository::list_for_payment(&state.store, auth.tenant_id, id)
            .await
            .map_err(openpay_application::ApplicationError::from)
            .map_err(ApiError::from)?;
    Ok(Json(json!(
        events
            .iter()
            .map(|e| json!({
                "id": e.id.as_prefixed(),
                "event_type": e.event_type,
                "actor_type": e.actor_type,
                "occurred_at": e.occurred_at,
                "metadata_redacted": e.metadata_redacted
            }))
            .collect::<Vec<_>>()
    )))
}

pub async fn list_payments(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
) -> Result<Json<Vec<PaymentView>>, ApiError> {
    let merchant_id = merchant_from_auth(&auth)?;
    let rows = openpay_application::PaymentRepository::list_by_merchant(
        &state.store,
        auth.tenant_id,
        merchant_id,
        50,
    )
    .await
    .map_err(openpay_application::ApplicationError::from)
    .map_err(ApiError::from)?;
    Ok(Json(rows.iter().map(PaymentView::from).collect()))
}

pub fn public_payment_url(_now: OffsetDateTime, id: &str) -> String {
    format!("/pay/{id}")
}
