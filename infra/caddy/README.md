# Caddy TLS terminator (production compose)

`docker-compose.prod.yml` puts Caddy in front of `openpay-server`.

- HTTP `:80` and HTTPS `:443` reverse-proxy to `server:8080`
- HTTPS uses **Caddy `tls internal`** (a local CA, not a public certificate). Browsers will warn.
- Optional: drop `tls.crt` / `tls.key` in `certs/` and change the Caddyfile `tls` line to those paths. This repo does not buy or issue Let's Encrypt / public CA certs.

Probes:

- Liveness: `GET /healthz` (process up)
- Readiness: `GET /readyz` (Postgres up) — use this in k8s `readinessProbe` and Docker `HEALTHCHECK`
