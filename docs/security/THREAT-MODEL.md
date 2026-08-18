# Threat model (v1 sandbox)

| Threat | Mitigation in v1 | Residual risk |
|---|---|---|
| Duplicate payment / double charge | Idempotency key + fingerprint; attempt unique provider ref; no auto-retry on non-idempotent connector calls | Mock connector is not a real PSP guarantee |
| Webhook spoofing | `OpenPay-Signature` HMAC over `timestamp.body`; tolerance window | Secret leakage bypasses it |
| QR replay | Short-lived HMAC token bound to payment/tenant/merchant; nonce table | Stolen token valid until expiry if unused |
| API key leakage | Keys hashed (Argon2id); fingerprint lookup; shown once in seed docs | Demo key is public by design |
| Cross-tenant leakage | `tenant_id` on queries and auth context | Bugs in new queries |
| Connector compromise | Secrets by reference, encrypted at rest as `enc:v1:` when a master key is set; sandbox_only flags; normalized errors | A future live connector enlarges blast radius |
| Delayed callback | Duplicate callbacks map to same state; version lock | Ambiguous timeout stays `PROCESSING` until reconcile |
| Ambiguous outcome | `AttemptStatus::Ambiguous` → payment `PROCESSING` | Operator must reconcile |
| Privilege escalation | RBAC admin vs merchant API key | Demo admin password is documented |
| SSRF via webhook URL | Private/link-local/metadata blocked; allowlist; no redirects | Misconfigured allowlist |
| Secrets in logs | Structured tracing; redacted messages; no raw payloads | Mis-added debug logs |

v1 is **not** PCI DSS, PSD2, GDPR, or AML certified.
