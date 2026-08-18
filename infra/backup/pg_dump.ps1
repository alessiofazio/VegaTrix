# PostgreSQL logical backup for OpenPay (Windows).
# Usage:
#   $env:DATABASE_URL = "postgresql://openpay:pass@localhost:5432/openpay"
#   .\infra\backup\pg_dump.ps1
# Or dump a compose Postgres container:
#   .\infra\backup\pg_dump.ps1 -ComposeService postgres -ComposeFile docker-compose.prod.yml
param(
    [string]$DatabaseUrl = $env:DATABASE_URL,
    [string]$OutDir = "./backups",
    [string]$ComposeService = "",
    [string]$ComposeFile = "docker-compose.prod.yml",
    [string]$PgUser = "openpay",
    [string]$PgDatabase = "openpay"
)

$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$stamp = (Get-Date).ToUniversalTime().ToString("yyyyMMddTHHmmssZ")
$file = Join-Path $OutDir "openpay-$stamp.sql"

if ($ComposeService) {
    Write-Host "Dumping compose service '$ComposeService' to $file"
    $dump = docker compose -f $ComposeFile exec -T $ComposeService pg_dump -U $PgUser -d $PgDatabase --no-owner
    Set-Content -Path $file -Value $dump -Encoding utf8
} else {
    if (-not $DatabaseUrl) {
        throw "Set DATABASE_URL or pass -DatabaseUrl / -ComposeService"
    }
    Write-Host "Dumping $DatabaseUrl to $file"
    & pg_dump --no-owner --format=plain $DatabaseUrl | Set-Content -Path $file -Encoding utf8
}

Write-Host "Done. Restore with: psql `$env:DATABASE_URL -f $file"
Write-Host "Redis is cache/rate-limit only; do not treat it as the ledger."
