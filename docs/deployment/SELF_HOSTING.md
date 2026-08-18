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

`/healthz` is liveness (process up). `/readyz` returns **HTTP 503** when the database is down.

## External PostgreSQL / Redis

Set `DATABASE_URL` and `REDIS_URL`. Run `openpay-cli migrate` then `openpay-cli seed` only for demo data.

## Backup / restore

Backup PostgreSQL with `infra/backup/pg_dump.sh` or `infra/backup/pg_dump.ps1` (or raw `pg_dump`). Restore with `psql`. Redis is cache/rate-limit, not the ledger. After restore, run migrations if the schema version differs.

## Secrets

Use Docker secrets or a manager in production. Rotate `JWT_*`, `QR_SIGNING_SECRET`, `WEBHOOK_SIGNING_SECRET` independently. API keys are stored hashed.

## TLS

Terminate TLS at a reverse proxy (Caddy, nginx, Traefik). Set `API_BASE_URL` and `CORS_ALLOW_ORIGINS` to HTTPS origins. CORS is closed unless allowlisted.

## Health

- API liveness: `GET /healthz` (process up)
- API readiness: `GET /readyz` (HTTP 503 if Postgres is down)
- Postgres/Redis: compose healthchecks
- Prometheus: `infra/prometheus/alerts.yml` when `TELEMETRY_OPT_IN=true` (scrape `METRICS_BIND_ADDR`, default `:9090`)

## Scaling

Run N API replicas behind the proxy. Run **one or few** workers; outbox uses `published_at` to avoid double fan-out. Partitioning event tables is a future option.

## Production checklist

- [ ] `APP_ENV=production` (no demo seed, mock connector off)
- [ ] Secrets rotated off `.env.example` / `replace_me_*` values
- [ ] `ENCRYPTION_MASTER_KEY` set (32 bytes or Base64 of 32 bytes)
- [ ] PostgreSQL with backups (`infra/backup/`) and TLS
- [ ] Webhook allowlist (hostnames only; private IPs still blocked unless Docker hostname is allowlisted)
- [ ] Reverse proxy TLS; public URLs `https://`
- [ ] `/readyz` wired as the orchestrator readiness probe
- [ ] Legal review of license
- [ ] No claim of PCI/PSD2 compliance without assessment
- [ ] Live connectors only with authorized providers (not in this repo)
