use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::middleware;
use axum::response::IntoResponse;
use axum::routing::{get, post};
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
use crate::middleware::{rate_limit, track_http_metrics};
use crate::state::AppState;

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::merchant::create_payment,
        crate::merchant::get_payment,
        health,
        ready
    ),
    components(schemas(CreatePaymentBody, PaymentCreatedResponse, PaymentView, ProblemDetails)),
    tags(
        (name = "merchant", description = "Merchant payment API"),
        (name = "admin", description = "Administrative API"),
        (name = "public", description = "QR / wallet public API")
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
    let cors = if state.config.cors_allow_origins.is_empty() {
        CorsLayer::new()
            .allow_origin(AllowOrigin::exact(HeaderValue::from_static(
                "http://localhost:3002",
            )))
            .allow_headers(tower_http::cors::Any)
            .allow_methods(tower_http::cors::Any)
    } else {
        let origins: Vec<HeaderValue> = state
            .config
            .cors_allow_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_headers(tower_http::cors::Any)
            .allow_methods(tower_http::cors::Any)
    };

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
        );

    let admin = Router::new()
        .route("/overview", get(crate::admin::overview))
        .route("/connectors", get(crate::admin::connectors))
        .route("/settings", get(crate::admin::settings))
        .route("/api-keys", get(crate::admin::list_api_keys))
        .route(
            "/webhook-endpoints",
            get(crate::admin::list_webhook_endpoints),
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
