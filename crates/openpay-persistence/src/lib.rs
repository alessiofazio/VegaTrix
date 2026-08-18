use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub mod map;
pub mod seed;
pub mod store;

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
