# Self-hosting

## Demo hardware

- 2 vCPU, 4 GB RAM is enough for the sandbox compose stack.

## Docker Compose

```bash
cp .env.example .env
docker compose up --build
```

Replace every `replace_me_*` secret before any network that is not loopback. `APP_ENV=development` keeps the current demo behavior (auto-seed, mock connector).

## Production compose

```bash
cp .env.production.example .env.production
# set real secrets, https:// API/dashboard/wallet URLs, WEBHOOK_URL_ALLOWLIST hostnames
docker compose -f docker-compose.prod.yml --env-file .env.production up --build
```

That file starts **postgres, redis, server, worker only** — no dashboard, demo wallet, demo merchant, and no seed. `APP_ENV=production` refuses to boot if:

- JWT / QR / webhook / encryption secrets are empty, too short, or contain `replace_me`
- `FEATURE_CONNECTOR_MOCK=true`
- `WEBHOOK_URL_ALLOWLIST` is empty
- `API_BASE_URL` / `APP_BASE_URL` / `DASHBOARD_BASE_URL` / `WALLET_BASE_URL` are not `https://`
- `--seed` or `openpay-cli seed` is used

`/healthz` is liveness (process up). `/readyz` returns **HTTP 503** when the database is down. Docker `HEALTHCHECK` and k8s **readinessProbe** should hit `/readyz`. Use `/healthz` only as **livenessProbe**.

Production compose also starts **Caddy** on `:80` / `:443` terminating TLS with Caddy's internal CA (browsers warn; this is not a public certificate). See `infra/caddy/README.md`. Direct `:8080` remains mapped for debug.

## External PostgreSQL / Redis

Set `DATABASE_URL` and `REDIS_URL`. Run `openpay-cli migrate` then `openpay-cli seed` only for demo data.

## Backup / restore

Backup PostgreSQL with `infra/backup/pg_dump.sh` or `infra/backup/pg_dump.ps1` (or raw `pg_dump`). Restore with `psql`. Redis is cache/rate-limit, not the ledger. After restore, run migrations if the schema version differs.

## Secrets

Use Docker secrets or a manager in production. Rotate `JWT_*`, `QR_SIGNING_SECRET`, `WEBHOOK_SIGNING_SECRET` independently. API keys are stored hashed.

## TLS

Production compose includes Caddy (`infra/caddy/Caddyfile`) with `tls internal`. That is a **local CA**, not a certificate from Let's Encrypt or another public CA. Browsers will warn until you replace the Caddyfile `tls` line with your own certs in `infra/caddy/certs/`. Set `API_BASE_URL` and `CORS_ALLOW_ORIGINS` to HTTPS origins. CORS is closed unless allowlisted.

## Health

- API liveness: `GET /healthz` (process up) — k8s `livenessProbe`
- API readiness: `GET /readyz` (HTTP 503 if Postgres is down) — k8s `readinessProbe` and Docker HEALTHCHECK
- Worker: same paths on `WORKER_BIND_ADDR` (default `:8081`)
- Postgres/Redis: compose healthchecks
- Prometheus: `infra/prometheus/alerts.yml` when `TELEMETRY_OPT_IN=true` (scrape `METRICS_BIND_ADDR`, default `:9090`)

Merchant `/v1` rate limits are per API key fingerprint or JWT tenant when Redis is up (`Retry-After: 60` on HTTP 429). If Redis is down, **production fail-closes** (HTTP 503); development fail-opens. Public wallet/QR routes stay IP-based and fail-open.

## Scaling

Run N API replicas behind the proxy. Run **one or few** workers; outbox uses `published_at` to avoid double fan-out. Partitioning event tables is a future option.

## Production checklist

- [ ] `APP_ENV=production` (no demo seed, mock connector off)
- [ ] Secrets rotated off `.env.example` / `replace_me_*` values
- [ ] `ENCRYPTION_MASTER_KEY` set (32 bytes or Base64 of 32 bytes)
- [ ] PostgreSQL with backups (`infra/backup/`) and TLS
- [ ] Webhook allowlist (hostnames only; private IPs still blocked unless Docker hostname is allowlisted)
- [ ] Reverse proxy TLS; public URLs `https://` (compose Caddy uses an internal CA, not a public cert)
- [ ] `/readyz` wired as the orchestrator readiness probe (`/healthz` for liveness)
- [ ] Understand [Apache-2.0](../../LICENSE) obligations for your deployment
- [ ] No claim of PCI/PSD2 compliance without assessment
- [ ] Live connectors only with authorized providers (not in this repo)
