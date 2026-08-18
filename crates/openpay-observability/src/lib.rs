use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_tracing(log_level: &str, json: bool) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level));
    let registry = tracing_subscriber::registry().with(filter);
    if json {
        registry
            .with(fmt::layer().json().flatten_event(true))
            .init();
    } else {
        registry.with(fmt::layer()).init();
    }
}

pub fn init_metrics(bind: &str) -> Result<(), String> {
    metrics_exporter_prometheus::PrometheusBuilder::new()
        .with_http_listener(
            bind.parse::<std::net::SocketAddr>()
                .map_err(|e| e.to_string())?,
        )
        .install()
        .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthStatus {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
}

pub fn live(service: &'static str) -> HealthStatus {
    HealthStatus {
        status: "ok",
        service,
        version: env!("CARGO_PKG_VERSION"),
    }
}
