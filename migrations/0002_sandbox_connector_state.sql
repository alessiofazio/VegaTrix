-- Sandbox connector attempt ledger shared by API server and worker.
-- Mock/manual rails persist provider references here so reconcile survives restart.

CREATE TABLE sandbox_connector_attempts (
    connector_key         TEXT NOT NULL,
    provider_reference    TEXT NOT NULL,
    status                TEXT NOT NULL,
    created_at            TIMESTAMPTZ NOT NULL,
    updated_at            TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (connector_key, provider_reference)
);
