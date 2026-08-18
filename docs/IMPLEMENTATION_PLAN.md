# OpenPay Protocol — Implementation Plan (v1)

**Draft.** Core backend is **Rust stable** (Cargo workspace, Axum, Tokio, SQLx/PostgreSQL). TypeScript is limited to dashboard, demo UIs, and generated/wrapper SDKs.

## Goal of v1

Prove a complete, auditable sandbox flow:

```text
demo-merchant → API (Rust) → PaymentRequest + QR/token
      → demo-wallet authorize/reject
      → routing + mock connector
      → outbox → worker webhook delivery
      → dashboard timeline / audit
```

The product is **non-custodial orchestration**. It does not hold funds, issue cards, or settle money.

## Stack decisions

| Area | Choice | Why |
|---|---|---|
| Core language | Rust stable, edition 2024 (fallback 2021 if toolchain requires) | Memory safety, concurrency, predictable ops for payment state |
| HTTP | Axum + Tokio | Idiomatic, composable middleware, OpenAPI via utoipa |
| DB | PostgreSQL + SQLx | ACID, compile-friendly queries, versioned migrations |
| Jobs | PostgreSQL outbox + dedicated Tokio worker | Same-commit events; no extra broker for v1 |
| Cache / rate-limit | Redis | Token bucket + webhook circuit breaker counters |
| Money | `i64` minor units (`AmountMinor`) | Never float |
| IDs | UUIDv7 newtypes | Time-sortable, unique, API-friendly |
| JSON | `snake_case` | Matches published API contracts |
| Errors | RFC 9457 Problem Details | Stable public error shape |
| Auth v1 | JWT access/refresh + hashed API keys | OIDC-ready interfaces; no mandatory cloud IdP |
| QR | HMAC-SHA256 short-lived token | Server is source of truth |
| Editions | `EditionCapabilities` | Community / cloud / enterprise gating |

## Crate dependency direction

```text
openpay-domain
  ↑
openpay-application   ← ports (traits)
  ↑                 ↑
openpay-persistence openpay-connectors
  ↑                 ↑
openpay-api       openpay-worker
  ↑                 ↑
openpay-server    openpay-worker binary
```

- `openpay-domain` has zero HTTP/DB/Redis deps.
- Application depends on ports, not SQLx or Axum.
- API and worker are thin adapters.

## Build order

1. Workspace, config, domain, crypto, observability.
2. Application use cases, routing, idempotency, QR policy.
3. SQLx migrations + repositories + outbox.
4. Connectors (mock-instant, manual-test, open-banking stub).
5. Axum `/v1` merchant + admin API + OpenAPI.
6. Worker: outbox, webhooks, SSRF guard, retries, reconciliation.
7. Bins: server, worker, CLI.
8. Web: dashboard, demo-merchant, demo-wallet.
9. SDK Rust + TypeScript wrapper.
10. Docker Compose, CI, docs, license drafts, tests.

## Persistence rules

- Explicit transactions around every payment state change.
- Unique `(tenant_id, idempotency_key, request_fingerprint)`.
- Optimistic locking via `version` on payment rows.
- Outbox rows inserted in the same commit as the mutation.
- SQLite only behind feature `sqlite-demo` (not production).

## Security baseline for v1

- Tenant isolation on every query.
- API keys stored as Argon2id/HMAC hashes, shown once.
- Webhook HMAC `OpenPay-Signature` over timestamp + raw body.
- SSRF deny of private/link-local/metadata IPs for webhook URLs.
- No PAN/CVV/bank credentials stored.
- Logs redact tokens, secrets, and raw payment payloads.
- CORS closed by default; configurable allowlist.

## Demo acceptance

`docker compose up --build` must allow: create 12.00 EUR order → QR/link → wallet approve/reject → mock connector → `SETTLED`/`FAILED` → dashboard audit → duplicate callback idempotency → timeout → `PROCESSING` until reconcile.

## Out of scope for v1

Real PSP/bank connectors, KYC/AML, card data, blockchain, production compliance certification, enterprise SSO/SCIM (interfaces only).
