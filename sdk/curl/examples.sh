#!/usr/bin/env bash
# OpenPay Protocol — cURL examples
# SANDBOX ONLY. Demo key + localhost. Do not use in production.
set -euo pipefail

BASE="${OPENPAY_BASE:-http://localhost:8080}"
KEY="${OPENPAY_KEY:-opk_demo_merchant_sandbox_not_for_production_use_only}"
AUTH=( -H "Authorization: Bearer ${KEY}" -H "content-type: application/json" )

echo "# GET /healthz"
curl -sS "${BASE}/healthz"
echo

IDEMPOTENCY_KEY="${IDEMPOTENCY_KEY:-$(uuidgen 2>/dev/null || python -c 'import uuid; print(uuid.uuid4())')}"
ORDER="ORD-CURL-$(date +%s)"

echo "# POST /v1/payment-requests  (12,00 EUR = amount_minor 1200)"
CREATED="$(curl -sS -X POST "${BASE}/v1/payment-requests" \
  "${AUTH[@]}" \
  -H "Idempotency-Key: ${IDEMPOTENCY_KEY}" \
  -d "{
    \"merchant_order_id\": \"${ORDER}\",
    \"amount_minor\": 1200,
    \"currency\": \"EUR\",
    \"description\": \"Espresso + cornetto\",
    \"allowed_methods\": [\"ACCOUNT_TO_ACCOUNT\"],
    \"expires_in_seconds\": 300
  }")"
echo "${CREATED}"
echo

PAY_ID="$(printf '%s' "${CREATED}" | python -c 'import json,sys; print(json.load(sys.stdin)["id"])' 2>/dev/null || true)"
if [ -z "${PAY_ID:-}" ]; then
  echo "Could not parse payment id; install python or copy id from JSON above."
  exit 0
fi

echo "# GET /v1/payment-requests/${PAY_ID}"
curl -sS "${BASE}/v1/payment-requests/${PAY_ID}" -H "Authorization: Bearer ${KEY}"
echo

echo "# GET /v1/payment-requests/${PAY_ID}/attempts"
curl -sS "${BASE}/v1/payment-requests/${PAY_ID}/attempts" -H "Authorization: Bearer ${KEY}"
echo

echo "# GET /v1/payment-requests/${PAY_ID}/events"
curl -sS "${BASE}/v1/payment-requests/${PAY_ID}/events" -H "Authorization: Bearer ${KEY}"
echo

echo "# POST /v1/auth/login (admin JWT — dashboard /admin)"
curl -sS -X POST "${BASE}/v1/auth/login" \
  -H "content-type: application/json" \
  -d '{"email":"admin@demo.openpay.local","password":"ChangeMeNow_OpenPayDemo1"}'
echo

echo "# Replay same Idempotency-Key → HTTP 200 replayed:true"
curl -sS -o /tmp/openpay_replay.json -w "HTTP %{http_code}\n" -X POST "${BASE}/v1/payment-requests" \
  "${AUTH[@]}" \
  -H "Idempotency-Key: ${IDEMPOTENCY_KEY}" \
  -d "{
    \"merchant_order_id\": \"${ORDER}\",
    \"amount_minor\": 1200,
    \"currency\": \"EUR\",
    \"allowed_methods\": [\"ACCOUNT_TO_ACCOUNT\"],
    \"expires_in_seconds\": 300
  }"
cat /tmp/openpay_replay.json
echo

# Optional: set CANCEL=1 to cancel the pending payment
if [ "${CANCEL:-}" = "1" ]; then
  echo "# POST .../cancel"
  curl -sS -X POST "${BASE}/v1/payment-requests/${PAY_ID}/cancel" -H "Authorization: Bearer ${KEY}"
  echo
fi
