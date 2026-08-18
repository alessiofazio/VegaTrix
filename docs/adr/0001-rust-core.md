# ADR 0001 — Rust core instead of TypeScript

## Status

Accepted (replaces the original TypeScript/Nest proposal).

## Context

The orchestration layer mutates financial state, must be idempotent under retries, and is exposed on a network boundary.

## Decision

Implement API, worker, connectors, routing, crypto, and persistence in **Rust stable** (Axum + Tokio + SQLx). Keep TypeScript for UIs and generated clients.

## Consequences

Steeper contributor onboarding; stronger memory safety and operational predictability. Windows hosts need an MSVC (or GNU) linker, or they build via Docker.
