use anyhow::Context;
use clap::{Parser, Subcommand};
use openpay_config::AppConfig;
use openpay_persistence::{connect, migrate, seed::seed_demo};

#[derive(Parser, Debug)]
#[command(name = "openpay")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Migrate,
    Seed,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let cli = Cli::parse();
    let config = AppConfig::load().context("load config")?;
    let pool = connect(&config.database_url).await?;
    match cli.command {
        Commands::Migrate => {
            migrate(&pool).await?;
            println!("migrations applied");
        }
        Commands::Seed => {
            config
                .assert_seed_allowed()
                .context("refusing seed in production")?;
            migrate(&pool).await?;
            let webhook = std::env::var("DEMO_MERCHANT_URL")
                .unwrap_or_else(|_| "http://demo-merchant:3002/webhooks/openpay".into());
            seed_demo(&pool, &webhook).await?;
            println!("demo seed applied");
            println!(
                "admin: {} / {}",
                openpay_persistence::seed::DEMO_ADMIN_EMAIL,
                openpay_persistence::seed::DEMO_ADMIN_PASSWORD
            );
            println!("api key: {}", openpay_persistence::seed::DEMO_API_KEY_PLAIN);
        }
    }
    Ok(())
}
