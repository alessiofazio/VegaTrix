use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::middleware;
use axum::response::IntoResponse;
use axum::http::request::Parts;
use axum::routing::{get, patch, post};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::auth::{login, refresh};
use crate::dto::{CreatePaymentBody, PaymentCreatedResponse, PaymentView};
use crate::error::ProblemDetails;
use crate::middleware::{rate_limit, rate_limit_authenticated, track_http_metrics};
use crate::public::AuthorizeBody;
use crate::state::AppState;

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::merchant::create_payment,
        crate::merchant::get_payment,
        crate::auth::login,
        crate::auth::refresh,
        crate::public::public_authorize,
        crate::admin::reconcile_payment_admin,
        health,
        ready
    ),
    components(schemas(
        CreatePaymentBody,
        PaymentCreatedResponse,
        PaymentView,
        ProblemDetails,
        crate::auth::LoginRequest,
        crate::auth::RefreshRequest,
        crate::auth::TokenResponse,
        AuthorizeBody
    )),
    tags(
        (name = "merchant", description = "Merchant payment API"),
        (name = "admin", description = "Administrative API"),
        (name = "public", description = "QR / wallet public API"),
        (name = "auth", description = "JWT login and refresh")
    ),
    info(title = "OpenPay Protocol API", version = "0.1.0")
)]
pub struct ApiDoc;

#[utoipa::path(get, path = "/healthz", responses((status = 200)))]
async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "ok",
        "service": "openpay-server",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

#[utoipa::path(get, path = "/readyz", responses((status = 200), (status = 503)))]
async fn ready(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> impl IntoResponse {
    let db_ok = sqlx::query("SELECT 1")
        .execute(&state.store.pool)
        .await
        .is_ok();
    let body = axum::Json(serde_json::json!({
        "status": if db_ok { "ok" } else { "unavailable" },
        "database": db_ok
    }));
    if db_ok {
        (StatusCode::OK, body).into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, body).into_response()
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    let operator = state.operator.clone();
    let env_origins = state.config.cors_allow_origins.clone();
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(
            move |origin: &HeaderValue, _parts: &Parts| {
                let Ok(origin_str) = origin.to_str() else {
                    return false;
                };
                let locked = operator.read().ok();
                let list = locked
                    .as_ref()
                    .map(|s| s.cors_allow_origins.as_slice())
                    .unwrap_or(env_origins.as_slice());
                if list.is_empty() {
                    return origin_str == "http://localhost:3001"
                        || origin_str == "http://localhost:3002"
                        || origin_str == "http://localhost:3003";
                }
                list.iter().any(|allowed| allowed == origin_str)
            },
        ))
        .allow_headers(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any);

    let x_request_id = HeaderName::from_static("x-request-id");

    let merchant = Router::new()
        .route(
            "/payment-requests",
            post(crate::merchant::create_payment).get(crate::merchant::list_payments),
        )
        .route(
            "/payment-requests/{payment_id}",
            get(crate::merchant::get_payment),
        )
        .route(
            "/payment-requests/{payment_id}/cancel",
            post(crate::merchant::cancel_payment),
        )
        .route(
            "/payment-requests/{payment_id}/refunds",
            post(crate::merchant::refund_payment),
        )
        .route(
            "/payment-requests/{payment_id}/attempts",
            get(crate::merchant::list_attempts),
        )
        .route(
            "/payment-requests/{payment_id}/events",
            get(crate::merchant::list_events),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit_authenticated,
        ));

    let admin = Router::new()
        .route("/overview", get(crate::admin::overview))
        .route("/connectors", get(crate::admin::connectors))
        .route(
            "/connectors/{key}",
            patch(crate::admin::update_connector),
        )
        .route(
            "/settings",
            get(crate::admin::settings).patch(crate::admin::update_settings),
        )
        .route(
            "/api-keys",
            get(crate::admin::list_api_keys).post(crate::admin::create_api_key),
        )
        .route(
            "/api-keys/{key_id}/revoke",
            post(crate::admin::revoke_api_key),
        )
        .route(
            "/webhook-endpoints",
            get(crate::admin::list_webhook_endpoints).post(crate::admin::create_webhook_endpoint),
        )
        .route(
            "/webhook-endpoints/{endpoint_id}",
            patch(crate::admin::update_webhook_endpoint),
        )
        .route(
            "/webhook-endpoints/{endpoint_id}/rotate-secret",
            post(crate::admin::rotate_webhook_secret),
        )
        .route(
            "/webhook-deliveries",
            get(crate::admin::list_webhook_deliveries),
        )
        .route(
            "/routing-policies",
            get(crate::admin::list_routing_policies),
        )
        .route(
            "/routing-policies/{policy_id}",
            patch(crate::admin::update_routing_policy),
        )
        .route("/sandbox", get(crate::sandbox::sandbox_status))
        .route(
            "/sandbox/payments",
            post(crate::sandbox::create_sandbox_payment),
        )
        .route(
            "/sandbox/payments/{payment_id}/authorize",
            post(crate::sandbox::sandbox_authorize),
        )
        .route(
            "/sandbox/payments/{payment_id}/duplicate",
            post(crate::sandbox::sandbox_duplicate),
        )
        .route(
            "/payments/{payment_id}/reconcile",
            post(crate::admin::reconcile_payment_admin),
        )
        .route(
            "/attempts/{attempt_id}/resolve",
            post(crate::admin::resolve_manual_attempt_admin),
        );

    let public = Router::new()
        .route("/payments/{payment_id}", get(crate::public::public_get))
        .route(
            "/payments/{payment_id}/authorize",
            post(crate::public::public_authorize),
        )
        .route(
            "/payments/{payment_id}/simulate-duplicate",
            post(crate::public::simulate_duplicate_callback),
        )
        .route("/payments/{payment_id}/qr.svg", get(crate::public::qr_png))
        .layer(middleware::from_fn_with_state(state.clone(), rate_limit));

    let api = Router::new()
        .nest("/v1", merchant)
        .nest("/v1/admin", admin)
        .nest("/v1/public", public)
        .route("/v1/auth/login", post(login))
        .route("/v1/auth/refresh", post(refresh));

    let swagger = SwaggerUi::new("/docs").url("/docs/openapi.json", ApiDoc::openapi());

    Router::new()
        .route("/healthz", get(health))
        .route("/readyz", get(ready))
        .merge(api)
        .merge(swagger)
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .layer(middleware::from_fn(track_http_metrics))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(SetRequestIdLayer::new(
            x_request_id.clone(),
            MakeRequestUuid,
        ))
        .layer(PropagateRequestIdLayer::new(x_request_id))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("no-referrer"),
        ))
        .with_state(state)
}
