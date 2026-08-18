<div align="center">

<img src="docs/assets/openpay-logo.svg" alt="OpenPay Protocol" width="96" height="96" />

# OpenPay Protocol

### Payment orchestration (open-core sandbox) — not a bank or PSP

**_One API to create, route, observe, and reconcile payments on existing rails._**

[![CI](https://github.com/alessiofazio/VegaTrix/actions/workflows/ci.yml/badge.svg)](https://github.com/alessiofazio/VegaTrix/actions/workflows/ci.yml)
[![GitHub stars](https://img.shields.io/github/stars/alessiofazio/VegaTrix?style=flat-square&logo=github)](https://github.com/alessiofazio/VegaTrix/stargazers)
[![GitHub forks](https://img.shields.io/github/forks/alessiofazio/VegaTrix?style=flat-square&logo=github)](https://github.com/alessiofazio/VegaTrix/forks)
[![GitHub issues](https://img.shields.io/github/issues/alessiofazio/VegaTrix?style=flat-square&logo=github)](https://github.com/alessiofazio/VegaTrix/issues)
[![License](https://img.shields.io/github/license/alessiofazio/VegaTrix?style=flat-square)](LICENSE)
[![Last commit](https://img.shields.io/github/last-commit/alessiofazio/VegaTrix?style=flat-square&logo=github)](https://github.com/alessiofazio/VegaTrix/commits/main)

[![Rust](https://img.shields.io/badge/Rust-stable-orange?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Docker](https://img.shields.io/badge/Docker-Compose-2496ED?style=flat-square&logo=docker&logoColor=white)](https://docs.docker.com/get-docker/)
[![PostgreSQL](https://img.shields.io/badge/PostgreSQL-16-4169E1?style=flat-square&logo=postgresql&logoColor=white)](https://www.postgresql.org/)
[![Axum](https://img.shields.io/badge/Axum-HTTP-000000?style=flat-square&logo=rust&logoColor=white)](https://github.com/tokio-rs/axum)
[![Sandbox](https://img.shields.io/badge/v1-sandbox-yellow?style=flat-square)](#what-it-does-not-do)

</div>

> **Not a bank, PSP, card issuer, or custodian.** v1 is a **sandbox**. It does not move real money.

---

## Get started

No `.exe` or `.dmg` installers — run the same Docker Compose stack on every OS.

<p align="center">
  <a href="#docker"><img src="https://img.shields.io/badge/🐳%20Docker-Get%20started-2496ED?style=for-the-badge&logo=docker&logoColor=white" alt="Docker"></a>
  &nbsp;
  <a href="#windows"><img src="https://img.shields.io/badge/🪟%20Windows-Get%20started-0078D4?style=for-the-badge&logo=windows&logoColor=white" alt="Windows"></a>
  &nbsp;
  <a href="#macos"><img src="https://img.shields.io/badge/🍎%20macOS-Get%20started-555555?style=for-the-badge&logo=apple&logoColor=white" alt="macOS"></a>
  &nbsp;
  <a href="#linux"><img src="https://img.shields.io/badge/🐧%20Linux-Get%20started-FCC624?style=for-the-badge&logo=linux&logoColor=black" alt="Linux"></a>
</p>

<p align="center"><em>Docker · Windows · macOS · Linux — same Compose stack</em></p>

### <a id="docker"></a>Docker (recommended)

1. Install [Docker Desktop](https://docs.docker.com/get-docker/) or Docker Engine + Compose.
2. Clone and start:

```bash
git clone https://github.com/alessiofazio/VegaTrix.git
cd VegaTrix
cp .env.example .env
docker compose up --build
```

### <a id="windows"></a>Windows

1. Install [Docker Desktop for Windows](https://docs.docker.com/desktop/setup/install/windows-install/).
2. Clone the repo (PowerShell):

```powershell
git clone https://github.com/alessiofazio/VegaTrix.git
cd VegaTrix
Copy-Item .env.example .env
.\scripts\windows\dev-up.ps1
```

Or run `docker compose up --build` manually after Docker Desktop is running.

### <a id="macos"></a>macOS

1. Install [Docker Desktop for Mac](https://docs.docker.com/desktop/setup/install/mac-install/).
2. Clone and start:

```bash
git clone https://github.com/alessiofazio/VegaTrix.git
cd VegaTrix
cp .env.example .env
docker compose up --build
```

### <a id="linux"></a>Linux

**Option A — Docker Compose (recommended)**

1. Install [Docker Engine](https://docs.docker.com/engine/install/) and the [Compose plugin](https://docs.docker.com/compose/install/).
2. Clone and start:

```bash
git clone https://github.com/alessiofazio/VegaTrix.git
cd VegaTrix
cp .env.example .env
./scripts/linux/dev-up.sh
```

Or run `docker compose up --build` manually. No `.AppImage` or `.deb` — the stack runs from source via Compose.

**Option B — native Rust (without Docker)**

Requires Rust stable, a C linker, and local PostgreSQL + Redis:

```bash
git clone https://github.com/alessiofazio/VegaTrix.git
cd VegaTrix
cp .env.example .env
cargo test --workspace
cargo run -p openpay-server
# in another terminal:
cargo run -p openpay-worker-bin
```

See [Local Rust (without Docker)](#local-rust-without-docker) for details.

### Localhost URLs

| Surface | URL |
|---|---|
| API | http://localhost:8080 |
| OpenAPI | http://localhost:8080/docs |
| Dashboard | http://localhost:3001 |
| Demo merchant / POS | http://localhost:3002 |
| Demo wallet | http://localhost:3003 |

Demo login: `admin@demo.openpay.local` / `ChangeMeNow_OpenPayDemo1`  
Demo merchant API key: `opk_demo_merchant_sandbox_not_for_production_use_only`

---

## Positioning (honest)

| | OpenPay Protocol | Traditional PSP | Direct Stripe/Nexi SDK |
|---|---|---|---|
| Role | Orchestration layer you self-host | Licensed payment institution | Single-rail integration |
| Holds funds | No | Often yes | Via provider |
| Multi-connector routing | Yes (plugin trait) | Provider-specific | One provider per integration |
| v1 live rails | Mock / manual / stub only | Production | Production (with contract) |
| Compliance claim | **None** — sandbox software | Provider-dependent | Provider-dependent |

OpenPay sits **between** your POS/e-commerce and external rails: state machine, webhooks, routing — not a replacement for a bank contract.

---

## Documentation

| Guide | Path |
|---|---|
| Merchant + public + admin + auth API | [`docs/api/API-GUIDE.md`](docs/api/API-GUIDE.md) |
| Implement merchants & connectors | [`docs/protocol/IMPLEMENTING.md`](docs/protocol/IMPLEMENTING.md) |
| cURL / TypeScript / Python / Go / PHP / Java / C# | [`docs/sdk/MULTI-LANGUAGE.md`](docs/sdk/MULTI-LANGUAGE.md) |
| Architecture | [`docs/architecture/ARCHITECTURE.md`](docs/architecture/ARCHITECTURE.md) |
| Contributing | [`CONTRIBUTING.md`](CONTRIBUTING.md) |
| Security | [`SECURITY.md`](SECURITY.md) |

Examples: [`sdk/typescript`](sdk/typescript), [`sdk/python/openpay_example.py`](sdk/python/openpay_example.py), [`sdk/curl/examples.sh`](sdk/curl/examples.sh).

---

## What it does

- Payment Request + QR / payment link
- Strict state machine with audit trail
- Connector plugins (mock instant, manual test, open-banking stub)
- Configurable routing with bounded fallback to the next enabled connector
- Signed merchant webhooks via outbox + worker
- Self-hosted Docker Compose

## What it does not do

- Hold funds, run KYC/AML, issue cards, or settle
- Store PAN/CVV/bank credentials
- Provide live Nexi/Stripe/Visa/SEPA integrations in v1
- Claim PCI, PSD2, or GDPR certification

---

## Architecture

```text
HTTP / wallet / CLI
        ↓
openpay-api (Axum)
        ↓
openpay-application (use cases, routing, idempotency)
        ↓
openpay-domain (state machine)
        ↓
ports (traits)
        ↓
PostgreSQL + Redis + connector adapters + worker
```

Core is **Rust** (Axum, Tokio, SQLx). TypeScript powers the dashboard, demo UIs, and a thin SDK wrapper. JSON APIs use **snake_case**. Money is **integer minor units**. IDs are **prefixed UUIDv7**.

---

## Demo flow

1. Open the merchant demo and create a **12,00 EUR** order.
2. Open the wallet link / QR.
3. Approve or reject.
4. Watch the till move to `SETTLED` or `FAILED`.
5. Inspect the timeline on the dashboard.

---

## Local Rust (without Docker)

<a id="local-rust-without-docker"></a>

Requires Rust stable **and** a C linker (MSVC Build Tools on Windows, or compile inside Docker).

```bash
cargo test --workspace
cargo run -p openpay-server
cargo run -p openpay-worker-bin
cargo run -p openpay-cli -- seed
```

PostgreSQL and Redis must be running. See `.env.example`.

---

## Production

For a prod-like self-hosted stack:

```bash
cp .env.production.example .env.production
docker compose -f docker-compose.prod.yml --env-file .env.production up --build
```

See [`docs/deployment/SELF_HOSTING.md`](docs/deployment/SELF_HOSTING.md). Still **not** a live PSP.

---

## Contributing

Issues and pull requests are welcome on [GitHub](https://github.com/alessiofazio/VegaTrix/issues). Read [`CONTRIBUTING.md`](CONTRIBUTING.md) before opening a PR. No separate CLA — contributions are licensed under [Apache-2.0](LICENSE) (inbound = outbound).

---

## License

[Apache License 2.0](LICENSE). A previous draft Sustainable Use License was superseded by Apache-2.0 at the author's request.
