use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use serde_json::json;

use openpay_application::{PaymentRepository, reconcile_payment};
use openpay_domain::{AttemptId, PaymentId};

use crate::auth::AuthContext;
use crate::error::ApiError;
use crate::public::ResolveBody;
use crate::state::AppState;

pub async fn overview(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    let merchants =
        openpay_application::MerchantRepository::list_merchants(&state.store, auth.tenant_id)
            .await
            .map_err(openpay_application::ApplicationError::from)
            .map_err(ApiError::from)?;
    let payments = if let Some(m) = merchants.first() {
        PaymentRepository::list_by_merchant(&state.store, auth.tenant_id, m.id, 100)
            .await
            .map_err(openpay_application::ApplicationError::from)
            .map_err(ApiError::from)?
    } else {
        Vec::new()
    };
    let mut counts = json!({});
    for p in &payments {
        let key = p.status.as_str();
        let n = counts.get(key).and_then(|v| v.as_u64()).unwrap_or(0) + 1;
        counts[key] = json!(n);
    }
    Ok(Json(json!({
        "edition": state.config.edition.as_str(),
        "self_hosted": state.config.self_hosted,
        "capabilities": {
            "advanced_routing": state.config.capabilities().advanced_routing,
            "analytics": state.config.capabilities().analytics,
            "sso": state.config.capabilities().sso,
            "connector_open_banking": state.config.capabilities().connector_open_banking
        },
        "payment_counts": counts,
        "payments": payments.iter().map(crate::dto::PaymentView::from).collect::<Vec<_>>(),
        "merchants": merchants.iter().map(|m| json!({
            "id": m.id.as_prefixed(),
            "display_name": m.display_name,
            "status": format!("{:?}", m.status).to_lowercase()
        })).collect::<Vec<_>>()
    })))
}

pub async fn connectors(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    let mut items = Vec::new();
    for connector in state.connectors.registry.all() {
        let health = connector.health_check().await.ok();
        items.push(json!({
            "key": connector.key(),
            "health": health,
            "capabilities": connector.capabilities()
        }));
    }
    Ok(Json(json!({ "connectors": items })))
}

pub async fn settings(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    Ok(Json(json!({
        "app_name": state.config.app_name,
        "environment": state.config.environment,
        "edition": state.config.edition.as_str(),
        "self_hosted": state.config.self_hosted,
        "deployment": if state.config.self_hosted { "self-hosted" } else { "cloud" }
    })))
}

pub async fn list_api_keys(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    let keys = state
        .store
        .list_api_keys(auth.tenant_id)
        .await
        .map_err(openpay_application::ApplicationError::from)
        .map_err(ApiError::from)?;
    Ok(Json(json!(
        keys.iter()
            .map(|k| json!({
                "id": k.id,
                "name": k.name,
                "fingerprint": k.fingerprint,
                "revoked": k.revoked,
                "scopes": k.scopes
            }))
            .collect::<Vec<_>>()
    )))
}

pub async fn list_webhook_endpoints(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    let rows = state
        .store
        .list_webhook_endpoints_admin(auth.tenant_id)
        .await
        .map_err(openpay_application::ApplicationError::from)
        .map_err(ApiError::from)?;
    Ok(Json(json!(
        rows.iter()
            .map(|e| json!({
                "id": e.id.as_prefixed(),
                "url": e.url,
                "status": format!("{:?}", e.status).to_lowercase(),
                "failure_count": e.failure_count,
                "event_types": e.event_types
            }))
            .collect::<Vec<_>>()
    )))
}

pub async fn list_webhook_deliveries(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    let rows = state
        .store
        .list_recent_deliveries(50)
        .await
        .map_err(openpay_application::ApplicationError::from)
        .map_err(ApiError::from)?;
    Ok(Json(json!(
        rows.iter()
            .map(|d| json!({
                "id": d.id.as_prefixed(),
                "event_id": d.event_id.as_prefixed(),
                "status": format!("{:?}", d.status).to_lowercase(),
                "attempt_count": d.attempt_count,
                "response_code": d.response_code,
                "last_error_safe": d.last_error_safe
            }))
            .collect::<Vec<_>>()
    )))
}

pub async fn list_routing_policies(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    let rows = state
        .store
        .list_routing_policies(auth.tenant_id)
        .await
        .map_err(openpay_application::ApplicationError::from)
        .map_err(ApiError::from)?;
    Ok(Json(json!(
        rows.iter()
            .map(|p| json!({
                "id": p.id.as_prefixed(),
                "name": p.name,
                "status": format!("{:?}", p.status).to_lowercase(),
                "rules_json": p.rules_json,
                "fallback_policy": p.fallback_policy
            }))
            .collect::<Vec<_>>()
    )))
}

pub async fn reconcile_payment_admin(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(payment_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
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
    let updated = reconcile_payment(
        &state.payments,
        &state.connectors.registry,
        auth.tenant_id,
        id,
    )
    .await
    .map_err(ApiError::from)?;
    Ok(Json(json!({
        "id": updated.id.as_prefixed(),
        "status": updated.status.as_str()
    })))
}

pub async fn resolve_manual_attempt_admin(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(attempt_id): Path<String>,
    Json(body): Json<ResolveBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    let attempt_id: AttemptId = attempt_id
        .parse()
        .map_err(|e: openpay_domain::DomainError| {
            ApiError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                e.to_string(),
            )
        })?;
    let manual = state.connectors.manual.as_ref().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "connector",
            "Manual connector unavailable",
            "manual-test not registered",
        )
    })?;
    let attempt = PaymentRepository::get_attempt(&state.store, auth.tenant_id, attempt_id)
        .await
        .map_err(openpay_application::ApplicationError::from)
        .map_err(ApiError::from)?;
    let provider_ref = attempt.provider_reference.clone().ok_or_else(|| {
        ApiError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "validation",
            "Missing provider reference",
            "attempt has no provider_reference",
        )
    })?;
    manual
        .resolve(&provider_ref, body.approve)
        .await
        .map_err(|e| {
            ApiError::new(
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                "connector",
                "Resolve failed",
                e.to_string(),
            )
        })?;
    let payment = state
        .payments
        .get_payment(attempt.tenant_id, attempt.payment_request_id)
        .await
        .map_err(ApiError::from)?;
    let next = if body.approve {
        openpay_domain::PaymentStatus::Settled
    } else {
        openpay_domain::PaymentStatus::Failed
    };
    let event = if body.approve {
        "payment.settled"
    } else {
        "payment.failed"
    };
    let updated = state
        .payments
        .apply_status(&payment, next, "admin", &auth.actor_id, event)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(json!({
        "attempt_id": attempt_id.as_prefixed(),
        "payment_id": updated.id.as_prefixed(),
        "status": updated.status.as_str()
    })))
}

fn require_admin(auth: &AuthContext) -> Result<(), ApiError> {
    if auth.role == "admin" || auth.scopes.iter().any(|s| s == "admin") {
        Ok(())
    } else {
        Err(ApiError::new(
            axum::http::StatusCode::FORBIDDEN,
            "forbidden",
            "Forbidden",
            "admin role required",
        ))
    }
}
