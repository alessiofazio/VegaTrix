# Guida API OpenPay Protocol (v1)

Guida operativa dell’HTTP API così come è implementata in `crates/openpay-api` (router, DTO, auth). Non copre integrazioni PSP live (Stripe, Nexi, ecc.): in v1 i connettori sono sandbox.

**Sandbox only.** Chiave demo e `localhost:8080` non sono per produzione.

| | |
|---|---|
| Base URL demo | `http://localhost:8080` |
| OpenAPI / Swagger | [http://localhost:8080/docs](http://localhost:8080/docs) (`/docs/openapi.json`) |
| Chiave merchant demo | `opk_demo_merchant_sandbox_not_for_production_use_only` |
| Login dashboard demo | `admin@demo.openpay.local` / `ChangeMeNow_OpenPayDemo1` |

JSON **snake_case**. Importi **interi in unità minori** (`amount_minor`: 12,00 EUR → `1200`). ID **prefissati** (UUIDv7). Errori **RFC 9457** `application/problem+json`.

---

## Convenzioni

### Autenticazione

Tutte le rotte merchant e admin richiedono:

```http
Authorization: Bearer <token>
```

Due schemi, nello stesso header:

| Schema | Forma del token | Ruolo | Uso |
|---|---|---|---|
| API key | inizia con `opk_` | `merchant` + scope dal DB | POS / backend merchant |
| JWT access | JWT HS256, claim `typ=access` | `admin` o `merchant` | dashboard, `/v1/admin/*` |

- Le API key sono hashed (Argon2id); il plaintext si vede **una volta** (seed demo). Chiave revocata → `401`.
- JWT **access** scade dopo **15 minuti** (`expires_in: 900`). JWT **refresh** dura **7 giorni**, claim `typ=refresh`.
- Le rotte `/v1/admin/*` richiedono `role == "admin"` **oppure** scope `admin`. Una chiave `opk_…` di merchant **non** passa.
- Le rotte `/v1/public/*` **non** usano Bearer: autenticano il pagatore con il **token QR** (`?token=` o body).
- `/v1/auth/login` e `/v1/auth/refresh` sono pubblici (rate-limit IP sulle public; login è sotto `/v1` senza il layer merchant).

JWT senza `merchant_id`: `POST/GET /v1/payment-requests` (lista/create) usa il merchant demo seed. È un comportamento v1 sandbox, non un modello multi-merchant da produzione.

### Idempotenza

`POST /v1/payment-requests` **richiede** l’header `Idempotency-Key` (1–128 caratteri). Stessa chiave + stesso fingerprint (`merchant_id|merchant_order_id|amount_minor|currency|idempotency_key`) → **HTTP 200** e `replayed: true`. Payload diverso con la stessa chiave → **409** `idempotency`.

### Denaro

- Campo: `amount_minor` (`i64` > 0). Mai float.
- `currency`: ISO 4217, esattamente 3 lettere (`EUR`, `USD`, `GBP`, …).
- 12,00 EUR = `1200`. 10,50 EUR = `1050`.

### ID prefissati

Formato `{prefisso}_{uuid32}` (UUIDv7 senza trattini, `as_simple`).

| Prefisso | Entità |
|---|---|
| `pay_` | Payment request |
| `mch_` | Merchant |
| `ten_` | Tenant |
| `att_` | Attempt |
| `con_` | Connector |
| `evt_` | Evento / outbox |
| `whe_` | Webhook endpoint |
| `whd_` | Webhook delivery |
| `aud_` | Audit |
| `rpl_` | Routing policy |
| `key_` | API key (id interno, non il secret `opk_`) |
| `usr_` | User |
| `otx_` | Outbox |
| `rfd_` | Refund |

Il parser accetta anche UUID nudo (32/36 char) ma le risposte usano sempre il prefisso.

### Errori (Problem Details)

`Content-Type: application/problem+json`

```json
{
  "type": "https://openpay.local/problems/validation",
  "title": "Validation failed",
  "status": 400,
  "detail": "Idempotency-Key header is required"
}
```

| `type` (kind) | HTTP tipico | Quando |
|---|---|---|
| `auth` | 401 | Bearer mancante, JWT/chiave invalidi |
| `forbidden` | 403 | Non admin; token QR rifiutato |
| `validation` | 400 | Body / ID / header |
| `not-found` | 404 | Risorsa assente (o merchant mismatch mascherato) |
| `illegal-transition` | 409 | Transizione di stato illegale |
| `idempotency` | 409 | Chiave riusata con payload diverso |
| `version-conflict` | 409 | Lock ottimistico: ritenta |
| `conflict` | 409 | Altro conflitto |
| `expired` | 410 | Token QR o pagamento scaduto |
| `connector` | 422 | Errore connettore / routing |
| `internal` | 500 | Infra (dettaglio non esposto) |

Limite body: **64 KiB**. Timeout richiesta: **30 s** → 408. Rate limit default **120/min** (`RATE_LIMIT_PER_MINUTE`) → 429 + `Retry-After: 60`.

Header di correlazione: `X-Request-Id` (generato e propagato).

Timestamp JSON (`expires_at`, `created_at`, …): il crate `time` con feature `serde` serializza `OffsetDateTime` come **intero Unix (secondi)**.

---

## Auth

### `POST /v1/auth/login`

```json
{ "email": "admin@demo.openpay.local", "password": "ChangeMeNow_OpenPayDemo1" }
```

**200**

```json
{
  "access_token": "<jwt>",
  "refresh_token": "<jwt>",
  "token_type": "Bearer",
  "expires_in": 900,
  "edition": "community",
  "self_hosted": true
}
```

Credenziali errate → 401 `invalid credentials`.

### `POST /v1/auth/refresh`

```json
{ "refresh_token": "<jwt refresh>" }
```

Stessa forma di `TokenResponse`. Un access token **non** vale come refresh (`wrong token type`).

---

## Merchant (`/v1/payment-requests`)

Bearer API key `opk_…` o JWT.

### `POST /v1/payment-requests`

Crea una Payment Request in stato **`PENDING`**. Header obbligatorio: `Idempotency-Key`.

**Body** (`CreatePaymentBody`)

| Campo | Tipo | Vincoli |
|---|---|---|
| `merchant_order_id` | string | 1–128 |
| `amount_minor` | integer | ≥ 1 |
| `currency` | string | 3 lettere |
| `description` | string? | |
| `allowed_methods` | string[]? | default `["ACCOUNT_TO_ACCOUNT"]` |
| `expires_in_seconds` | u32? | 30–3600, default **300** |
| `return_url` | string? | |
| `metadata` | object? | |
| `scenario` | string? | copiato in `metadata.scenario` (sandbox mock) |

Metodi ammessi: `ACCOUNT_TO_ACCOUNT`, `CARD`, `WALLET`, `MANUAL`. Altro → 400.

`scenario` per il connettore `mock-instant`: `success` (default), `decline`, `timeout`, `unavailable`, `duplicate`, `delayed`. Ha effetto **all’authorize** del pagatore, non alla create.

**201** (nuova) o **200** (`replayed: true`)

```json
{
  "id": "pay_…",
  "status": "PENDING",
  "amount_minor": 1200,
  "currency": "EUR",
  "payment_url": "http://localhost:3003/?payment=pay_…&token=…",
  "qr_payload": "openpay://v1/pay/pay_…?token=…",
  "qr_svg": "<svg …>",
  "expires_at": 1730000000,
  "created_at": 1730000000,
  "replayed": false
}
```

`payment_url` punta al wallet (`WALLET_BASE_URL`). Il QR **non** contiene importo né merchant: li impone il server.

### `GET /v1/payment-requests`

Ultimi **50** pagamenti del merchant (ordine `created_at` desc). Array di `PaymentView`.

### `GET /v1/payment-requests/{payment_id}`

`PaymentView`: `id`, `status`, `amount_minor`, `currency`, `merchant_order_id`, `merchant_id`, `expires_at`, `created_at`, `updated_at`, `description`, `metadata`.

Se il pagamento è scaduto e lo stato permette `EXPIRED` (non `PROCESSING`), la GET può **transitare** a `EXPIRED` (side effect di lettura + expiry).

### `POST /v1/payment-requests/{payment_id}/cancel`

Transizione verso `CANCELLED` se la state machine lo consente (`PENDING` / `REQUIRES_ACTION` / `AUTHORIZED`). v1 applica lo stato nel dominio; **non** chiama `cancel_attempt` sul connettore.

### `POST /v1/payment-requests/{payment_id}/refunds`

Solo da `SETTLED` o `PARTIALLY_REFUNDED` → `REFUNDED`. Altrimenti errore di dominio (`not refundable`). v1 **non** chiama `refund_attempt` sul connettore.

### `GET /v1/payment-requests/{payment_id}/attempts`

```json
[{
  "id": "att_…",
  "connector_key": "mock-instant",
  "rail_type": "MOCK_INSTANT",
  "status": "SETTLED",
  "provider_reference": "mock_…",
  "failure_code": null,
  "created_at": 1730000000
}]
```

### `GET /v1/payment-requests/{payment_id}/events`

Audit trail: `id`, `event_type`, `actor_type`, `occurred_at`, `metadata_redacted`.

---

## Public / wallet (`/v1/public`)

Rate-limit per IP. Token QR obbligatorio (query o body), HMAC-SHA256, scadenza `exp`, binding all’`id` del path.

### `GET /v1/public/payments/{payment_id}?token=`

Non consuma il nonce (lettura). Risposta ridotta:

```json
{
  "id": "pay_…",
  "merchant_display_name": "Caffè Aurora",
  "amount_minor": 1200,
  "currency": "EUR",
  "status": "PENDING",
  "expires_at": 1730000000,
  "description": "Espresso + cornetto"
}
```

Token scaduto → **410**. Firma/binding errati → **403**.

### `POST /v1/public/payments/{payment_id}/authorize`

**Consuma** il nonce (anti-replay, TTL 900 s). Body:

```json
{
  "token": "<qr token>",
  "decision": "approve",
  "scenario": "success"
}
```

`decision` case-insensitive: `reject` → `FAILED`; qualsiasi altro valore → approve (routing + `create_attempt`). `scenario` opzionale: override del metadata.

**200**

```json
{
  "id": "pay_…",
  "status": "SETTLED",
  "idempotent_replay": false,
  "routing": {
    "connector": "mock-instant",
    "explanation": "…"
  }
}
```

Pagamento già terminale → `idempotent_replay: true` senza nuovo attempt.

### `POST /v1/public/payments/{payment_id}/simulate-duplicate`

**Solo sandbox / demo.** Rigioca `fetch_attempt` sull’ultimo attempt e **ignora** il duplicato (`duplicate_ignored: true`). Non è un endpoint di produzione. Il body ricalca `AuthorizeBody` ma il token QR **non** viene verificato; il tenant è quello demo.

### `GET /v1/public/payments/{payment_id}/qr.svg?token=`

SVG (`Content-Type: image/svg+xml`), stesso token della GET pubblica. Il path handler si chiama `qr_png` nel codice ma il contenuto è **SVG**.

---

## Admin (`/v1/admin`)

JWT admin. Tutte le query sono scoped al `tenant_id` del token.

| Metodo | Path | Risposta (campi reali) |
|---|---|---|
| GET | `/v1/admin/overview` | `edition`, `self_hosted`, `capabilities` (`advanced_routing`, `analytics`, `sso`, `connector_open_banking`), `payment_counts`, `payments`, `merchants` |
| GET | `/v1/admin/connectors` | `{ "connectors": [{ "key", "health", "capabilities" }] }` |
| GET | `/v1/admin/settings` | `app_name`, `environment`, `edition`, `self_hosted`, `deployment` |
| GET | `/v1/admin/api-keys` | `id`, `name`, `fingerprint`, `revoked`, `scopes` — **mai** il secret |
| GET | `/v1/admin/webhook-endpoints` | `id`, `url`, `status`, `failure_count`, `event_types` |
| GET | `/v1/admin/webhook-deliveries` | opzionale `?payment_id=pay_…` |
| GET | `/v1/admin/routing-policies` | `id`, `name`, `status`, `rules_json`, `fallback_policy` |
| POST | `/v1/admin/payments/{payment_id}/reconcile` | `{ "id", "status" }` — solo se lo stato è `PROCESSING`; chiama `fetch_attempt` |
| POST | `/v1/admin/attempts/{attempt_id}/resolve` | body `{ "approve": true \| false }` — connettore `manual-test` |

`resolve` richiede `FEATURE_CONNECTOR_MOCK` (connettore `manual-test` registrato) e un `provider_reference` sull’attempt.

---

## Ops

### `GET /healthz`

Liveness. Sempre 200 se il processo risponde:

```json
{ "status": "ok", "service": "openpay-server", "version": "…" }
```

### `GET /readyz`

Readiness DB. 200 se `SELECT 1` ok, altrimenti **503** `{ "status": "unavailable", "database": false }`.

Usa `/readyz` come readinessProbe, `/healthz` come livenessProbe.

---

## Webhook merchant (outbound)

Il worker drena l’outbox e POST sull’URL dell’endpoint (`webhook_endpoints`). Seed demo: `{DEMO_MERCHANT_URL}/webhooks/openpay`.

### Header

```http
Content-Type: application/json
OpenPay-Signature: t=<unix>,v1=<hex hmac-sha256>
OpenPay-Event: payment.settled
```

Firma (crate `openpay-crypto`): HMAC-SHA256 del messaggio `{timestamp}.{raw_body}` con `WEBHOOK_SIGNING_SECRET`. Tolleranza default **300 s**.

Verifica (stesso algoritmo dello SDK TypeScript):

1. Parsa `t` e `v1` dall’header (split su `,` e `=`).
2. Scarta se `|now - t| > 300`.
3. `HMAC-SHA256(secret, ascii(t) + "." + raw_body)` in hex.
4. Confronta con `v1`. Usa il **body grezzo**, non un JSON ri-serializzato.

### Payload (`api_version`: `2026-08-18`)

```json
{
  "id": "evt_…",
  "type": "payment.settled",
  "api_version": "2026-08-18",
  "created_at": 1730000000,
  "data": {
    "payment_id": "pay_…",
    "merchant_order_id": "ORD-1",
    "status": "SETTLED",
    "amount_minor": 1200,
    "currency": "EUR",
    "merchant_id": "mch_…"
  }
}
```

Eventi emessi da `PaymentStatus::webhook_event`:

| Stato | `type` |
|---|---|
| CREATED / create | `payment.created` |
| PENDING / PROCESSING | `payment.processing` |
| REQUIRES_ACTION | `payment.requires_action` |
| AUTHORIZED | `payment.authorized` |
| SETTLED | `payment.settled` |
| FAILED | `payment.failed` |
| CANCELLED | `payment.cancelled` |
| EXPIRED | `payment.expired` |
| REFUNDED / PARTIALLY_REFUNDED | `payment.refunded` |
| REFUND_PENDING | nessun webhook |

Il worker considera successo qualsiasi **2xx**. Retry esponenziale, max `WEBHOOK_MAX_ATTEMPTS` (default 8), poi dead-letter. Circuit breaker: dopo **20** failure consecutive il circuito si apre; dopo **60 s** half-open (un probe). SSRF: IP privati/metadata bloccati salvo hostname in `WEBHOOK_URL_ALLOWLIST`.

---

## Token QR

URI: `openpay://v1/pay/{payment_id}?token={token}`

Token: `{base64url(json claims)}.{hmac_sha256_hex}` (HMAC sul base64, secret `QR_SIGNING_SECRET`). Claims: `payment_id`, `tenant_id`, `merchant_id`, `exp`, `nonce`, `v` (1). Authorize consuma `nonce`.

---

## Esempi cURL (sandbox)

```bash
# Solo sandbox. Non usare in produzione.
BASE=http://localhost:8080
KEY=opk_demo_merchant_sandbox_not_for_production_use_only

# Liveness
curl -sS "$BASE/healthz"

# Crea pagamento 12,00 EUR
curl -sS -X POST "$BASE/v1/payment-requests" \
  -H "Authorization: Bearer $KEY" \
  -H "content-type: application/json" \
  -H "Idempotency-Key: $(uuidgen)" \
  -d '{
    "merchant_order_id": "ORD-DEMO-1",
    "amount_minor": 1200,
    "currency": "EUR",
    "description": "Espresso + cornetto",
    "allowed_methods": ["ACCOUNT_TO_ACCOUNT"],
    "expires_in_seconds": 300
  }'

# Poll
curl -sS "$BASE/v1/payment-requests/pay_REPLACE" \
  -H "Authorization: Bearer $KEY"

# Login JWT (admin)
curl -sS -X POST "$BASE/v1/auth/login" \
  -H "content-type: application/json" \
  -d '{"email":"admin@demo.openpay.local","password":"ChangeMeNow_OpenPayDemo1"}'
```

Altri linguaggi: [`docs/sdk/MULTI-LANGUAGE.md`](../sdk/MULTI-LANGUAGE.md). Script: [`sdk/curl/examples.sh`](../../sdk/curl/examples.sh).

OpenAPI generato da utoipa copre un **sottoinsieme** delle rotte (create/get payment, login/refresh, public authorize, admin reconcile, health/ready). Questa guida elenca **tutte** le rotte del `router`.
