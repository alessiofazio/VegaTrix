use axum::body::Body;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use redis::AsyncCommands;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::warn;

use crate::state::AppState;

/// Token-bucket style limiter keyed by client IP (public routes) using Redis when available.
pub async fn rate_limit(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let limit = state.config.rate_limit_per_minute.max(1);
    let key = format!("rl:{}", addr.ip());

    if let Some(redis) = &state.redis {
        let mut conn = redis.clone();
        let count: i64 = conn.incr(&key, 1).await.unwrap_or(1);
        if count == 1 {
            let _: Result<(), _> = conn.expire(&key, 60).await;
        }
        if count as u64 > limit {
            metrics::counter!("openpay_rate_limit_rejected").increment(1);
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
    }

    Ok(next.run(request).await)
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
