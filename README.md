# OpenPay Protocol

Open-core **payment orchestration** for POS, e-commerce, wallets, and merchant software. One API to create, route, observe, and reconcile payments that still settle on existing rails.

This repository’s **core is Rust** (Axum, Tokio, SQLx, PostgreSQL). TypeScript is used only for the dashboard, demo UIs, and a thin SDK wrapper.

> Not a bank, PSP, card issuer, or custodian. v1 is a **sandbox**. It does not move real money.

## What it does

- Payment Request + QR / payment link
- Strict state machine with audit trail
- Connector plugins (mock instant, manual test, open-banking stub)
- Configurable routing policy (no “cheapest rail” claims)
- Signed merchant webhooks via outbox + worker
- Self-hosted Docker Compose

## What it does not do

- Hold funds, run KYC/AML, issue cards, or settle
- Store PAN/CVV/bank credentials
- Provide a live Nexi/Stripe/Visa/SEPA integration in v1
- Replace legal/compliance work for production rails

## Quick start

```bash
cp .env.example .env
docker compose up --build
```

| Surface | URL |
|---|---|
| API | http://localhost:8080 |
| OpenAPI | http://localhost:8080/docs |
| Dashboard | http://localhost:3001 |
| Demo merchant / POS | http://localhost:3002 |
| Demo wallet | http://localhost:3003 |

Demo login: `admin@demo.openpay.local` / `ChangeMeNow_OpenPayDemo1`  
Demo merchant API key: `opk_demo_merchant_sandbox_not_for_production_use_only`

### Demo flow

1. Open the merchant demo and create a **12,00 EUR** order.
2. Open the wallet link / QR.
3. Approve or reject.
4. Watch the till move to `SETTLED` or `FAILED`.
5. Inspect the timeline on the dashboard.
6. Use “Simula callback duplicato” and “Simula timeout”.

## API surface (v1)

| Area | Examples |
|---|---|
| Merchant | `POST/GET /v1/payment-requests`, cancel, refund, attempts, events |
| Public wallet | `GET /v1/public/payments/{id}`, `POST .../authorize`, `POST .../simulate-duplicate` |
| Admin | `/v1/admin/overview`, reconcile, resolve attempt, webhooks, routing |
| Auth | `POST /v1/auth/login`, `POST /v1/auth/refresh` |
| Ops | `GET /healthz`, `GET /readyz`, Prometheus on `:9090` when `TELEMETRY_OPT_IN=true` |

Worker reconciles ambiguous `PROCESSING` payments on a tick; webhook delivery uses a circuit breaker after repeated failures.

## Architecture (short)

```text
HTTP / wallet / CLI
        ↓
openpay-api (Axum)
        ↓
openpay-application (use cases, routing, idempotency)
        ↓
openpay-domain (state machine)
        ↓
ports (traits)
        ↓
PostgreSQL + Redis + connector adapters + worker
```

JSON APIs use **snake_case**. Money is **integer minor units**. IDs are **prefixed UUIDv7**.

## Local Rust (without Docker)

Requires a Rust stable toolchain **and** a C linker (MSVC Build Tools on Windows, or compile via Docker).

```bash
cargo test --workspace
cargo run -p openpay-server
cargo run -p openpay-worker-bin
cargo run -p openpay-cli -- seed
```

PostgreSQL and Redis must be running. Configuration is validated at boot from environment variables (see `.env.example`).

## Editions

| Edition | Intent |
|---|---|
| Community self-hosted | Internal use, core protocol, mock connectors |
| Managed cloud | Hosted ops, official adapters (future, commercial) |
| Enterprise | SSO/SCIM/HSM and contractual extras (not in this repo) |

Feature flags are typed through `EditionCapabilities`. Telemetry is **opt-in**.

## License

Draft **OpenPay Sustainable Use License**. Community code is for internal self-hosting. Hosting it as a competing service for third parties, white-label, or embedding for customers requires a commercial agreement. See `LICENSE`, `LICENSE-COMMERCIAL.md`, and `docs/licensing/LICENSING-FAQ.md`.

**NOT LEGAL ADVICE — MUST BE REVIEWED BY QUALIFIED COUNSEL BEFORE PUBLIC RELEASE.**
