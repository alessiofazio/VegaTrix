use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;use axum::routing::get;
use axum::Router;
use clap::Parser;
use connector_mock_instant::MockInstantConnector;
use openpay_config::AppConfig;
use openpay_connectors::ConnectorRegistry;
use openpay_observability::init_tracing;
use openpay_persistence::{connect, migrate, PgStore};
use openpay_worker::{run_loop, WorkerRuntime};

#[derive(Parser, Debug)]
struct Args {}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = Args::parse();
    let config = AppConfig::load().context("load config")?;
    init_tracing(&config.log_level, !config.is_dev());
    if config.telemetry_opt_in {
        let _ = openpay_observability::init_metrics(&config.metrics_bind_addr);
    }

    let pool = connect(&config.database_url)
        .await
        .context("connect postgres")?;
    migrate(&pool).await.ok();
    let store = PgStore::new(pool);

    let mut registry = ConnectorRegistry::new();
    if config.features.connector_mock {
        registry.register(Arc::new(MockInstantConnector::new()));
    }

    let runtime = WorkerRuntime::new(store, config.clone(), registry).context("http client")?;

    let health_addr: SocketAddr = config.worker_bind_addr.parse().context("worker bind")?;
    tokio::spawn(async move {
        let app = Router::new().route("/healthz", get(|| async { "ok" }));
        if let Ok(listener) = tokio::net::TcpListener::bind(health_addr).await {
            tracing::info!(%health_addr, "worker health listening");
            let _ = axum::serve(listener, app).await;
        }
    });

    tracing::info!("openpay-worker started");
    run_loop(runtime).await;
    Ok(())
}
