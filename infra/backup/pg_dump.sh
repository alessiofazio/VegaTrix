#!/usr/bin/env sh
# PostgreSQL logical backup for OpenPay.
# Usage:
#   DATABASE_URL=postgresql://user:pass@host:5432/openpay ./infra/backup/pg_dump.sh
#   ./infra/backup/pg_dump.sh postgresql://user:pass@host:5432/openpay
set -eu

URL="${1:-${DATABASE_URL:-}}"
OUT_DIR="${BACKUP_DIR:-./backups}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"

if [ -z "$URL" ]; then
  echo "Set DATABASE_URL or pass a postgres URL as the first argument." >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
FILE="${OUT_DIR}/openpay-${STAMP}.sql.gz"
echo "Writing ${FILE}"
pg_dump --no-owner --format=plain "$URL" | gzip -c > "$FILE"
echo "Done. Restore with: gunzip -c ${FILE} | psql \"\$DATABASE_URL\""
