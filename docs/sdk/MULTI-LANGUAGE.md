# OpenPay in più linguaggi

Stessi flussi HTTP della guida API. Prosa in italiano; identificatori e JSON in inglese.

**Sandbox only.** Base URL `http://localhost:8080`. Chiave:

`opk_demo_merchant_sandbox_not_for_production_use_only`

Non usare in produzione. Importi: interi `amount_minor`. JSON snake_case. Header `Idempotency-Key` obbligatorio sulla create.

SDK TypeScript esistente: [`sdk/typescript`](../../sdk/typescript) (`@openpay/sdk`). Wrapper Rust: crate `openpay-sdk-rust`. Python/Go/PHP sotto sono esempi HTTP, non pacchetti pubblicati.

---

## Flusso comune

1. `POST /v1/payment-requests` → `PENDING`, QR / `payment_url`
2. Poll `GET /v1/payment-requests/{id}` **oppure** webhook `OpenPay-Signature`
3. Opzionale: cancel, refund, authorize pubblico (wallet)

Webhook: HMAC-SHA256 di `"{t}." + raw_body`, header `OpenPay-Signature: t=<unix>,v1=<hex>`, secret `WEBHOOK_SIGNING_SECRET`, tolleranza 300 s.

---

## cURL

Vedi anche [`sdk/curl/examples.sh`](../../sdk/curl/examples.sh).

```bash
# SANDBOX ONLY — localhost demo key
export OPENPAY_BASE=http://localhost:8080
export OPENPAY_KEY=opk_demo_merchant_sandbox_not_for_production_use_only
IDEMPOTENCY_KEY=$(uuidgen 2>/dev/null || cat /proc/sys/kernel/random/uuid)

curl -sS -X POST "$OPENPAY_BASE/v1/payment-requests" \
  -H "Authorization: Bearer $OPENPAY_KEY" \
  -H "content-type: application/json" \
  -H "Idempotency-Key: $IDEMPOTENCY_KEY" \
  -d '{
    "merchant_order_id": "ORD-LANG-1",
    "amount_minor": 1200,
    "currency": "EUR",
    "allowed_methods": ["ACCOUNT_TO_ACCOUNT"],
    "expires_in_seconds": 300
  }'
```

Poll, cancel, refund, login:

```bash
PAY=pay_REPLACE

curl -sS "$OPENPAY_BASE/v1/payment-requests/$PAY" \
  -H "Authorization: Bearer $OPENPAY_KEY"

curl -sS -X POST "$OPENPAY_BASE/v1/payment-requests/$PAY/cancel" \
  -H "Authorization: Bearer $OPENPAY_KEY"

curl -sS -X POST "$OPENPAY_BASE/v1/payment-requests/$PAY/refunds" \
  -H "Authorization: Bearer $OPENPAY_KEY"

curl -sS -X POST "$OPENPAY_BASE/v1/auth/login" \
  -H "content-type: application/json" \
  -d '{"email":"admin@demo.openpay.local","password":"ChangeMeNow_OpenPayDemo1"}'
```

Wallet (token dal `qr_payload`):

```bash
curl -sS "$OPENPAY_BASE/v1/public/payments/$PAY?token=$TOKEN"
curl -sS -X POST "$OPENPAY_BASE/v1/public/payments/$PAY/authorize" \
  -H "content-type: application/json" \
  -d "{\"token\":\"$TOKEN\",\"decision\":\"approve\"}"
```

---

## TypeScript

Pacchetto locale `sdk/typescript` — fetch + `verifyWebhookSignature`. Nessuna business logic.

```ts
import { OpenPay } from "@openpay/sdk";

// SANDBOX ONLY
const client = new OpenPay(
  "http://localhost:8080",
  "opk_demo_merchant_sandbox_not_for_production_use_only",
);

const payment = await client.createPayment(crypto.randomUUID(), {
  merchant_order_id: "ORD-TS-1",
  amount_minor: 1200,
  currency: "EUR",
  allowed_methods: ["ACCOUNT_TO_ACCOUNT"],
  expires_in_seconds: 300,
});

const latest = await client.getPayment(payment.id);

// Express/Fastify: usa req.rawBody, non JSON.stringify(req.body)
const ok = client.verifyWebhookSignature(
  process.env.WEBHOOK_SIGNING_SECRET!,
  req.header("openpay-signature") ?? "",
  rawBody,
  300,
);
```

Metodi SDK: `createPayment`, `getPayment`, `cancelPayment`, `refundPayment`, `verifyWebhookSignature`. Login admin: HTTP diretto su `/v1/auth/login` (non è nel wrapper).

---

## Python

Esempio eseguibile: [`sdk/python/openpay_example.py`](../../sdk/python/openpay_example.py). Dipendenza: `httpx` o `requests`.

```python
import hmac, hashlib, time, uuid
import httpx  # pip install httpx

# SANDBOX ONLY
BASE = "http://localhost:8080"
KEY = "opk_demo_merchant_sandbox_not_for_production_use_only"
headers = {
    "Authorization": f"Bearer {KEY}",
    "content-type": "application/json",
    "Idempotency-Key": str(uuid.uuid4()),
}
body = {
    "merchant_order_id": "ORD-PY-1",
    "amount_minor": 1200,
    "currency": "EUR",
    "allowed_methods": ["ACCOUNT_TO_ACCOUNT"],
    "expires_in_seconds": 300,
}
r = httpx.post(f"{BASE}/v1/payment-requests", headers=headers, json=body)
r.raise_for_status()
payment = r.json()  # id, status, amount_minor, qr_payload, ...

def verify_openpay_signature(secret: str, header: str, raw_body: bytes, tolerance=300) -> bool:
    parts = dict(p.strip().split("=", 1) for p in header.split(",") if "=" in p)
    t, v1 = parts.get("t"), parts.get("v1")
    if not t or not v1:
        return False
    if abs(int(time.time()) - int(t)) > tolerance:
        return False
    expected = hmac.new(secret.encode(), f"{t}.".encode() + raw_body, hashlib.sha256).hexdigest()
    return hmac.compare_digest(expected, v1)
```

Equivalente `requests`: `requests.post(..., headers=headers, json=body)`.

---

## Go

```go
package main

import (
	"bytes"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"net/http"
	"strconv"
	"strings"
	"time"
)

// SANDBOX ONLY
const base = "http://localhost:8080"
const key = "opk_demo_merchant_sandbox_not_for_production_use_only"

func createPayment(idempotencyKey string) {
	payload, _ := json.Marshal(map[string]any{
		"merchant_order_id":  "ORD-GO-1",
		"amount_minor":       1200,
		"currency":           "EUR",
		"allowed_methods":    []string{"ACCOUNT_TO_ACCOUNT"},
		"expires_in_seconds": 300,
	})
	req, _ := http.NewRequest(http.MethodPost, base+"/v1/payment-requests", bytes.NewReader(payload))
	req.Header.Set("Authorization", "Bearer "+key)
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Idempotency-Key", idempotencyKey)
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		panic(err)
	}
	defer resp.Body.Close()
	fmt.Println(resp.Status)
}

func verifySignature(secret, header string, raw []byte) bool {
	var t, v1 string
	for _, part := range strings.Split(header, ",") {
		kv := strings.SplitN(strings.TrimSpace(part), "=", 2)
		if len(kv) != 2 {
			continue
		}
		switch kv[0] {
		case "t":
			t = kv[1]
		case "v1":
			v1 = kv[1]
		}
	}
	ts, err := strconv.ParseInt(t, 10, 64)
	if err != nil || v1 == "" {
		return false
	}
	if abs(time.Now().Unix()-ts) > 300 {
		return false
	}
	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write([]byte(t + "."))
	mac.Write(raw)
	return hmac.Equal(mac.Sum(nil), mustDecodeHex(v1))
}

func mustDecodeHex(s string) []byte { b, _ := hex.DecodeString(s); return b }
func abs(n int64) int64 {
	if n < 0 {
		return -n
	}
	return n
}
```

Nota: `v1` nel protocollo è **hex ASCII**, non raw bytes. In Python/TS si confronta la hex string; in Go si può confrontare i digest dopo `hex.DecodeString`.

---

## PHP

```php
<?php
// SANDBOX ONLY
$base = 'http://localhost:8080';
$key  = 'opk_demo_merchant_sandbox_not_for_production_use_only';

$payload = json_encode([
  'merchant_order_id' => 'ORD-PHP-1',
  'amount_minor' => 1200,
  'currency' => 'EUR',
  'allowed_methods' => ['ACCOUNT_TO_ACCOUNT'],
  'expires_in_seconds' => 300,
]);

$ch = curl_init("$base/v1/payment-requests");
curl_setopt_array($ch, [
  CURLOPT_POST => true,
  CURLOPT_HTTPHEADER => [
    "Authorization: Bearer $key",
    'content-type: application/json',
    'Idempotency-Key: ' . bin2hex(random_bytes(16)),
  ],
  CURLOPT_POSTFIELDS => $payload,
  CURLOPT_RETURNTRANSFER => true,
]);
echo curl_exec($ch);

function verify_openpay_signature(string $secret, string $header, string $rawBody, int $tolerance = 300): bool {
  $parts = [];
  foreach (explode(',', $header) as $p) {
    [$k, $v] = array_pad(explode('=', trim($p), 2), 2, null);
    if ($k && $v) $parts[$k] = $v;
  }
  if (empty($parts['t']) || empty($parts['v1'])) return false;
  if (abs(time() - (int)$parts['t']) > $tolerance) return false;
  $expected = hash_hmac('sha256', $parts['t'] . '.' . $rawBody, $secret);
  return hash_equals($expected, $parts['v1']);
}
```

---

## Java (breve)

```java
// SANDBOX ONLY — HttpClient JDK 11+
var client = HttpClient.newHttpClient();
var body = """
  {"merchant_order_id":"ORD-JV-1","amount_minor":1200,"currency":"EUR",
   "allowed_methods":["ACCOUNT_TO_ACCOUNT"],"expires_in_seconds":300}
  """;
var req = HttpRequest.newBuilder()
    .uri(URI.create("http://localhost:8080/v1/payment-requests"))
    .header("Authorization", "Bearer opk_demo_merchant_sandbox_not_for_production_use_only")
    .header("content-type", "application/json")
    .header("Idempotency-Key", UUID.randomUUID().toString())
    .POST(HttpRequest.BodyPublishers.ofString(body))
    .build();
var res = client.send(req, HttpResponse.BodyHandlers.ofString());

// Firma: Mac.getInstance("HmacSHA256") su (t + "." + rawBody), hex lowercase
```

---

## C# (breve)

```csharp
// SANDBOX ONLY
var http = new HttpClient { BaseAddress = new Uri("http://localhost:8080") };
http.DefaultRequestHeaders.Authorization =
    new AuthenticationHeaderValue("Bearer", "opk_demo_merchant_sandbox_not_for_production_use_only");
var json = """
  {"merchant_order_id":"ORD-CS-1","amount_minor":1200,"currency":"EUR",
   "allowed_methods":["ACCOUNT_TO_ACCOUNT"],"expires_in_seconds":300}
  """;
var msg = new HttpRequestMessage(HttpMethod.Post, "/v1/payment-requests");
msg.Headers.Add("Idempotency-Key", Guid.NewGuid().ToString());
msg.Content = new StringContent(json, Encoding.UTF8, "application/json");
var res = await http.SendAsync(msg);

// HMACSHA256(Encoding.UTF8.GetBytes($"{t}."), rawBody) → hex
```

---

## Errori

Risposta non-2xx: `Content-Type: application/problem+json` (`type`, `title`, `status`, `detail`). Non parsare come `PaymentView`.

Admin JWT: `POST /v1/auth/login` poi `Authorization: Bearer {access_token}` su `/v1/admin/overview`, ecc.
