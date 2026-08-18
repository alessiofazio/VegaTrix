# Come implementare OpenPay Protocol

## Fatto vs migliorare

**v1 sandbox è completo** per il flusso demo e per un self-host “prod-like” (Compose di produzione, secret hardening, Caddy con CA interna, backup Postgres, alert Prometheus opt-in). Non è un PSP: non sposta denaro reale.

| Fatto in v1 | Ancora migliorabile |
|---|---|
| Payment Request + QR HMAC, state machine, audit | Connettori **live** (Stripe/Nexi/open banking reale) — fuori scope, nessun adapter in-tree |
| Merchant / public / admin / auth HTTP | TLS con certificato **pubblico** (oggi CA interna Caddy) |
| Mock-instant, manual-test, stub OB feature-gated | SSO / SCIM / HSM (edition enterprise, non in repo) |
| Routing con fallback bounded (non “cheapest rail”) | Dashboard più ricca, analytics, multi-merchant JWT senza fallback demo |
| Webhook firmati outbox + worker, SSRF, circuit breaker | Endpoint HTTP inbound PSP (oggi mapping via `fetch_attempt` / reconcile) |
| Idempotenza, money integer, ID prefissati, Problem Details | `cancel`/`refund` merchant **non** chiamano ancora il connettore |
| Docker Compose demo + `docker-compose.prod.yml` | License ancora **draft**; no claim PCI/PSD2 |

Dettaglio API: [`docs/api/API-GUIDE.md`](../api/API-GUIDE.md). Linguaggi: [`docs/sdk/MULTI-LANGUAGE.md`](../sdk/MULTI-LANGUAGE.md).

---

## Cos’è (e cosa non è)

OpenPay è un **orchestratore non-custodiale**: crea la Payment Request, instrada verso un `PaymentConnector`, osserva lo stato, riconcilia, notifica il merchant. Settlement sempre su un binario esterno (in v1: mock).

```text
Merchant  --API key-->  POST /v1/payment-requests  -->  PENDING + QR
Wallet    --QR token--> POST /v1/public/.../authorize --> PROCESSING
                         PaymentConnector.create_attempt
                         --> SETTLED | FAILED | REQUIRES_ACTION | PROCESSING
Worker    --outbox-->   POST merchant webhook (OpenPay-Signature)
Worker    --tick-->     expire (non PROCESSING) + reconcile PROCESSING
```

---

## A) Merchant che integra

### 1. Autenticazione

Usa `Authorization: Bearer opk_…`. In demo:

`opk_demo_merchant_sandbox_not_for_production_use_only`

JWT (`POST /v1/auth/login`) è per dashboard/admin, non per il POS.

### 2. Crea il pagamento

`POST /v1/payment-requests` con `Idempotency-Key` univoco per tentativo d’ordine (UUID). Body snake_case:

```json
{
  "merchant_order_id": "ORD-1001",
  "amount_minor": 1200,
  "currency": "EUR",
  "description": "Tavolo 4",
  "allowed_methods": ["ACCOUNT_TO_ACCOUNT"],
  "expires_in_seconds": 300,
  "metadata": { "store_id": "MILANO-001" }
}
```

Risposta: `id` (`pay_…`), `status` (`PENDING`), `payment_url`, `qr_payload` (`openpay://v1/pay/…?token=…`), `qr_svg`.

Mostra il QR o apri `payment_url` sul wallet. **Non** fidarti dell’importo nel QR: riletto dal server.

### 3. Poll

`GET /v1/payment-requests/{id}` ogni 1–2 s finché lo stato è terminale per la cassa: `SETTLED`, `FAILED`, `CANCELLED`, `EXPIRED` (e in seguito `REFUNDED`).

### 4. Webhook (fonte di verità asincrona)

Non usare solo il poll. Esporre un HTTPS (in compose demo: HTTP sull’hostname allowlistato) che:

1. Legge il **raw body**.
2. Verifica `OpenPay-Signature: t=…,v1=…` con `WEBHOOK_SIGNING_SECRET` (HMAC-SHA256 di `{t}.{body}`, finestra 300 s). Vedi SDK TypeScript `verifyWebhookSignature`.
3. Risponde **2xx** in fretta; elabora `data.payment_id` / `data.status` / `type`.
4. È **idempotente**: callback duplicati sono normali (`simulate-duplicate` in demo).

Eventi: `payment.created`, `payment.processing`, `payment.requires_action`, `payment.authorized`, `payment.settled`, `payment.failed`, `payment.cancelled`, `payment.expired`, `payment.refunded`.

### 5. Cancel / refund

- Cancel: `POST .../cancel` da stati che la machine permette (`PENDING`, `REQUIRES_ACTION`, `AUTHORIZED`).
- Refund: `POST .../refunds` da `SETTLED` (o `PARTIALLY_REFUNDED`) → `REFUNDED`.

In v1 queste rotte aggiornano il **dominio OpenPay**, non il PSP.

### 6. Scenari sandbox

Campo `scenario` in create (finisce in metadata) o in authorize:

| Valore | Effetto `mock-instant` |
|---|---|
| `success` / omesso | `SETTLED` immediato |
| `decline` | `FAILED` / `PAYER_DECLINED` |
| `timeout` | errore TIMEOUT, attempt `AMBIGUOUS`, pagamento resta `PROCESSING` finché il worker/admin reconcilia |
| `unavailable` | `CONNECTOR_UNAVAILABLE` (fallback se la policy lo consente) |
| `delayed` | attempt `PROCESSING`, poi `fetch_attempt` → `SETTLED` |
| `duplicate` | create già `SETTLED` (demo) |

---

## B) Autori di connettori (`PaymentConnector`)

Il confine PSP è **solo** il trait `PaymentConnector` in `crates/openpay-connectors`. La business logic non importa SDK di banche.

### Trait (metodi reali)

```text
key() -> &str
capabilities() -> ConnectorCapabilities
health_check() -> ConnectorHealth
create_attempt(CreatePaymentAttemptInput) -> CreatePaymentAttemptOutput
fetch_attempt(GetPaymentAttemptInput) -> NormalizedAttemptStatus
cancel_attempt(CancelPaymentAttemptInput) -> CancelPaymentAttemptOutput
refund_attempt(RefundPaymentAttemptInput) -> RefundPaymentAttemptOutput
```

**Input create:** `payment_id`, `amount_minor`, `currency`, `method`, `scenario?`, `idempotency_key`.

**Output create:** `provider_reference` (stringa persistita sull’attempt), `status` (`AttemptStatus`), `action_url?`, `rail_type`.

`AttemptStatus` ammessi: `CREATED`, `REQUIRES_ACTION`, `PROCESSING`, `AUTHORIZED`, `SETTLED`, `FAILED`, `CANCELLED`, `EXPIRED`, `AMBIGUOUS`. `AMBIGUOUS` mappa a pagamento `PROCESSING` (il worker deve fare poll).

**Errori** (`ConnectorError`): `Unavailable`, `Timeout`, `Declined`, `Ambiguous`, `NotSupported`, `Message`. Codici: `CONNECTOR_UNAVAILABLE`, `TIMEOUT`, `PAYER_DECLINED`, `AMBIGUOUS`, `NOT_SUPPORTED`, `CONNECTOR_ERROR`. Il fallback routing ritenta solo failure **technical** ammesse dalla policy (seed: `TIMEOUT`, `CONNECTOR_UNAVAILABLE`), max `max_attempts` (seed: 2, clamp 1–8).

### `capabilities`

```json
{
  "methods": ["ACCOUNT_TO_ACCOUNT"],
  "refunds": true,
  "delayed_capture": false,
  "webhooks": true,
  "sandbox_only": true
}
```

In v1 tutti i connettori in-tree hanno `sandbox_only: true`. Un adapter live **non** esiste in questo repository e non va finto.

### `provider_reference`

Identificativo **lato PSP/mock** dell’attempt. Serve a `fetch` / `cancel` / `refund` e alla riconciliazione. Esempi mock: `mock_{uuid}`, `timeout_{payment_id}`, `man_{uuid}`.

### Registrazione (registry)

In `bins/openpay-server/src/main.rs` (e analogo worker):

1. `ConnectorRegistry::new()`
2. `registry.register(Arc::new(YourConnector::with_store(...)))` se `FEATURE_CONNECTOR_MOCK` / capability edition
3. Inserire riga in tabella `connectors` (key, capabilities, priority, `sandbox_only`) — il seed registra `mock-instant` (priority 100) e `manual-test` (10)
4. Policy di routing in `routing_policies.rules_json` con `"select": "your-key"`

Connettori in-tree:

| Crate | `key()` | Note |
|---|---|---|
| `connector-mock-instant` | `mock-instant` | scenari sandbox; stato in `SandboxAttemptStore` (Postgres in compose) |
| `connector-manual-test` | `manual-test` | `REQUIRES_ACTION` finché admin `POST /v1/admin/attempts/{id}/resolve` |
| `connector-open-banking-stub` | `open-banking-stub` | **skeleton**: ogni metodo restituisce `NotSupported`; non parla con una banca |

Server e worker **devono** condividere lo stesso `SandboxAttemptStore` (Postgres), altrimenti dopo restart le decisioni mock spariscono. In-memory solo per unit test.

### Mapping webhook inbound PSP

**Non esiste** una rotta HTTP tipo `/v1/webhooks/{connector}` nel router v1. Il pattern da implementare (come fa già reconcile):

1. Il PSP chiama **il tuo adapter** (nuovo route che aggiungi tu, non inventato qui).
2. Verifichi la firma **del PSP** (non `OpenPay-Signature`).
3. Risolvi l’attempt tramite `provider_reference`.
4. Aggiorna lo store del connettore / chiama `fetch_attempt`.
5. Normalizza lo status PSP → `AttemptStatus` di dominio (**mai** far trapelare status provider nel JSON merchant).
6. `reconcile_payment` / `apply_status` porta il Payment Request a `SETTLED` / `FAILED` / ecc.

In v1 puoi esercitare lo stesso percorso con:

- worker tick → `reconcile_stale_attempts` su pagamenti `PROCESSING`
- `POST /v1/admin/payments/{id}/reconcile`
- `POST /v1/public/payments/{id}/simulate-duplicate` (demo: duplicato ignorato)

`PaymentConnector` non riceve il raw webhook: lo mappa l’adapter HTTP che scriverai.

### Manual rail

Implementa anche `ManualAttemptResolver::resolve(provider_reference, approve)`. Il server lo tiene in `ConnectorRuntime.manual`.

---

## C) State machine

Canonical in `openpay_domain::PaymentStatus`. Gli status adapter **non** appaiono in API.

```text
CREATED ──► PENDING ──► REQUIRES_ACTION ──► PROCESSING ──► AUTHORIZED ──► SETTLED
                │              │                  │              │            │
                ├── FAILED     ├── FAILED         ├── SETTLED    ├── FAILED   ├── REFUND_PENDING
                ├── CANCELLED  ├── CANCELLED      └── FAILED     └── CANCELLED ├── PARTIALLY_REFUNDED
                └── EXPIRED    └── EXPIRED                                      └── REFUNDED
```

Transizioni **permesse** (`allowed_targets`):

| Da | Verso |
|---|---|
| CREATED | PENDING |
| PENDING | REQUIRES_ACTION, PROCESSING, FAILED, CANCELLED, EXPIRED |
| REQUIRES_ACTION | PROCESSING, FAILED, CANCELLED, EXPIRED |
| PROCESSING | AUTHORIZED, SETTLED, FAILED |
| AUTHORIZED | SETTLED, FAILED, CANCELLED |
| SETTLED | REFUND_PENDING, PARTIALLY_REFUNDED, REFUNDED |
| REFUND_PENDING | REFUNDED, PARTIALLY_REFUNDED, FAILED |
| PARTIALLY_REFUNDED | REFUNDED, REFUND_PENDING |
| FAILED, CANCELLED, EXPIRED, REFUNDED | *(terminali)* |

Regole importanti:

- Stesso stato → no-op idempotente.
- Transizione illegale → **409** `illegal-transition`.
- **`PROCESSING` non può diventare `EXPIRED`.** Timeout/ambiguità restano in `PROCESSING` finché `fetch_attempt` (worker o admin reconcile) chiude.
- Create API parte da **`PENDING`** (non espone `CREATED` nel happy path HTTP).
- Authorize pagatore: se non già `PROCESSING`, transita a `PROCESSING`, poi `create_attempt`.
- Reject pagatore: `FAILED`.

Flusso cassa tipico: `PENDING` → `PROCESSING` → `SETTLED` | `FAILED`. Manual: `PENDING` → `PROCESSING` → `REQUIRES_ACTION` → (admin resolve) `SETTLED` | `FAILED`.

---

## Token QR (implementazione)

1. Alla create, `QrClaims` (payment, tenant, merchant, `exp` = `expires_at`, nonce, `v=1`) firmati HMAC-SHA256.
2. Payload QR: `openpay://v1/pay/{id}?token={b64}.{sig}`.
3. GET pubblica verifica firma + `exp` + binding path; **non** consuma nonce.
4. POST authorize verifica e **consuma** nonce (Redis/Postgres remember 900 s) → replay → 403 `Replay`.

Importo e merchant **solo server-side**.

---

## Routing

Policy JSON (seed “EUR instant preferred”): regole `when` / `select` / `priority`, poi fallback bounded. Non c’è selezione “binario più economico”. `country` nel contesto di routing è `"IT"` hardcoded in `decide_route`.
