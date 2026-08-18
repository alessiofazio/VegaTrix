"""Minimal OpenPay merchant HTTP example.

SANDBOX ONLY — demo API key and localhost:8080. Not for production.

Requires: pip install httpx
  (requests works the same: swap httpx for requests)

Usage:
  python sdk/python/openpay_example.py
  python sdk/python/openpay_example.py --poll
"""

from __future__ import annotations

import argparse
import hashlib
import hmac
import time
import uuid

import httpx

# SANDBOX ONLY
BASE = "http://localhost:8080"
KEY = "opk_demo_merchant_sandbox_not_for_production_use_only"


def merchant_headers(idempotency_key: str | None = None) -> dict[str, str]:
    headers = {
        "Authorization": f"Bearer {KEY}",
        "content-type": "application/json",
    }
    if idempotency_key:
        headers["Idempotency-Key"] = idempotency_key
    return headers


def create_payment() -> dict:
    body = {
        "merchant_order_id": f"ORD-PY-{int(time.time())}",
        "amount_minor": 1200,
        "currency": "EUR",
        "description": "Espresso + cornetto",
        "allowed_methods": ["ACCOUNT_TO_ACCOUNT"],
        "expires_in_seconds": 300,
    }
    response = httpx.post(
        f"{BASE}/v1/payment-requests",
        headers=merchant_headers(str(uuid.uuid4())),
        json=body,
        timeout=30.0,
    )
    if response.status_code not in (200, 201):
        raise SystemExit(f"OpenPay error {response.status_code}: {response.text}")
    return response.json()


def get_payment(payment_id: str) -> dict:
    response = httpx.get(
        f"{BASE}/v1/payment-requests/{payment_id}",
        headers=merchant_headers(),
        timeout=30.0,
    )
    if not response.is_success:
        raise SystemExit(f"OpenPay error {response.status_code}: {response.text}")
    return response.json()


def verify_openpay_signature(
    secret: str, header: str, raw_body: bytes, tolerance_secs: int = 300
) -> bool:
    """Verify OpenPay-Signature: t=<unix>,v1=<hex> over `{t}.{raw_body}`."""
    parts = dict(p.strip().split("=", 1) for p in header.split(",") if "=" in p)
    t, v1 = parts.get("t"), parts.get("v1")
    if not t or not v1:
        return False
    if abs(int(time.time()) - int(t)) > tolerance_secs:
        return False
    expected = hmac.new(
        secret.encode("utf-8"),
        f"{t}.".encode("ascii") + raw_body,
        hashlib.sha256,
    ).hexdigest()
    return hmac.compare_digest(expected, v1)


def main() -> None:
    parser = argparse.ArgumentParser(description="OpenPay sandbox example")
    parser.add_argument("--poll", action="store_true", help="poll until terminal status")
    args = parser.parse_args()

    health = httpx.get(f"{BASE}/healthz", timeout=5.0)
    print("healthz", health.json())

    created = create_payment()
    print("created", created["id"], created["status"], created["amount_minor"], created["currency"])
    print("qr_payload", created.get("qr_payload"))

    if args.poll:
        terminal = {"SETTLED", "FAILED", "CANCELLED", "EXPIRED", "REFUNDED"}
        payment_id = created["id"]
        for _ in range(20):
            latest = get_payment(payment_id)
            print("poll", latest["status"])
            if latest["status"] in terminal:
                break
            time.sleep(1.5)


if __name__ == "__main__":
    main()
