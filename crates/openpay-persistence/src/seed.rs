use serde_json::json;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use openpay_crypto::{api_key_fingerprint, hash_secret};

pub const DEMO_TENANT: Uuid = Uuid::from_bytes([
    0x01, 0x90, 0x00, 0x00, 0x00, 0x00, 0x70, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
]);
pub const DEMO_MERCHANT: Uuid = Uuid::from_bytes([
    0x01, 0x90, 0x00, 0x00, 0x00, 0x00, 0x70, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
]);
pub const DEMO_USER: Uuid = Uuid::from_bytes([
    0x01, 0x90, 0x00, 0x00, 0x00, 0x00, 0x70, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03,
]);
pub const DEMO_CONNECTOR_MOCK: Uuid = Uuid::from_bytes([
    0x01, 0x90, 0x00, 0x00, 0x00, 0x00, 0x70, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04,
]);
pub const DEMO_CONNECTOR_MANUAL: Uuid = Uuid::from_bytes([
    0x01, 0x90, 0x00, 0x00, 0x00, 0x00, 0x70, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05,
]);
pub const DEMO_POLICY: Uuid = Uuid::from_bytes([
    0x01, 0x90, 0x00, 0x00, 0x00, 0x00, 0x70, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06,
]);
pub const DEMO_WEBHOOK: Uuid = Uuid::from_bytes([
    0x01, 0x90, 0x00, 0x00, 0x00, 0x00, 0x70, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07,
]);
pub const DEMO_API_KEY: Uuid = Uuid::from_bytes([
    0x01, 0x90, 0x00, 0x00, 0x00, 0x00, 0x70, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08,
]);

/// Demo merchant API key shown once in logs/docs. Hash stored in DB.
pub const DEMO_API_KEY_PLAIN: &str = "opk_demo_merchant_sandbox_not_for_production_use_only";
pub const DEMO_ADMIN_EMAIL: &str = "admin@demo.openpay.local";
pub const DEMO_ADMIN_PASSWORD: &str = "ChangeMeNow_OpenPayDemo1";

pub async fn seed_demo(pool: &PgPool, merchant_webhook_url: &str) -> Result<(), sqlx::Error> {
    let now = OffsetDateTime::now_utc();
    sqlx::query(
        "INSERT INTO tenants (id, name, slug, status, plan, created_at, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(DEMO_TENANT)
    .bind("Demo Tenant")
    .bind("demo")
    .bind("active")
    .bind("community")
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO merchants (id, tenant_id, legal_name, display_name, merchant_reference, country, currency_preferences, status, created_at, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(DEMO_MERCHANT)
    .bind(DEMO_TENANT)
    .bind("Caffè Aurora S.r.l.")
    .bind("Caffè Aurora")
    .bind("CAFFE-AURORA")
    .bind("IT")
    .bind(json!(["EUR"]))
    .bind("active")
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;

    let password_hash = hash_secret(DEMO_ADMIN_PASSWORD).expect("argon2");
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, role, created_at)
         VALUES ($1,$2,$3,$4,$5,$6)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(DEMO_USER)
    .bind(DEMO_TENANT)
    .bind(DEMO_ADMIN_EMAIL)
    .bind(&password_hash)
    .bind("admin")
    .bind(now)
    .execute(pool)
    .await?;

    let key_hash = hash_secret(DEMO_API_KEY_PLAIN).expect("argon2");
    let fingerprint = api_key_fingerprint(DEMO_API_KEY_PLAIN);
    sqlx::query(
        "INSERT INTO api_keys (id, tenant_id, merchant_id, name, hash, fingerprint, scopes, revoked, created_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,false,$8)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(DEMO_API_KEY)
    .bind(DEMO_TENANT)
    .bind(DEMO_MERCHANT)
    .bind("Demo merchant key")
    .bind(&key_hash)
    .bind(&fingerprint)
    .bind(json!(["merchant"]))
    .bind(now)
    .execute(pool)
    .await?;

    let mock_caps = json!({
        "methods": ["ACCOUNT_TO_ACCOUNT", "WALLET"],
        "refunds": true,
        "delayed_capture": false,
        "webhooks": true,
        "sandbox_only": true
    });
    sqlx::query(
        "INSERT INTO connectors (id, tenant_id, key, name, connector_type, status, configuration_ref, capabilities, priority, health_status, created_at, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(DEMO_CONNECTOR_MOCK)
    .bind(DEMO_TENANT)
    .bind("mock-instant")
    .bind("Mock Instant Rail")
    .bind("mock_instant")
    .bind("enabled")
    .bind("secret://connectors/mock-instant")
    .bind(&mock_caps)
    .bind(100)
    .bind("HEALTHY")
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;

    let manual_caps = json!({
        "methods": ["MANUAL"],
        "refunds": true,
        "delayed_capture": true,
        "webhooks": false,
        "sandbox_only": true
    });
    sqlx::query(
        "INSERT INTO connectors (id, tenant_id, key, name, connector_type, status, configuration_ref, capabilities, priority, health_status, created_at, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(DEMO_CONNECTOR_MANUAL)
    .bind(DEMO_TENANT)
    .bind("manual-test")
    .bind("Manual Test")
    .bind("manual_test")
    .bind("enabled")
    .bind("secret://connectors/manual-test")
    .bind(&manual_caps)
    .bind(10)
    .bind("HEALTHY")
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;

    let rules = json!({
        "rules": [{
            "when": { "currency": "EUR", "method_available": "ACCOUNT_TO_ACCOUNT", "connector_health": "HEALTHY" },
            "select": "mock-instant",
            "priority": 100
        }, {
            "when": { "method_available": "MANUAL" },
            "select": "manual-test",
            "priority": 10
        }]
    });
    let fallback = json!({
        "enabled": true,
        "max_attempts": 2,
        "allowed_failure_codes": ["CONNECTOR_UNAVAILABLE", "TIMEOUT"]
    });
    sqlx::query(
        "INSERT INTO routing_policies (id, tenant_id, name, status, rules_json, fallback_policy, created_at, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(DEMO_POLICY)
    .bind(DEMO_TENANT)
    .bind("EUR instant preferred")
    .bind("active")
    .bind(&rules)
    .bind(&fallback)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO webhook_endpoints (id, tenant_id, merchant_id, url, event_types, signing_secret_ref, status, failure_count, created_at, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,0,$8,$9)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(DEMO_WEBHOOK)
    .bind(DEMO_TENANT)
    .bind(DEMO_MERCHANT)
    .bind(merchant_webhook_url)
    .bind(json!(["payment.created","payment.requires_action","payment.processing","payment.authorized","payment.settled","payment.failed","payment.cancelled","payment.expired","payment.refunded"]))
    .bind("env:WEBHOOK_SIGNING_SECRET")
    .bind("active")
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(())
}
