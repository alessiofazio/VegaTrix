# Infra

v1 ships Docker Compose only. Optional k8s/terraform skeletons can live here later.

| Artifact | Purpose |
|---|---|
| `../docker-compose.yml` | Demo stack (dashboard, wallet, merchant, auto-seed) |
| `../docker-compose.prod.yml` | postgres + redis + server + worker, no seed |
| `backup/pg_dump.sh` / `backup/pg_dump.ps1` | PostgreSQL logical backup |
| `prometheus/alerts.yml` | Webhook DLQ, stuck PROCESSING, HTTP 5xx |

Prometheus scrapes `METRICS_BIND_ADDR` (default `:9090`) only when `TELEMETRY_OPT_IN=true`.
