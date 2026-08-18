#Requires -Version 5.1
<#
.SYNOPSIS
  Start the OpenPay sandbox via Docker Compose on Windows.

.DESCRIPTION
  Checks that Docker is installed and running, creates .env from .env.example
  when missing, then runs docker compose up --build from the repo root.
#>
$ErrorActionPreference = "Stop"

function Assert-Docker {
    if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
        Write-Error @"
Docker was not found. Install Docker Desktop for Windows:
  https://docs.docker.com/desktop/setup/install/windows-install/
"@
    }

    docker info 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Docker daemon is not running. Start Docker Desktop and retry."
    }
}

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Set-Location $RepoRoot

Assert-Docker

$EnvFile = Join-Path $RepoRoot ".env"
$EnvExample = Join-Path $RepoRoot ".env.example"
if (-not (Test-Path $EnvFile)) {
    if (-not (Test-Path $EnvExample)) {
        Write-Error ".env.example is missing; cannot create .env"
    }
    Copy-Item $EnvExample $EnvFile
    Write-Host "Created .env from .env.example"
}

Write-Host "Starting OpenPay sandbox (docker compose up --build) ..."
docker compose up --build
