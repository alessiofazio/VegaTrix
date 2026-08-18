-- Tenant-scoped operator settings edited from the dashboard.
-- Process secrets (DATABASE_URL, JWT, master key) stay in .env.

CREATE TABLE tenant_settings (
    tenant_id   UUID PRIMARY KEY REFERENCES tenants (id),
    settings    JSONB NOT NULL DEFAULT '{}',
    updated_at  TIMESTAMPTZ NOT NULL
);
