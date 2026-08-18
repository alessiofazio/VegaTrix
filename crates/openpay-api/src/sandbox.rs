use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use openpay_application::{
    PayerDecision, authorize_payment, parse_amount, parse_currency, parse_order_id,
    replay_connector_callback,
};
use openpay_domain::{CreatePaymentCommand, IdempotencyKey, PaymentId, PaymentMethod};

use crate::auth::AuthContext;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SandboxPaymentBody {
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub description: Option<String>,
    pub scenario: Option<String>,
    pub allowed_methods: Option<Vec<String>>,
    pub expires_in_seconds: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct SandboxAuthorizeBody {
    pub decision: String,
    pub scenario: Option<String>,
}

pub async fn sandbox_status(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    crate::admin::require_admin(&auth)?;
    Ok(Json(sandbox_availability(&state)))
}

pub async fn create_sandbox_payment(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<SandboxPaymentBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    crate::admin::require_admin(&auth)?;
    crate::admin::require_sandbox(&state)?;
    let merchant_id = state
        .store
        .first_merchant_id(auth.tenant_id)
        .await
        .map_err(openpay_application::ApplicationError::from)
        .map_err(ApiError::from)?;
    let op = state.operator_snapshot();
    let amount = parse_amount(body.amount_minor.unwrap_or(1200)).map_err(ApiError::from)?;
    let currency_raw = body
        .currency
        .filter(|c| !c.is_empty())
        .unwrap_or(op.default_currency.clone());
    let currency = parse_currency(&currency_raw).map_err(ApiError::from)?;
    let ttl = body
        .expires_in_seconds
        .unwrap_or(op.qr_ttl_seconds.clamp(30, 3600) as u32);
    let methods = crate::dto::parse_methods(&body.allowed_methods)?;
    let methods = if methods.is_empty() {
        vec![PaymentMethod::AccountToAccount]
    } else {
        methods
    };
    let mut metadata = json!({ "sandbox_lab": true, "source": "dashboard" });
    if let Some(scenario) = &body.scenario {
        if let serde_json::Value::Object(map) = &mut metadata {
            map.insert("scenario".into(), json!(scenario));
        }
    }
    let idem = format!("sandbox-{}", Uuid::now_v7());
    let cmd = CreatePaymentCommand {
        tenant_id: auth.tenant_id,
        merchant_id,
        merchant_order_id: parse_order_id(&format!("LAB-{}", Uuid::now_v7().as_simple()))
            .map_err(ApiError::from)?,
        amount_minor: amount,
        currency,
        description: body
            .description
            .or_else(|| Some("Laboratorio sandbox dashboard".into())),
        allowed_methods: methods,
        expires_in_seconds: ttl.max(30),
        return_url: None,
        metadata,
        idempotency_key: IdempotencyKey::new(idem).map_err(|e| {
            ApiError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                e.to_string(),
            )
        })?,
        routing_policy_id: None,
    };
    let created = state
        .payments
        .create_payment(cmd, Some(auth.actor_id.clone()))
        .await
        .map_err(ApiError::from)?;
    Ok(Json(json!({
        "id": created.payment.id.as_prefixed(),
        "status": created.payment.status.as_str(),
        "amount_minor": created.payment.amount_minor.get(),
        "currency": created.payment.currency.as_str(),
        "merchant_order_id": created.payment.merchant_order_id.as_str(),
        "payment_url": created.payment_url,
        "qr_payload": created.qr_payload,
        "qr_svg": created.qr_svg,
        "qr_token": created.qr_token,
        "expires_at": created.payment.expires_at,
        "replayed": created.replayed
    })))
}

pub async fn sandbox_authorize(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(payment_id): Path<String>,
    Json(body): Json<SandboxAuthorizeBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    crate::admin::require_admin(&auth)?;
    crate::admin::require_sandbox(&state)?;
    let id: PaymentId = payment_id
        .parse()
        .map_err(|e: openpay_domain::DomainError| {
            ApiError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                e.to_string(),
            )
        })?;
    let decision = if body.decision.eq_ignore_ascii_case("reject") {
        PayerDecision::Reject
    } else {
        PayerDecision::Approve
    };
    let outcome = authorize_payment(
        &state.payments,
        &state.connectors.registry,
        auth.tenant_id,
        id,
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

pub async fn sandbox_duplicate(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(payment_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    crate::admin::require_admin(&auth)?;
    crate::admin::require_sandbox(&state)?;
    let id: PaymentId = payment_id
        .parse()
        .map_err(|e: openpay_domain::DomainError| {
            ApiError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                e.to_string(),
            )
        })?;
    let outcome = replay_connector_callback(
        &state.payments,
        &state.connectors.registry,
        auth.tenant_id,
        id,
    )
    .await
    .map_err(ApiError::from)?;
    Ok(Json(json!({
        "payment_id": outcome.payment.id.as_prefixed(),
        "status": outcome.payment.status.as_str(),
        "duplicate_ignored": outcome.idempotent_replay,
        "detail": outcome.explanation
    })))
}

pub fn sandbox_availability(state: &AppState) -> serde_json::Value {
    if state.config.is_production() {
        return json!({
            "available": false,
            "reason": "production",
            "message": "Laboratorio sandbox disabilitato: APP_ENV=production"
        });
    }
    if !state.config.features.connector_mock {
        return json!({
            "available": false,
            "reason": "mock_env",
            "message": "Laboratorio sandbox richiede FEATURE_CONNECTOR_MOCK=true nel .env e un riavvio"
        });
    }
    if !state.operator_snapshot().feature_connector_mock {
        return json!({
            "available": false,
            "reason": "mock_tenant",
            "message": "Flag mock disattivato nelle impostazioni tenant"
        });
    }
    json!({
        "available": true,
        "reason": serde_json::Value::Null,
        "message": "Laboratorio sandbox attivo (connettori mock/manual, non un PSP live)"
    })
}
