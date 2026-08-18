use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;
use clap::Parser;
use connector_manual_test::ManualTestConnector;
use connector_mock_instant::MockInstantConnector;
use openpay_config::AppConfig;
use openpay_connectors::{ConnectorRegistry, SandboxAttemptStore};
use openpay_observability::init_tracing;
use openpay_persistence::{PgStore, connect, maybe_encrypt_connector_refs, migrate};
use openpay_worker::{WorkerRuntime, run_loop};
use sqlx::PgPool;

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
    let store = PgStore::new(pool.clone());
    maybe_encrypt_connector_refs(&store, &config.encryption_master_key).await;

    let sandbox: Arc<dyn SandboxAttemptStore> = Arc::new(store.clone());
    let mut registry = ConnectorRegistry::new();
    if config.features.connector_mock {
        registry.register(Arc::new(MockInstantConnector::with_store(sandbox.clone())));
        registry.register(Arc::new(ManualTestConnector::with_store(sandbox)));
    }

    let runtime = WorkerRuntime::new(store, config.clone(), registry).context("http client")?;

    let health_addr: SocketAddr = config.worker_bind_addr.parse().context("worker bind")?;
    tokio::spawn(async move {
        let app = worker_probe_router(pool);
        if let Ok(listener) = tokio::net::TcpListener::bind(health_addr).await {
            tracing::info!(%health_addr, "worker health listening");
            let _ = axum::serve(listener, app).await;
        }
    });

    tracing::info!("openpay-worker started");
    run_loop(runtime).await;
    Ok(())
}

/// `/healthz` = process up (liveness). `/readyz` = Postgres reachable (readiness).
fn worker_probe_router(pool: PgPool) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route(
            "/readyz",
            get(move || {
                let pool = pool.clone();
                async move {
                    match sqlx::query("SELECT 1").execute(&pool).await {
                        Ok(_) => (StatusCode::OK, "ok"),
                        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
                    }
                }
            }),
        )
}
