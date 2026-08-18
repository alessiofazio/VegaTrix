# ADR 0004 — Transactional outbox

Accepted. Payment mutations and outbox rows share a commit. A Tokio worker delivers webhooks later. Kafka/NATS can replace the publisher later without changing the domain.
