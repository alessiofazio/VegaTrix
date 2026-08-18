# Architecture

OpenPay Protocol is a non-custodial orchestration layer. Settlement always happens on an external rail (mock in v1).

## C4 context

```mermaid
flowchart LR
  MerchantSoft[Gestionale / POS / e-commerce]
  Wallet[Demo wallet]
  API[OpenPay API Rust]
  Worker[OpenPay worker Rust]
  PG[(PostgreSQL)]
  Redis[(Redis)]
  PSP[Connector mock / future PSP]
  MerchantSoft --> API
  Wallet --> API
  API --> PG
  API --> Redis
  API --> PSP
  Worker --> PG
  Worker --> MerchantSoft
```

## Containers

- `openpay-server`: Axum `/v1` merchant, admin, public QR APIs
- `openpay-worker`: outbox drain, webhook delivery, retries, SSRF guard
- `dashboard` / `demo-merchant` / `demo-wallet`: TypeScript UIs
- PostgreSQL: source of truth
- Redis: rate-limit / cache (optional degradation if down in dev)

## Modules

| Crate | Responsibility |
|---|---|
| `openpay-domain` | Entities, newtypes, state machine |
| `openpay-application` | Use cases, routing, QR verify, ports |
| `openpay-persistence` | SQLx, transactions, outbox |
| `openpay-connectors` | `PaymentConnector` trait + registry |
| `openpay-crypto` | HMAC, Argon2id, webhook signatures |
| `openpay-api` | HTTP adapters, JWT/API keys, OpenAPI |
| `openpay-worker` | Async delivery |

## Sync vs async

Creating a payment is synchronous and transactional (row + idempotency + audit + outbox). Merchant webhooks are asynchronous: the worker publishes outbox records into `webhook_deliveries` and retries with jitter.

## Multi-tenancy

Every payment, attempt, webhook, and audit row is scoped by `tenant_id`. API keys and JWTs carry tenant identity; queries include it.

## Connector pattern

Business logic never imports a PSP SDK. It calls `PaymentConnector`. v1 ships `mock-instant`, `manual-test`, and a feature-gated open-banking stub that **does not** connect to a bank.
