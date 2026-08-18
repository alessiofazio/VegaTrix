# Security policy

**NOT A PRODUCTION PAYMENT SYSTEM.** v1 is sandbox orchestration software.

## Reporting a vulnerability

Email security reports to the maintainers privately. Do not open public issues for exploitable defects that could affect deployments.

We aim to acknowledge reports within 5 business days.

## Scope notes

- Do not send real card data, PAN, CVV, or live bank credentials to any environment.
- Secrets belong in environment variables, Docker secrets, or a secret manager — never in git.
- Webhook URLs are SSRF-checked; do not disable allowlists in production without review.

## Residual risks

See `docs/security/THREAT-MODEL.md`. PSD2, PCI DSS, GDPR, and AML/KYC compliance are **not** automatic.
