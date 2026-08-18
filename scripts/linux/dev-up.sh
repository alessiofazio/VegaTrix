#!/usr/bin/env bash
# Start the OpenPay sandbox via Docker Compose on Linux.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

if ! command -v docker >/dev/null 2>&1; then
  echo "Docker was not found. Install Docker Engine:" >&2
  echo "  https://docs.docker.com/engine/install/" >&2
  exit 1
fi

if ! docker info >/dev/null 2>&1; then
  echo "Docker daemon is not running. Start Docker and retry." >&2
  exit 1
fi

if [[ ! -f .env ]]; then
  if [[ ! -f .env.example ]]; then
    echo ".env.example is missing; cannot create .env" >&2
    exit 1
  fi
  cp .env.example .env
  echo "Created .env from .env.example"
fi

echo "Starting OpenPay sandbox (docker compose up --build) ..."
docker compose up --build
