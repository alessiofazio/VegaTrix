# ADR 0005 — Short-lived HMAC QR tokens

Accepted. The QR carries only `openpay://v1/pay/{id}?token=...`. Amount and merchant are enforced server-side. HMAC-SHA256 with a rotatable secret is enough for v1; asymmetric keys can follow.
