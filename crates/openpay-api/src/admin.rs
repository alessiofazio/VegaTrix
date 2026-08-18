use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use openpay_application::{
    ApiKeyRecord, PaymentRepository, WebhookRepository, reconcile_payment,
};
use openpay_crypto::{
    api_key_fingerprint, decode_master_key, generate_api_key, generate_webhook_secret, hash_secret,
    is_encrypted_envelope, is_secret_ref, open_secret_value, seal_if_plaintext,
};
use openpay_domain::{
    ApiKeyId, AttemptId, MerchantId, PaymentId, RoutingPolicyId, WebhookEndpoint,
    WebhookEndpointId, WebhookEndpointStatus,
};
use openpay_persistence::OperatorSettings;
use time::OffsetDateTime;

use crate::auth::AuthContext;
use crate::error::ApiError;
use crate::public::ResolveBody;
use crate::sandbox::sandbox_availability;
use crate::state::{AppState, operator_from_config};

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
    let rows = state
        .store
        .list_connectors_admin(auth.tenant_id)
        .await
        .map_err(openpay_application::ApplicationError::from)
        .map_err(ApiError::from)?;
    let mut items = Vec::new();
    for row in rows {
        let live = state.connectors.registry.get(&row.key);
        let live_health = if let Some(ref connector) = live {
            connector.health_check().await.ok()
        } else {
            None
        };
        let (kind, display) =
            redact_configuration_ref(&row.configuration_ref, &state.config.encryption_master_key);
        items.push(json!({
            "id": row.id.as_prefixed(),
            "key": row.key,
            "name": row.name,
            "connector_type": row.connector_type,
            "status": row.status,
            "health": live_health,
            "health_status": row.health_status,
            "registered": live.is_some(),
            "sandbox_only": row.capabilities.get("sandbox_only").and_then(|v| v.as_bool()).unwrap_or(true),
            "capabilities": row.capabilities,
            "priority": row.priority,
            "configuration_kind": kind,
            "configuration_ref": display
        }));
    }
    Ok(Json(json!({
        "connectors": items,
        "note": "Enable/disable and secret:// or env: refs only. This does not configure a live PSP such as Stripe."
    })))
}

#[derive(Debug, Deserialize)]
pub struct ConnectorPatch {
    pub status: Option<String>,
    pub configuration_ref: Option<String>,
}

pub async fn update_connector(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(key): Path<String>,
    Json(body): Json<ConnectorPatch>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    let status = match body.status.as_deref() {
        None => None,
        Some("enabled") | Some("active") => Some("enabled"),
        Some("disabled") => Some("disabled"),
        Some(other) => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                format!("unknown connector status {other}"),
            ));
        }
    };
    let sealed = if let Some(raw) = body.configuration_ref.as_deref() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            validate_connector_ref(trimmed)?;
            Some(seal_connector_ref(
                trimmed,
                &state.config.encryption_master_key,
            )?)
        }
    } else {
        None
    };
    let row = state
        .store
        .update_connector_admin(auth.tenant_id, &key, status, sealed.as_deref())
        .await
        .map_err(openpay_application::ApplicationError::from)
        .map_err(ApiError::from)?;
    let (kind, display) =
        redact_configuration_ref(&row.configuration_ref, &state.config.encryption_master_key);
    Ok(Json(json!({
        "key": row.key,
        "status": row.status,
        "configuration_kind": kind,
        "configuration_ref": display
    })))
}

pub async fn settings(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    let stored = state
        .store
        .get_tenant_settings_json(auth.tenant_id)
        .await
        .map_err(openpay_application::ApplicationError::from)
        .map_err(ApiError::from)?;
    let mut effective = operator_from_config(&state.config);
    if let Some(json) = &stored {
        effective.overlay_json(json);
    }
    Ok(Json(settings_view(&state, &effective, stored.as_ref())))
}

#[derive(Debug, Deserialize)]
pub struct OperatorPatch {
    pub default_currency: Option<String>,
    pub qr_ttl_seconds: Option<i64>,
    pub webhook_timeout_ms: Option<u64>,
    pub rate_limit_per_minute: Option<u64>,
    pub cors_allow_origins: Option<Vec<String>>,
    pub webhook_url_allowlist: Option<Vec<String>>,
    pub features: Option<FeaturePatch>,
}

#[derive(Debug, Deserialize)]
pub struct FeaturePatch {
    pub connector_mock: Option<bool>,
    pub connector_open_banking: Option<bool>,
    pub telemetry_opt_in: Option<bool>,
}

pub async fn update_settings(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<OperatorPatch>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    if state.config.is_production() {
        if body.features.as_ref().and_then(|f| f.connector_mock) == Some(true) {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "forbidden",
                "Forbidden",
                "cannot enable the mock connector when APP_ENV=production",
            ));
        }
    }
    let mut stored = state
        .store
        .get_tenant_settings_json(auth.tenant_id)
        .await
        .map_err(openpay_application::ApplicationError::from)
        .map_err(ApiError::from)?
        .unwrap_or_else(|| json!({}));
    apply_operator_patch(&mut stored, &body)?;
    let mut effective = operator_from_config(&state.config);
    effective.overlay_json(&stored);
    state
        .store
        .upsert_tenant_settings_json(auth.tenant_id, &stored)
        .await
        .map_err(openpay_application::ApplicationError::from)
        .map_err(ApiError::from)?;
    state.replace_operator(effective.clone());
    Ok(Json(settings_view(&state, &effective, Some(&stored))))
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
                "id": api_key_prefixed(&k.id),
                "name": k.name,
                "fingerprint": k.fingerprint,
                "revoked": k.revoked,
                "scopes": k.scopes,
                "merchant_id": k.merchant_id.map(|m| m.as_prefixed())
            }))
            .collect::<Vec<_>>()
    )))
}

#[derive(Debug, Deserialize)]
pub struct CreateApiKeyBody {
    pub name: String,
    pub merchant_id: Option<String>,
}

pub async fn create_api_key(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<CreateApiKeyBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    let name = body.name.trim();
    if name.is_empty() || name.len() > 128 {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "validation",
            "Validation failed",
            "name must be 1–128 characters",
        ));
    }
    let merchant_id = if let Some(raw) = body.merchant_id {
        raw.parse::<MerchantId>().map_err(|e| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                e.to_string(),
            )
        })?
    } else {
        state
            .store
            .first_merchant_id(auth.tenant_id)
            .await
            .map_err(openpay_application::ApplicationError::from)
            .map_err(ApiError::from)?
    };
    let plaintext = generate_api_key();
    let hash = hash_secret(&plaintext).map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "Internal error",
            "could not hash api key",
        )
    })?;
    let id = Uuid::now_v7();
    let record = ApiKeyRecord {
        id: id.to_string(),
        tenant_id: auth.tenant_id,
        merchant_id: Some(merchant_id),
        name: name.to_string(),
        hash,
        fingerprint: api_key_fingerprint(&plaintext),
        scopes: vec!["merchant".into()],
        revoked: false,
    };
    openpay_application::ApiKeyRepository::insert(&state.store, record)
        .await
        .map_err(openpay_application::ApplicationError::from)
        .map_err(ApiError::from)?;
    Ok(Json(json!({
        "id": ApiKeyId::from_uuid(id).as_prefixed(),
        "name": name,
        "scopes": ["merchant"],
        "merchant_id": merchant_id.as_prefixed(),
        "secret": plaintext,
        "shown_once": true
    })))
}

pub async fn revoke_api_key(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(key_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    let id = parse_api_key_uuid(&key_id)?;
    let updated = state
        .store
        .revoke_api_key(auth.tenant_id, id)
        .await
        .map_err(openpay_application::ApplicationError::from)
        .map_err(ApiError::from)?;
    if !updated {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "not-found",
            "Not found",
            "api key not found or already revoked",
        ));
    }
    Ok(Json(json!({ "id": key_id, "revoked": true })))
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
                "event_types": e.event_types,
                "merchant_id": e.merchant_id.as_prefixed(),
                "signing_secret_kind": signing_secret_kind(&e.signing_secret_ref)
            }))
            .collect::<Vec<_>>()
    )))
}

#[derive(Debug, Deserialize)]
pub struct CreateWebhookBody {
    pub url: String,
    pub event_types: Option<Vec<String>>,
    pub merchant_id: Option<String>,
}

pub async fn create_webhook_endpoint(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Json(body): Json<CreateWebhookBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    let op = state.operator_snapshot();
    validate_webhook_url(&body.url, &op.webhook_url_allowlist)?;
    let merchant_id = if let Some(raw) = body.merchant_id {
        raw.parse::<MerchantId>().map_err(|e| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                e.to_string(),
            )
        })?
    } else {
        state
            .store
            .first_merchant_id(auth.tenant_id)
            .await
            .map_err(openpay_application::ApplicationError::from)
            .map_err(ApiError::from)?
    };
    let events = normalize_event_types(body.event_types);
    let plaintext = generate_webhook_secret();
    let signing_secret_ref = seal_connector_ref(&plaintext, &state.config.encryption_master_key)?;
    let now = OffsetDateTime::now_utc();
    let endpoint = WebhookEndpoint {
        id: WebhookEndpointId::new(),
        tenant_id: auth.tenant_id,
        merchant_id,
        url: body.url.clone(),
        event_types: events.clone(),
        signing_secret_ref,
        status: WebhookEndpointStatus::Active,
        failure_count: 0,
        created_at: now,
        updated_at: now,
    };
    let id = endpoint.id;
    state
        .store
        .insert_webhook_endpoint(&endpoint)
        .await
        .map_err(openpay_application::ApplicationError::from)
        .map_err(ApiError::from)?;
    Ok(Json(json!({
        "id": id.as_prefixed(),
        "url": body.url,
        "event_types": events,
        "status": "active",
        "secret": plaintext,
        "shown_once": true
    })))
}

#[derive(Debug, Deserialize)]
pub struct UpdateWebhookBody {
    pub url: Option<String>,
    pub event_types: Option<Vec<String>>,
    pub status: Option<String>,
}

pub async fn update_webhook_endpoint(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(endpoint_id): Path<String>,
    Json(body): Json<UpdateWebhookBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    let id: WebhookEndpointId = endpoint_id.parse().map_err(|e: openpay_domain::DomainError| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "validation",
            "Validation failed",
            e.to_string(),
        )
    })?;
    if let Some(url) = &body.url {
        let op = state.operator_snapshot();
        validate_webhook_url(url, &op.webhook_url_allowlist)?;
    }
    let status = match body.status.as_deref() {
        None => None,
        Some("active") => Some("active"),
        Some("disabled") => Some("disabled"),
        Some(other) => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                format!("unknown webhook status {other}"),
            ));
        }
    };
    let events = body
        .event_types
        .map(|raw| normalize_event_types(Some(raw)));
    let updated = state
        .store
        .update_webhook_endpoint(
            auth.tenant_id,
            id,
            body.url.as_deref(),
            events.as_deref(),
            status,
            None,
        )
        .await
        .map_err(openpay_application::ApplicationError::from)
        .map_err(ApiError::from)?;
    Ok(Json(json!({
        "id": updated.id.as_prefixed(),
        "url": updated.url,
        "status": format!("{:?}", updated.status).to_lowercase(),
        "event_types": updated.event_types
    })))
}

pub async fn rotate_webhook_secret(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(endpoint_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    let id: WebhookEndpointId = endpoint_id.parse().map_err(|e: openpay_domain::DomainError| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "validation",
            "Validation failed",
            e.to_string(),
        )
    })?;
    let plaintext = generate_webhook_secret();
    let signing_secret_ref = seal_connector_ref(&plaintext, &state.config.encryption_master_key)?;
    let updated = state
        .store
        .update_webhook_endpoint(
            auth.tenant_id,
            id,
            None,
            None,
            None,
            Some(&signing_secret_ref),
        )
        .await
        .map_err(openpay_application::ApplicationError::from)
        .map_err(ApiError::from)?;
    Ok(Json(json!({
        "id": updated.id.as_prefixed(),
        "secret": plaintext,
        "shown_once": true
    })))
}

#[derive(Debug, Deserialize)]
pub struct WebhookDeliveryQuery {
    pub payment_id: Option<String>,
}

pub async fn list_webhook_deliveries(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Query(query): Query<WebhookDeliveryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    let rows = if let Some(raw) = query.payment_id {
        let id: PaymentId = raw.parse().map_err(|e: openpay_domain::DomainError| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                e.to_string(),
            )
        })?;
        WebhookRepository::list_deliveries_for_payment(&state.store, auth.tenant_id, id)
            .await
            .map_err(openpay_application::ApplicationError::from)
            .map_err(ApiError::from)?
    } else {
        state
            .store
            .list_recent_deliveries(50)
            .await
            .map_err(openpay_application::ApplicationError::from)
            .map_err(ApiError::from)?
    };
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

#[derive(Debug, Deserialize)]
pub struct RoutingPatch {
    pub name: Option<String>,
    pub rules_json: Option<Value>,
    pub fallback_policy: Option<Value>,
    pub status: Option<String>,
}

pub async fn update_routing_policy(
    State(state): State<Arc<AppState>>,
    auth: AuthContext,
    Path(policy_id): Path<String>,
    Json(body): Json<RoutingPatch>,
) -> Result<Json<serde_json::Value>, ApiError> {
    require_admin(&auth)?;
    let id: RoutingPolicyId = policy_id.parse().map_err(|e: openpay_domain::DomainError| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "validation",
            "Validation failed",
            e.to_string(),
        )
    })?;
    if let Some(rules) = &body.rules_json {
        if !rules.is_object() {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                "rules_json must be a JSON object",
            ));
        }
    }
    if let Some(fallback) = &body.fallback_policy {
        if !fallback.is_object() {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                "fallback_policy must be a JSON object",
            ));
        }
    }
    let status = match body.status.as_deref() {
        None => None,
        Some("active") => Some("active"),
        Some("disabled") => Some("disabled"),
        Some(other) => {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                format!("unknown routing status {other}"),
            ));
        }
    };
    let updated = state
        .store
        .update_routing_policy(
            auth.tenant_id,
            id,
            body.name.as_deref(),
            body.rules_json.as_ref(),
            body.fallback_policy.as_ref(),
            status,
        )
        .await
        .map_err(openpay_application::ApplicationError::from)
        .map_err(ApiError::from)?;
    Ok(Json(json!({
        "id": updated.id.as_prefixed(),
        "name": updated.name,
        "status": format!("{:?}", updated.status).to_lowercase(),
        "rules_json": updated.rules_json,
        "fallback_policy": updated.fallback_policy
    })))
}

#[utoipa::path(
    post,
    path = "/v1/admin/payments/{payment_id}/reconcile",
    tag = "admin",
    params(("payment_id" = String, Path, description = "Prefixed payment id")),
    responses((status = 200, description = "Reconciled payment status"), (status = 403), (status = 404))
)]
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
                StatusCode::BAD_REQUEST,
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
                StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                e.to_string(),
            )
        })?;
    let manual = state.connectors.manual.as_ref().ok_or_else(|| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
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
            StatusCode::BAD_REQUEST,
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
                StatusCode::UNPROCESSABLE_ENTITY,
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

pub(crate) fn require_admin(auth: &AuthContext) -> Result<(), ApiError> {
    if auth.role == "admin" || auth.scopes.iter().any(|s| s == "admin") {
        Ok(())
    } else {
        Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Forbidden",
            "admin role required",
        ))
    }
}

pub(crate) fn require_sandbox(state: &AppState) -> Result<(), ApiError> {
    let status = sandbox_availability(state);
    if status
        .get("available")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        Ok(())
    } else {
        let detail = status
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("sandbox lab unavailable");
        Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "Sandbox lab unavailable",
            detail,
        ))
    }
}

fn settings_view(
    state: &AppState,
    effective: &OperatorSettings,
    stored: Option<&Value>,
) -> Value {
    json!({
        "app_name": state.config.app_name,
        "environment": state.config.environment,
        "edition": state.config.edition.as_str(),
        "self_hosted": state.config.self_hosted,
        "deployment": if state.config.self_hosted { "self-hosted" } else { "cloud" },
        "sandbox_lab": sandbox_availability(state),
        "operator": effective.to_json(),
        "operator_source": json!({
            "default_currency": source_of(stored, "default_currency"),
            "qr_ttl_seconds": source_of(stored, "qr_ttl_seconds"),
            "webhook_timeout_ms": source_of(stored, "webhook_timeout_ms"),
            "rate_limit_per_minute": source_of(stored, "rate_limit_per_minute"),
            "cors_allow_origins": source_of(stored, "cors_allow_origins"),
            "webhook_url_allowlist": source_of(stored, "webhook_url_allowlist"),
            "features": if stored.and_then(|v| v.get("features")).is_some() { "tenant" } else { "env" }
        }),
        "env_only": [
            env_only_row("DATABASE_URL", !state.config.database_url.is_empty(),
                "Solo .env / riavvio. Non si scrive dalla dashboard: esporrebbe le credenziali del database."),
            env_only_row("REDIS_URL", !state.config.redis_url.is_empty(),
                "Solo .env / riavvio. Il limiter e la sessione dipendono da Redis a livello di processo."),
            env_only_row("JWT_ACCESS_SECRET", !state.config.jwt_access_secret.is_empty(),
                "Solo .env / riavvio. Un secret JWT in un form web è un leak."),
            env_only_row("JWT_REFRESH_SECRET", !state.config.jwt_refresh_secret.is_empty(),
                "Solo .env / riavvio."),
            env_only_row("ENCRYPTION_MASTER_KEY", !state.config.encryption_master_key.is_empty(),
                "Solo .env / riavvio. Serve a cifrare i configuration_ref, non va mai nel browser."),
            env_only_row("WEBHOOK_SIGNING_SECRET", !state.config.webhook_signing_secret.is_empty(),
                "Fallback di firma se l'endpoint usa env:WEBHOOK_SIGNING_SECRET. I nuovi endpoint dal desk hanno un secret proprio (mostrato una volta).")
        ],
        "process_notes": {
            "cors_rate_limit": "CORS e rate limit salvati si applicano alle nuove richieste HTTP senza riavvio.",
            "qr_currency": "Valuta di default e TTL QR si applicano ai nuovi pagamenti del laboratorio.",
            "webhooks": "Timeout e allowlist hostname sono letti dal worker a ogni delivery.",
            "features": "Mock e open-banking stub si registrano all'avvio. Disattivare il mock qui spegne subito il laboratorio; attivarlo richiede FEATURE_CONNECTOR_MOCK=true e riavvio se il processo è partito senza mock.",
            "psp": "La dashboard non configura Stripe o altri PSP live."
        }
    })
}

fn source_of(stored: Option<&Value>, key: &str) -> &'static str {
    if stored.and_then(|v| v.get(key)).is_some() {
        "tenant"
    } else {
        "env"
    }
}

fn env_only_row(key: &str, configured: bool, hint: &str) -> Value {
    json!({
        "key": key,
        "configured": configured,
        "hint": hint
    })
}

fn apply_operator_patch(stored: &mut Value, body: &OperatorPatch) -> Result<(), ApiError> {
    let obj = stored.as_object_mut().ok_or_else(|| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "Internal error",
            "tenant settings is not an object",
        )
    })?;
    if let Some(v) = &body.default_currency {
        if v.trim().len() != 3 {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                "default_currency must be a 3-letter code",
            ));
        }
        obj.insert(
            "default_currency".into(),
            json!(v.trim().to_ascii_uppercase()),
        );
    }
    if let Some(v) = body.qr_ttl_seconds {
        obj.insert("qr_ttl_seconds".into(), json!(v.clamp(30, 3600)));
    }
    if let Some(v) = body.webhook_timeout_ms {
        obj.insert("webhook_timeout_ms".into(), json!(v.clamp(500, 60_000)));
    }
    if let Some(v) = body.rate_limit_per_minute {
        obj.insert("rate_limit_per_minute".into(), json!(v.clamp(1, 10_000)));
    }
    if let Some(v) = &body.cors_allow_origins {
        obj.insert("cors_allow_origins".into(), json!(v));
    }
    if let Some(v) = &body.webhook_url_allowlist {
        obj.insert("webhook_url_allowlist".into(), json!(v));
    }
    if let Some(features) = &body.features {
        let mut feat = obj
            .get("features")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if let Some(map) = feat.as_object_mut() {
            if let Some(v) = features.connector_mock {
                map.insert("connector_mock".into(), json!(v));
            }
            if let Some(v) = features.connector_open_banking {
                map.insert("connector_open_banking".into(), json!(v));
            }
            if let Some(v) = features.telemetry_opt_in {
                map.insert("telemetry_opt_in".into(), json!(v));
            }
        }
        obj.insert("features".into(), feat);
    }
    Ok(())
}

fn validate_connector_ref(value: &str) -> Result<(), ApiError> {
    if looks_like_live_psp(value) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "validation",
            "Validation failed",
            "live PSP credentials cannot be set from the dashboard",
        ));
    }
    if is_secret_ref(value) && value.len() > "secret://".len() {
        return Ok(());
    }
    if let Some(rest) = value.strip_prefix("env:") {
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Ok(());
        }
    }
    Err(ApiError::new(
        StatusCode::BAD_REQUEST,
        "validation",
        "Validation failed",
        "use secret://… or env:VAR_NAME; this UI does not configure live Stripe",
    ))
}

pub(crate) fn looks_like_live_psp(value: &str) -> bool {
    let v = value.to_ascii_lowercase();
    v.contains("sk_live")
        || v.contains("rk_live")
        || v.contains("sk_test")
        || (v.contains("stripe") && (v.contains("sk_") || v.contains("rk_")))
}

fn seal_connector_ref(plaintext: &str, master_key_raw: &str) -> Result<String, ApiError> {
    let key = decode_master_key(master_key_raw).map_err(|_| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "crypto",
            "Encryption unavailable",
            "ENCRYPTION_MASTER_KEY is not usable",
        )
    })?;
    seal_if_plaintext(&key, plaintext).map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "Internal error",
            "could not encrypt secret",
        )
    })
}

fn redact_configuration_ref(stored: &str, master_key_raw: &str) -> (&'static str, String) {
    if is_encrypted_envelope(stored) {
        if let Ok(key) = decode_master_key(master_key_raw) {
            if let Ok(plain) = open_secret_value(&key, stored) {
                if plain.starts_with("secret://") || plain.starts_with("env:") {
                    return ("encrypted_ref", plain);
                }
                return ("encrypted", "********".into());
            }
        }
        return ("encrypted", "enc:v1:********".into());
    }
    if is_secret_ref(stored) {
        return ("secret_ref", stored.to_string());
    }
    if stored.starts_with("env:") {
        return ("env_ref", stored.to_string());
    }
    ("opaque", "********".into())
}

fn signing_secret_kind(stored: &str) -> &'static str {
    if stored.starts_with("env:") {
        "env"
    } else if is_encrypted_envelope(stored) {
        "endpoint"
    } else {
        "unknown"
    }
}

fn validate_webhook_url(url: &str, allowlist: &[String]) -> Result<(), ApiError> {
    let uri: axum::http::Uri = url.parse().map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "validation",
            "Validation failed",
            "invalid webhook URL",
        )
    })?;
    let scheme = uri.scheme_str().unwrap_or("");
    if scheme != "http" && scheme != "https" {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "validation",
            "Validation failed",
            "webhook URL must be http or https",
        ));
    }
    let host = uri.host().ok_or_else(|| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "validation",
            "Validation failed",
            "webhook URL missing host",
        )
    })?;
    if allowlist.is_empty() {
        return Ok(());
    }
    let allowed = allowlist.iter().any(|entry| {
        host.eq_ignore_ascii_case(entry) || host.to_ascii_lowercase().ends_with(&format!(".{entry}"))
    });
    if allowed {
        Ok(())
    } else {
        Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "validation",
            "Validation failed",
            format!("host {host} is not in the webhook allowlist"),
        ))
    }
}

const DEFAULT_EVENTS: &[&str] = &[
    "payment.created",
    "payment.requires_action",
    "payment.processing",
    "payment.authorized",
    "payment.settled",
    "payment.failed",
    "payment.cancelled",
    "payment.expired",
    "payment.refunded",
];

fn normalize_event_types(raw: Option<Vec<String>>) -> Vec<String> {
    let items = raw.unwrap_or_else(|| DEFAULT_EVENTS.iter().map(|s| (*s).to_string()).collect());
    let mut out: Vec<String> = items
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    out.sort();
    out.dedup();
    if out.is_empty() {
        DEFAULT_EVENTS.iter().map(|s| (*s).to_string()).collect()
    } else {
        out
    }
}

fn parse_api_key_uuid(raw: &str) -> Result<Uuid, ApiError> {
    raw.parse::<ApiKeyId>()
        .map(|id| id.as_uuid())
        .or_else(|_| Uuid::parse_str(raw))
        .map_err(|_| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "validation",
                "Validation failed",
                "invalid api key id",
            )
        })
}

fn api_key_prefixed(id: &str) -> String {
    Uuid::parse_str(id)
        .map(|u| ApiKeyId::from_uuid(u).as_prefixed())
        .unwrap_or_else(|_| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::looks_like_live_psp;

    #[test]
    fn rejects_stripe_live_keys() {
        assert!(looks_like_live_psp("sk_live_abc"));
        assert!(looks_like_live_psp("rk_live_abc"));
        assert!(looks_like_live_psp("sk_test_abc"));
        assert!(!looks_like_live_psp("secret://connectors/mock-instant"));
        assert!(!looks_like_live_psp("env:CONNECTOR_SECRET"));
    }
}
