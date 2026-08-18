use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

pub mod map;
pub mod sandbox;
pub mod seed;
pub mod settings;
pub mod store;

pub use settings::{ConnectorAdminRow, OperatorSettings};
pub use store::PgStore;

pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(20)
        .connect(database_url)
        .await
}

pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::migrate!("../../migrations").run(pool).await?;
    Ok(())
}

/// Best-effort: encrypt plaintext connector `secret://` refs when the master key is valid.
pub async fn maybe_encrypt_connector_refs(store: &PgStore, master_key_raw: &str) {
    match openpay_crypto::decode_master_key(master_key_raw) {
        Ok(key) => match store.encrypt_connector_secret_refs(&key).await {
            Ok(n) if n > 0 => tracing::info!(count = n, "encrypted connector configuration refs"),
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(error = %err, "could not encrypt connector configuration refs")
            }
        },
        Err(_) => tracing::debug!("ENCRYPTION_MASTER_KEY not usable; leaving connector refs as-is"),
    }
}
