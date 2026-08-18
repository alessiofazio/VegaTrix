# API docs

Merchant API v1 is generated from Rust via utoipa.

Run the server and open http://localhost:8080/docs (`/docs/openapi.json`).

**Guida completa (italiano, allineata al router reale):** [`API-GUIDE.md`](API-GUIDE.md)

Also:

- Protocol + connettori + state machine: [`../protocol/IMPLEMENTING.md`](../protocol/IMPLEMENTING.md)
- Esempi multi-linguaggio: [`../sdk/MULTI-LANGUAGE.md`](../sdk/MULTI-LANGUAGE.md)

OpenAPI documents a **subset**: merchant create/get payment, `POST /v1/auth/login`, `POST /v1/auth/refresh`, `POST /v1/public/payments/{id}/authorize`, `POST /v1/admin/payments/{id}/reconcile`, `/healthz`, `/readyz`. The human guide lists every route registered in `crates/openpay-api/src/router.rs` (list/cancel/refund/attempts/events, public GET/QR/simulate-duplicate, remaining admin GETs).

Canonical JSON is snake_case. Money is integer minor units. IDs are prefixed UUIDv7. Errors use RFC 9457 Problem Details (`application/problem+json`).

Auth: `Authorization: Bearer opk_…` (merchant API key) or Bearer JWT (`typ=access`). Admin routes need role/scope `admin`.

Admin webhook deliveries accept `?payment_id=` to filter by payment.
