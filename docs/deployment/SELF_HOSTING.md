# Self-hosting

## Demo hardware

- 2 vCPU, 4 GB RAM is enough for the sandbox compose stack.

## Docker Compose

```bash
cp .env.example .env
docker compose up --build
```

Replace every `replace_me_*` secret before any network that is not loopback.

## External PostgreSQL / Redis

Set `DATABASE_URL` and `REDIS_URL`. Run `openpay-cli migrate` then `openpay-cli seed` only for demo data.

## Backup / restore

Backup PostgreSQL with `pg_dump`. Restore with `psql`. Redis is cache/rate-limit, not the ledger. After restore, run migrations if the schema version differs.

## Secrets

Use Docker secrets or a manager in production. Rotate `JWT_*`, `QR_SIGNING_SECRET`, `WEBHOOK_SIGNING_SECRET` independently. API keys are stored hashed.

## TLS

Terminate TLS at a reverse proxy (Caddy, nginx, Traefik). Set `API_BASE_URL` and `CORS_ALLOW_ORIGINS` to HTTPS origins. CORS is closed unless allowlisted.

## Health

- API: `GET /healthz`, `GET /readyz`
- Postgres/Redis: compose healthchecks

## Scaling

Run N API replicas behind the proxy. Run **one or few** workers; outbox uses `published_at` to avoid double fan-out. Partitioning event tables is a future option.

## Production checklist

- [ ] Secrets rotated off `.env.example` values
- [ ] PostgreSQL with backups and TLS
- [ ] Webhook allowlist (no open SSRF)
- [ ] Reverse proxy TLS
- [ ] No demo seed in production
- [ ] Legal review of license
- [ ] No claim of PCI/PSD2 compliance without assessment
- [ ] Live connectors only with authorized providers
