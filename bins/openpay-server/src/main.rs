use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use connector_manual_test::ManualTestConnector;
use connector_mock_instant::MockInstantConnector;
use connector_open_banking_stub::OpenBankingStubConnector;
use openpay_api::{connectors::ConnectorRuntime, router, AppState};
use openpay_config::AppConfig;
use openpay_connectors::ConnectorRegistry;
use openpay_observability::init_tracing;
use openpay_persistence::{connect, migrate, seed::seed_demo, PgStore};

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "false")]
    seed: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = AppConfig::load().context("load config")?;
    init_tracing(&config.log_level, !config.is_dev());
    if config.telemetry_opt_in {
        let _ = openpay_observability::init_metrics(&config.metrics_bind_addr);
    }

    let pool = connect(&config.database_url)
        .await
        .context("connect postgres")?;
    migrate(&pool).await.context("migrate")?;
    if args.seed || config.is_dev() {
        let webhook = format!(
            "{}/webhooks/openpay",
            std::env::var("DEMO_MERCHANT_URL").unwrap_or_else(|_| "http://demo-merchant:3002".into())
        );
        seed_demo(&pool, &webhook).await.context("seed")?;
        tracing::info!("demo seed applied");
    }

    let store = PgStore::new(pool);
    let mut registry = ConnectorRegistry::new();
    let mut manual: Option<Arc<dyn openpay_connectors::ManualAttemptResolver>> = None;
    if config.features.connector_mock {
        registry.register(Arc::new(MockInstantConnector::new()));
        let m = Arc::new(ManualTestConnector::new());
        registry.register(m.clone());
        manual = Some(m);
    }
    if config.capabilities().connector_open_banking {
        registry.register(Arc::new(OpenBankingStubConnector));
    }

    let runtime = {
        let mut rt = ConnectorRuntime::new(registry);
        if let Some(m) = manual {
            rt = rt.with_manual(m);
        }
        rt
    };

    let redis = match redis::Client::open(config.redis_url.clone()) {
        Ok(client) => match redis::aio::ConnectionManager::new(client).await {
            Ok(mgr) => Some(mgr),
            Err(err) => {
                tracing::warn!(error = %err, "redis unavailable, rate limiting disabled");
                None
            }
        },
        Err(err) => {
            tracing::warn!(error = %err, "redis client error");
            None
        }
    };

    let state = AppState::new(config.clone(), store, runtime, redis);
    let app = router(state);
    let addr: SocketAddr = config.bind_addr.parse().context("bind addr")?;
    tracing::info!(%addr, "openpay-server listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
