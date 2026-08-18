use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use connector_manual_test::ManualTestConnector;
use connector_mock_instant::MockInstantConnector;
use connector_open_banking_stub::OpenBankingStubConnector;
use openpay_api::{AppState, connectors::ConnectorRuntime, router};
use openpay_config::AppConfig;
use openpay_connectors::{ConnectorRegistry, SandboxAttemptStore};
use openpay_observability::init_tracing;
use openpay_persistence::{
    PgStore, connect, maybe_encrypt_connector_refs, migrate, seed::seed_demo,
};

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "false")]
    seed: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = AppConfig::load().context("load config")?;
    if args.seed {
        config
            .assert_seed_allowed()
            .context("refusing --seed in production")?;
    }
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
            std::env::var("DEMO_MERCHANT_URL")
                .unwrap_or_else(|_| "http://demo-merchant:3002".into())
        );
        seed_demo(&pool, &webhook).await.context("seed")?;
        tracing::info!("demo seed applied");
    }

    let store = PgStore::new(pool);
    maybe_encrypt_connector_refs(&store, &config.encryption_master_key).await;

    let sandbox: Arc<dyn SandboxAttemptStore> = Arc::new(store.clone());
    let mut registry = ConnectorRegistry::new();
    let mut manual: Option<Arc<dyn openpay_connectors::ManualAttemptResolver>> = None;
    if config.features.connector_mock {
        registry.register(Arc::new(MockInstantConnector::with_store(sandbox.clone())));
        let m = Arc::new(ManualTestConnector::with_store(sandbox));
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
                if config.is_production() {
                    tracing::error!(error = %err, "redis unavailable; merchant rate limit will fail closed");
                } else {
                    tracing::warn!(error = %err, "redis unavailable, public rate limiting fail-open");
                }
                None
            }
        },
        Err(err) => {
            tracing::warn!(error = %err, "redis client error");
            None
        }
    };

    let operator = openpay_api::state::load_operator_settings(&store, &config).await;
    let state = AppState::new(config.clone(), store, runtime, redis, operator);
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
