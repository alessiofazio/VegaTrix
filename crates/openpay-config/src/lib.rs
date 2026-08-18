use serde::Deserialize;
use std::path::Path;
use thiserror::Error;
use url::Url;

use openpay_crypto::decode_master_key;
use openpay_domain::Plan;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("configuration error: {0}")]
    Load(String),
    #[error("invalid URL: {0}")]
    Url(String),
    #[error("missing required secret: {0}")]
    MissingSecret(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edition {
    Community,
    Cloud,
    Enterprise,
}

impl Edition {
    pub fn parse(raw: &str) -> Result<Self, ConfigError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "community" => Ok(Self::Community),
            "cloud" => Ok(Self::Cloud),
            "enterprise" => Ok(Self::Enterprise),
            other => Err(ConfigError::Load(format!("unknown edition: {other}"))),
        }
    }

    pub fn plan(self) -> Plan {
        match self {
            Self::Community => Plan::Community,
            Self::Cloud => Plan::Cloud,
            Self::Enterprise => Plan::Enterprise,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Community => "community",
            Self::Cloud => "cloud",
            Self::Enterprise => "enterprise",
        }
    }
}

#[derive(Debug, Clone)]
pub struct EditionCapabilities {
    pub advanced_routing: bool,
    pub analytics: bool,
    pub sso: bool,
    pub connector_open_banking: bool,
    pub white_label: bool,
}

impl EditionCapabilities {
    pub fn for_edition(edition: Edition, flags: &FeatureFlags) -> Self {
        match edition {
            Edition::Community => Self {
                advanced_routing: false,
                analytics: false,
                sso: false,
                connector_open_banking: flags.connector_open_banking,
                white_label: false,
            },
            Edition::Cloud => Self {
                advanced_routing: flags.advanced_routing,
                analytics: flags.analytics,
                sso: flags.sso,
                connector_open_banking: flags.connector_open_banking,
                white_label: false,
            },
            Edition::Enterprise => Self {
                advanced_routing: true,
                analytics: true,
                sso: true,
                connector_open_banking: flags.connector_open_banking,
                white_label: false,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct FeatureFlags {
    pub connector_mock: bool,
    pub connector_open_banking: bool,
    pub advanced_routing: bool,
    pub analytics: bool,
    pub sso: bool,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub app_name: String,
    pub environment: String,
    pub self_hosted: bool,
    pub edition: Edition,
    pub app_base_url: String,
    pub api_base_url: String,
    pub dashboard_base_url: String,
    pub wallet_base_url: String,
    pub metrics_bind_addr: String,
    pub database_url: String,
    pub redis_url: String,
    pub jwt_access_secret: String,
    pub jwt_refresh_secret: String,
    pub encryption_master_key: String,
    pub webhook_signing_secret: String,
    pub qr_signing_secret: String,
    pub default_currency: String,
    pub default_timezone: String,
    pub log_level: String,
    pub bind_addr: String,
    pub worker_bind_addr: String,
    pub webhook_timeout_ms: u64,
    pub webhook_max_attempts: u32,
    pub webhook_tolerance_secs: i64,
    pub qr_ttl_seconds: i64,
    pub rate_limit_per_minute: u64,
    pub telemetry_opt_in: bool,
    pub cors_allow_origins: Vec<String>,
    pub webhook_url_allowlist: Vec<String>,
    pub features: FeatureFlags,
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

fn env_or(key: &str, default: &str) -> String {
    env(key).unwrap_or_else(|| default.to_string())
}

fn env_bool(key: &str, default: bool) -> bool {
    match env(key) {
        Some(v) => matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        None => default,
    }
}

fn env_csv(key: &str) -> Vec<String> {
    env(key)
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let _ = dotenvy::dotenv();
        if Path::new("config/default.toml").exists() {
            let _cfg = config::Config::builder()
                .add_source(config::File::with_name("config/default"))
                .build();
            let _ = _cfg;
        }

        let environment = env_or("APP_ENV", &env_or("NODE_ENV", "development"));
        let production = environment.eq_ignore_ascii_case("production");
        let cfg = Self {
            app_name: env_or("APP_NAME", "OpenPay Protocol"),
            environment,
            self_hosted: env_bool("SELF_HOSTED", true),
            edition: Edition::parse(&env_or("EDITION", "community"))?,
            app_base_url: env_or("APP_BASE_URL", "http://localhost:3000"),
            api_base_url: env_or("API_BASE_URL", "http://localhost:8080"),
            dashboard_base_url: env_or("DASHBOARD_BASE_URL", "http://localhost:3001"),
            wallet_base_url: env_or("WALLET_BASE_URL", "http://localhost:3003"),
            metrics_bind_addr: env_or("METRICS_BIND_ADDR", "0.0.0.0:9090"),
            database_url: env_or(
                "DATABASE_URL",
                "postgresql://openpay:openpay@localhost:5432/openpay",
            ),
            redis_url: env_or("REDIS_URL", "redis://localhost:6379"),
            jwt_access_secret: env_or("JWT_ACCESS_SECRET", ""),
            jwt_refresh_secret: env_or("JWT_REFRESH_SECRET", ""),
            encryption_master_key: env_or("ENCRYPTION_MASTER_KEY", ""),
            webhook_signing_secret: env_or("WEBHOOK_SIGNING_SECRET", ""),
            qr_signing_secret: env_or("QR_SIGNING_SECRET", ""),
            default_currency: env_or("DEFAULT_CURRENCY", "EUR"),
            default_timezone: env_or("DEFAULT_TIMEZONE", "Europe/Rome"),
            log_level: env_or("LOG_LEVEL", "info"),
            bind_addr: env_or("BIND_ADDR", "0.0.0.0:8080"),
            worker_bind_addr: env_or("WORKER_BIND_ADDR", "0.0.0.0:8081"),
            webhook_timeout_ms: env_or("WEBHOOK_TIMEOUT_MS", "5000").parse().unwrap_or(5000),
            webhook_max_attempts: env_or("WEBHOOK_MAX_ATTEMPTS", "8").parse().unwrap_or(8),
            webhook_tolerance_secs: env_or("WEBHOOK_TOLERANCE_SECS", "300")
                .parse()
                .unwrap_or(300),
            qr_ttl_seconds: env_or("QR_TTL_SECONDS", "300").parse().unwrap_or(300),
            rate_limit_per_minute: env_or("RATE_LIMIT_PER_MINUTE", "120")
                .parse()
                .unwrap_or(120),
            telemetry_opt_in: env_bool("TELEMETRY_OPT_IN", false),
            cors_allow_origins: env_csv("CORS_ALLOW_ORIGINS"),
            webhook_url_allowlist: env_csv("WEBHOOK_URL_ALLOWLIST"),
            features: FeatureFlags {
                connector_mock: env_bool("FEATURE_CONNECTOR_MOCK", !production),
                connector_open_banking: env_bool("FEATURE_CONNECTOR_OPEN_BANKING", false),
                advanced_routing: env_bool("FEATURE_ADVANCED_ROUTING", false),
                analytics: env_bool("FEATURE_ANALYTICS", false),
                sso: env_bool("FEATURE_SSO", false),
            },
        };
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        Url::parse(&self.api_base_url).map_err(|e| ConfigError::Url(e.to_string()))?;
        if self.jwt_access_secret.len() < 32 {
            return Err(ConfigError::MissingSecret("JWT_ACCESS_SECRET"));
        }
        if self.jwt_refresh_secret.len() < 32 {
            return Err(ConfigError::MissingSecret("JWT_REFRESH_SECRET"));
        }
        if self.qr_signing_secret.len() < 16 {
            return Err(ConfigError::MissingSecret("QR_SIGNING_SECRET"));
        }
        if self.webhook_signing_secret.len() < 16 {
            return Err(ConfigError::MissingSecret("WEBHOOK_SIGNING_SECRET"));
        }
        if self.database_url.starts_with("sqlite") && self.is_production() {
            return Err(ConfigError::Load(
                "SQLite is not supported in production (sqlite-demo only)".into(),
            ));
        }
        if self.is_production() {
            self.validate_production()?;
        }
        Ok(())
    }

    pub fn validate_production(&self) -> Result<(), ConfigError> {
        for (name, value, min_len) in [
            ("JWT_ACCESS_SECRET", self.jwt_access_secret.as_str(), 32),
            ("JWT_REFRESH_SECRET", self.jwt_refresh_secret.as_str(), 32),
            ("QR_SIGNING_SECRET", self.qr_signing_secret.as_str(), 16),
            (
                "WEBHOOK_SIGNING_SECRET",
                self.webhook_signing_secret.as_str(),
                16,
            ),
            (
                "ENCRYPTION_MASTER_KEY",
                self.encryption_master_key.as_str(),
                32,
            ),
        ] {
            if looks_like_placeholder(value) || value.len() < min_len {
                return Err(ConfigError::MissingSecret(name));
            }
        }
        decode_master_key(&self.encryption_master_key)
            .map_err(|_| ConfigError::MissingSecret("ENCRYPTION_MASTER_KEY"))?;
        if self.features.connector_mock {
            return Err(ConfigError::Load(
                "FEATURE_CONNECTOR_MOCK must be false in production".into(),
            ));
        }
        if self.webhook_url_allowlist.is_empty() {
            return Err(ConfigError::Load(
                "WEBHOOK_URL_ALLOWLIST is required in production".into(),
            ));
        }
        require_https(&self.api_base_url, "API_BASE_URL")?;
        require_https(&self.app_base_url, "APP_BASE_URL")?;
        require_https(&self.dashboard_base_url, "DASHBOARD_BASE_URL")?;
        require_https(&self.wallet_base_url, "WALLET_BASE_URL")?;
        Ok(())
    }

    pub fn assert_seed_allowed(&self) -> Result<(), ConfigError> {
        if self.is_production() {
            return Err(ConfigError::Load(
                "demo seed is not allowed when APP_ENV=production".into(),
            ));
        }
        Ok(())
    }

    pub fn capabilities(&self) -> EditionCapabilities {
        EditionCapabilities::for_edition(self.edition, &self.features)
    }

    pub fn is_production(&self) -> bool {
        self.environment.eq_ignore_ascii_case("production")
    }

    pub fn is_dev(&self) -> bool {
        !self.is_production()
    }
}

pub fn looks_like_placeholder(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return true;
    }
    trimmed.to_ascii_lowercase().contains("replace_me")
}

fn require_https(url: &str, name: &str) -> Result<(), ConfigError> {
    let parsed = Url::parse(url).map_err(|e| ConfigError::Url(format!("{name}: {e}")))?;
    if parsed.scheme() != "https" {
        return Err(ConfigError::Load(format!(
            "{name} must use https:// in production"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(environment: &str) -> AppConfig {
        AppConfig {
            app_name: "OpenPay Protocol".into(),
            environment: environment.into(),
            self_hosted: true,
            edition: Edition::Community,
            app_base_url: "https://app.example.com".into(),
            api_base_url: "https://api.example.com".into(),
            dashboard_base_url: "https://dash.example.com".into(),
            wallet_base_url: "https://wallet.example.com".into(),
            metrics_bind_addr: "0.0.0.0:9090".into(),
            database_url: "postgresql://openpay:openpay@localhost:5432/openpay".into(),
            redis_url: "redis://localhost:6379".into(),
            jwt_access_secret: "prod_jwt_access_secret_value_32b!!".into(),
            jwt_refresh_secret: "prod_jwt_refresh_secret_value_32!!".into(),
            encryption_master_key: "openpay-master-key-32-bytes-ok!!".into(),
            webhook_signing_secret: "prod_webhook_signing_secret".into(),
            qr_signing_secret: "prod_qr_signing_secret!".into(),
            default_currency: "EUR".into(),
            default_timezone: "Europe/Rome".into(),
            log_level: "info".into(),
            bind_addr: "0.0.0.0:8080".into(),
            worker_bind_addr: "0.0.0.0:8081".into(),
            webhook_timeout_ms: 5000,
            webhook_max_attempts: 8,
            webhook_tolerance_secs: 300,
            qr_ttl_seconds: 300,
            rate_limit_per_minute: 120,
            telemetry_opt_in: false,
            cors_allow_origins: vec!["https://dash.example.com".into()],
            webhook_url_allowlist: vec!["merchant.example.com".into()],
            features: FeatureFlags {
                connector_mock: false,
                connector_open_banking: false,
                advanced_routing: false,
                analytics: false,
                sso: false,
            },
        }
    }

    #[test]
    fn development_accepts_placeholder_secrets() {
        let mut cfg = sample("development");
        cfg.jwt_access_secret = "replace_me_with_a_long_random_secret_32b".into();
        cfg.jwt_refresh_secret = "replace_me_with_a_different_long_secret".into();
        cfg.encryption_master_key = "replace_me_with_32_byte_base64_key!!".into();
        cfg.webhook_signing_secret = "replace_me_webhook_signing".into();
        cfg.qr_signing_secret = "replace_me_qr_signing_secret".into();
        cfg.api_base_url = "http://localhost:8080".into();
        cfg.app_base_url = "http://localhost:3000".into();
        cfg.dashboard_base_url = "http://localhost:3001".into();
        cfg.wallet_base_url = "http://localhost:3003".into();
        cfg.features.connector_mock = true;
        cfg.webhook_url_allowlist = vec!["demo-merchant".into()];
        cfg.validate()
            .expect("development should boot with demo secrets");
        cfg.assert_seed_allowed().expect("seed ok in development");
    }

    #[test]
    fn production_accepts_hardened_config() {
        sample("production")
            .validate()
            .expect("valid production config");
        sample("PRODUCTION")
            .validate()
            .expect("case-insensitive production");
    }

    #[test]
    fn production_refusals() {
        struct Case {
            name: &'static str,
            mutate: fn(&mut AppConfig),
        }
        let cases = [
            Case {
                name: "placeholder jwt",
                mutate: |c| c.jwt_access_secret = "replace_me_with_a_long_random_secret_32b".into(),
            },
            Case {
                name: "empty encryption key",
                mutate: |c| c.encryption_master_key.clear(),
            },
            Case {
                name: "short encryption key",
                mutate: |c| c.encryption_master_key = "too-short".into(),
            },
            Case {
                name: "malformed encryption key",
                mutate: |c| {
                    c.encryption_master_key = "not-a-valid-master-key-because-wrong-size!!!".into()
                },
            },
            Case {
                name: "mock connector",
                mutate: |c| c.features.connector_mock = true,
            },
            Case {
                name: "empty webhook allowlist",
                mutate: |c| c.webhook_url_allowlist.clear(),
            },
            Case {
                name: "http api url",
                mutate: |c| c.api_base_url = "http://api.example.com".into(),
            },
            Case {
                name: "http wallet url",
                mutate: |c| c.wallet_base_url = "http://wallet.example.com".into(),
            },
        ];
        for case in cases {
            let mut cfg = sample("production");
            (case.mutate)(&mut cfg);
            assert!(
                cfg.validate().is_err(),
                "expected production refusal for {}",
                case.name
            );
        }
    }

    #[test]
    fn production_rejects_seed() {
        assert!(sample("production").assert_seed_allowed().is_err());
        assert!(sample("development").assert_seed_allowed().is_ok());
    }

    #[test]
    fn placeholder_detection() {
        assert!(looks_like_placeholder(""));
        assert!(looks_like_placeholder("  replace_me_secret  "));
        assert!(!looks_like_placeholder(
            "prod_jwt_access_secret_value_32b!!"
        ));
    }
}
