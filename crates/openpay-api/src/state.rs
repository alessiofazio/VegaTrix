use std::sync::Arc;

use openpay_application::{PaymentService, SystemClock};
use openpay_config::AppConfig;
use openpay_persistence::PgStore;

use crate::connectors::ConnectorRuntime;

pub type Payments =
    PaymentService<PgStore, PgStore, PgStore, PgStore, PgStore, PgStore, PgStore, SystemClock>;

pub struct AppState {
    pub config: AppConfig,
    pub store: PgStore,
    pub payments: Payments,
    pub connectors: ConnectorRuntime,
    pub redis: Option<redis::aio::ConnectionManager>,
}

impl AppState {
    pub fn new(
        config: AppConfig,
        store: PgStore,
        connectors: ConnectorRuntime,
        redis: Option<redis::aio::ConnectionManager>,
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
            qr_ttl_seconds: config.qr_ttl_seconds,
        };
        Arc::new(Self {
            config,
            store,
            payments,
            connectors,
            redis,
        })
    }
}
