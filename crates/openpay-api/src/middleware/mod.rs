use axum::body::Body;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use redis::AsyncCommands;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::warn;

use openpay_crypto::{api_key_fingerprint, sha256_hex};

use crate::auth::tenant_id_from_access_token;
use crate::state::AppState;

enum LimitDecision {
    Allow,
    Reject,
    BackendUnavailable,
}

/// Token-bucket limiter for public (wallet/QR) routes, keyed by client IP.
/// Redis down: fail-open so demo authorize still works.
pub async fn rate_limit(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let key = format!("rl:ip:{}", addr.ip());
    match enforce(&state, &key).await {
        LimitDecision::Allow | LimitDecision::BackendUnavailable => next.run(request).await,
        LimitDecision::Reject => too_many_requests(),
    }
}

/// Merchant `/v1` limiter keyed by API key fingerprint or JWT tenant.
/// Production + Redis unavailable: fail-closed (503). Development: fail-open.
pub async fn rate_limit_authenticated(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let key = merchant_limit_key(request.headers(), addr, &state.config.jwt_access_secret);
    match enforce(&state, &key).await {
        LimitDecision::Allow => next.run(request).await,
        LimitDecision::Reject => too_many_requests(),
        LimitDecision::BackendUnavailable if state.config.is_production() => limiter_unavailable(),
        LimitDecision::BackendUnavailable => next.run(request).await,
    }
}

fn merchant_limit_key(headers: &HeaderMap, addr: SocketAddr, jwt_secret: &str) -> String {
    let Some(header) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
        return format!("rl:ip:{}", addr.ip());
    };
    let Some(token) = header.strip_prefix("Bearer ") else {
        return format!("rl:ip:{}", addr.ip());
    };
    if token.starts_with("opk_") {
        return format!("rl:key:{}", api_key_fingerprint(token));
    }
    if let Some(tenant) = tenant_id_from_access_token(jwt_secret, token) {
        return format!("rl:tenant:{tenant}");
    }
    format!("rl:token:{}", sha256_hex(token.as_bytes()))
}

async fn enforce(state: &AppState, key: &str) -> LimitDecision {
    let limit = state.operator_snapshot().rate_limit_per_minute.max(1);
    let Some(redis) = &state.redis else {
        return LimitDecision::BackendUnavailable;
    };
    let mut conn = redis.clone();
    match conn.incr::<_, _, i64>(key, 1).await {
        Ok(count) => {
            if count == 1 {
                let _: Result<(), _> = conn.expire(key, 60).await;
            }
            if count as u64 > limit {
                metrics::counter!("openpay_rate_limit_rejected").increment(1);
                LimitDecision::Reject
            } else {
                LimitDecision::Allow
            }
        }
        Err(err) => {
            warn!(error = %err, "rate-limit redis error");
            LimitDecision::BackendUnavailable
        }
    }
}

fn too_many_requests() -> Response {
    let mut response = (StatusCode::TOO_MANY_REQUESTS, Body::empty()).into_response();
    response.headers_mut().insert(
        axum::http::header::RETRY_AFTER,
        HeaderValue::from_static("60"),
    );
    response
}

fn limiter_unavailable() -> Response {
    let mut response =
        (StatusCode::SERVICE_UNAVAILABLE, "rate limiter unavailable").into_response();
    response.headers_mut().insert(
        axum::http::header::RETRY_AFTER,
        HeaderValue::from_static("5"),
    );
    response
}

pub async fn track_http_metrics(request: Request<Body>, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let started = std::time::Instant::now();
    let response = next.run(request).await;
    let status = response.status().as_u16();
    let path_label = path.clone();
    metrics::counter!("openpay_http_requests_total", "method" => method.to_string(), "path" => path_label, "status" => status.to_string()).increment(1);
    metrics::histogram!("openpay_http_request_duration_seconds", "path" => path)
        .record(started.elapsed().as_secs_f64());
    if status >= 500 {
        warn!(%status, "server error");
    }
    response
}

#[cfg(test)]
mod tests {
    use super::merchant_limit_key;
    use axum::http::{HeaderMap, HeaderValue};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[test]
    fn api_key_uses_fingerprint_bucket() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer opk_abc"),
        );
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 80);
        let key = merchant_limit_key(&headers, addr, "not-used");
        assert!(key.starts_with("rl:key:"));
        assert_ne!(key, "rl:ip:1.2.3.4");
    }
}
