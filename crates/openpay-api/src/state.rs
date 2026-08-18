use std::sync::{Arc, RwLock};

use openpay_application::{PaymentService, SystemClock};
use openpay_config::AppConfig;
use openpay_persistence::{OperatorSettings, PgStore};

use crate::connectors::ConnectorRuntime;

pub type Payments =
    PaymentService<PgStore, PgStore, PgStore, PgStore, PgStore, PgStore, PgStore, SystemClock>;

pub struct AppState {
    pub config: AppConfig,
    pub store: PgStore,
    pub payments: Payments,
    pub connectors: ConnectorRuntime,
    pub redis: Option<redis::aio::ConnectionManager>,
    pub operator: Arc<RwLock<OperatorSettings>>,
}

impl AppState {
    pub fn new(
        config: AppConfig,
        store: PgStore,
        connectors: ConnectorRuntime,
        redis: Option<redis::aio::ConnectionManager>,
        operator: OperatorSettings,
    ) -> Arc<Self> {
        let payments = PaymentService {
            payments: store.clone(),
            merchants: store.clone(),
            audit: store.clone(),
            outbox: store.clone(),
            routing: store.clone(),
            connectors: store.clone(),
            nonces: store.clone(),
            clock: SystemClock,
            qr_secret: config.qr_signing_secret.as_bytes().to_vec(),
            api_base_url: config.api_base_url.clone(),
            wallet_base_url: config.wallet_base_url.clone(),
            qr_ttl_seconds: operator.qr_ttl_seconds,
        };
        Arc::new(Self {
            config,
            store,
            payments,
            connectors,
            redis,
            operator: Arc::new(RwLock::new(operator)),
        })
    }

    pub fn operator_snapshot(&self) -> OperatorSettings {
        self.operator
            .read()
            .map(|g| g.clone())
            .unwrap_or_else(|_| operator_from_config(&self.config))
    }

    pub fn replace_operator(&self, next: OperatorSettings) {
        if let Ok(mut g) = self.operator.write() {
            *g = next;
        }
    }
}

pub fn operator_from_config(config: &AppConfig) -> OperatorSettings {
    OperatorSettings {
        default_currency: config.default_currency.clone(),
        qr_ttl_seconds: config.qr_ttl_seconds,
        webhook_timeout_ms: config.webhook_timeout_ms,
        rate_limit_per_minute: config.rate_limit_per_minute,
        cors_allow_origins: config.cors_allow_origins.clone(),
        webhook_url_allowlist: config.webhook_url_allowlist.clone(),
        feature_connector_mock: config.features.connector_mock,
        feature_connector_open_banking: config.features.connector_open_banking,
        telemetry_opt_in: config.telemetry_opt_in,
    }
}

pub async fn load_operator_settings(store: &PgStore, config: &AppConfig) -> OperatorSettings {
    let mut settings = operator_from_config(config);
    match store.load_latest_tenant_settings_json().await {
        Ok(Some(json)) => settings.overlay_json(&json),
        Ok(None) => {}
        Err(err) => tracing::warn!(error = %err, "could not load tenant operator settings"),
    }
    settings
}
