-- OpenPay Protocol v1 schema (PostgreSQL).
-- SQLite is not a production target.

CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE tenants (
    id              UUID PRIMARY KEY,
    name            TEXT NOT NULL,
    slug            TEXT NOT NULL UNIQUE,
    status          TEXT NOT NULL,
    plan            TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL,
    updated_at      TIMESTAMPTZ NOT NULL
);

CREATE TABLE merchants (
    id                    UUID PRIMARY KEY,
    tenant_id             UUID NOT NULL REFERENCES tenants (id),
    legal_name            TEXT NOT NULL,
    display_name          TEXT NOT NULL,
    merchant_reference    TEXT NOT NULL,
    country               TEXT NOT NULL,
    currency_preferences  JSONB NOT NULL DEFAULT '["EUR"]',
    status                TEXT NOT NULL,
    created_at            TIMESTAMPTZ NOT NULL,
    updated_at            TIMESTAMPTZ NOT NULL,
    UNIQUE (tenant_id, merchant_reference)
);

CREATE INDEX idx_merchants_tenant ON merchants (tenant_id);

CREATE TABLE users (
    id              UUID PRIMARY KEY,
    tenant_id       UUID NOT NULL REFERENCES tenants (id),
    email           TEXT NOT NULL UNIQUE,
    password_hash   TEXT NOT NULL,
    role            TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL
);

CREATE TABLE api_keys (
    id              UUID PRIMARY KEY,
    tenant_id       UUID NOT NULL REFERENCES tenants (id),
    merchant_id     UUID REFERENCES merchants (id),
    name            TEXT NOT NULL,
    hash            TEXT NOT NULL,
    fingerprint     TEXT NOT NULL UNIQUE,
    scopes          JSONB NOT NULL DEFAULT '[]',
    revoked         BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL
);

CREATE TABLE connectors (
    id                  UUID PRIMARY KEY,
    tenant_id           UUID REFERENCES tenants (id),
    key                 TEXT NOT NULL,
    name                TEXT NOT NULL,
    connector_type      TEXT NOT NULL,
    status              TEXT NOT NULL,
    configuration_ref   TEXT NOT NULL,
    capabilities        JSONB NOT NULL,
    priority            INT NOT NULL DEFAULT 0,
    health_status       TEXT NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL,
    updated_at          TIMESTAMPTZ NOT NULL,
    UNIQUE (tenant_id, key)
);

CREATE TABLE routing_policies (
    id                  UUID PRIMARY KEY,
    tenant_id           UUID NOT NULL REFERENCES tenants (id),
    name                TEXT NOT NULL,
    status              TEXT NOT NULL,
    rules_json          JSONB NOT NULL,
    fallback_policy     JSONB NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL,
    updated_at          TIMESTAMPTZ NOT NULL
);

CREATE TABLE payment_requests (
    id                  UUID PRIMARY KEY,
    tenant_id           UUID NOT NULL REFERENCES tenants (id),
    merchant_id         UUID NOT NULL REFERENCES merchants (id),
    merchant_order_id   TEXT NOT NULL,
    amount_minor        BIGINT NOT NULL CHECK (amount_minor > 0),
    currency            CHAR(3) NOT NULL,
    status              TEXT NOT NULL,
    allowed_methods     JSONB NOT NULL,
    description         TEXT,
    expires_at          TIMESTAMPTZ NOT NULL,
    return_url          TEXT,
    metadata            JSONB NOT NULL DEFAULT '{}',
    idempotency_key     TEXT NOT NULL,
    routing_policy_id   UUID,
    version             INT NOT NULL DEFAULT 1,
    created_at          TIMESTAMPTZ NOT NULL,
    updated_at          TIMESTAMPTZ NOT NULL
);

CREATE UNIQUE INDEX idx_payment_idempotency
    ON payment_requests (tenant_id, idempotency_key);

CREATE INDEX idx_payment_tenant_status_created
    ON payment_requests (tenant_id, status, created_at DESC);

CREATE INDEX idx_payment_merchant ON payment_requests (tenant_id, merchant_id);

CREATE TABLE idempotency_keys (
    tenant_id               UUID NOT NULL,
    idempotency_key         TEXT NOT NULL,
    request_fingerprint     TEXT NOT NULL,
    payment_id              UUID NOT NULL REFERENCES payment_requests (id),
    created_at              TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, idempotency_key, request_fingerprint)
);

CREATE TABLE payment_attempts (
    id                    UUID PRIMARY KEY,
    tenant_id             UUID NOT NULL REFERENCES tenants (id),
    payment_request_id    UUID NOT NULL REFERENCES payment_requests (id),
    connector_id          UUID NOT NULL,
    connector_key         TEXT NOT NULL,
    rail_type             TEXT NOT NULL,
    provider_reference    TEXT,
    status                TEXT NOT NULL,
    failure_code          TEXT,
    failure_message_safe  TEXT,
    amount_minor          BIGINT NOT NULL,
    currency              CHAR(3) NOT NULL,
    requested_at          TIMESTAMPTZ NOT NULL,
    authorized_at         TIMESTAMPTZ,
    settled_at            TIMESTAMPTZ,
    created_at            TIMESTAMPTZ NOT NULL,
    updated_at            TIMESTAMPTZ NOT NULL
);

CREATE UNIQUE INDEX idx_attempt_provider_ref
    ON payment_attempts (connector_key, provider_reference)
    WHERE provider_reference IS NOT NULL;

CREATE INDEX idx_attempt_payment ON payment_attempts (tenant_id, payment_request_id);

CREATE TABLE webhook_endpoints (
    id                    UUID PRIMARY KEY,
    tenant_id             UUID NOT NULL REFERENCES tenants (id),
    merchant_id           UUID NOT NULL REFERENCES merchants (id),
    url                   TEXT NOT NULL,
    event_types           JSONB NOT NULL,
    signing_secret_ref    TEXT NOT NULL,
    status                TEXT NOT NULL,
    failure_count         INT NOT NULL DEFAULT 0,
    created_at            TIMESTAMPTZ NOT NULL,
    updated_at            TIMESTAMPTZ NOT NULL
);

CREATE TABLE webhook_deliveries (
    id                    UUID PRIMARY KEY,
    webhook_endpoint_id   UUID NOT NULL REFERENCES webhook_endpoints (id),
    event_id              TEXT NOT NULL,
    payload_version       TEXT NOT NULL,
    payload               JSONB NOT NULL,
    status                TEXT NOT NULL,
    attempt_count         INT NOT NULL DEFAULT 0,
    next_retry_at         TIMESTAMPTZ,
    response_code         INT,
    last_error_safe       TEXT,
    created_at            TIMESTAMPTZ NOT NULL,
    updated_at            TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_deliveries_pending
    ON webhook_deliveries (status, next_retry_at);

CREATE TABLE audit_events (
    id                    UUID PRIMARY KEY,
    tenant_id             UUID NOT NULL REFERENCES tenants (id),
    actor_type            TEXT NOT NULL,
    actor_id              TEXT NOT NULL,
    event_type            TEXT NOT NULL,
    resource_type         TEXT NOT NULL,
    resource_id           TEXT NOT NULL,
    request_id            TEXT,
    ip_hash               TEXT,
    metadata_redacted     JSONB NOT NULL DEFAULT '{}',
    occurred_at           TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_audit_resource ON audit_events (tenant_id, resource_type, resource_id, occurred_at DESC);

CREATE TABLE outbox_events (
    id                    TEXT PRIMARY KEY,
    tenant_id             UUID NOT NULL REFERENCES tenants (id),
    aggregate_type        TEXT NOT NULL,
    aggregate_id          TEXT NOT NULL,
    event_type            TEXT NOT NULL,
    payload               JSONB NOT NULL,
    created_at            TIMESTAMPTZ NOT NULL,
    published_at          TIMESTAMPTZ
);

CREATE INDEX idx_outbox_pending ON outbox_events (created_at) WHERE published_at IS NULL;

CREATE TABLE qr_nonces (
    nonce         TEXT PRIMARY KEY,
    consumed_at   TIMESTAMPTZ NOT NULL,
    expires_at    TIMESTAMPTZ NOT NULL
);
