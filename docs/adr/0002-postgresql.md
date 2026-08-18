# ADR 0002 — PostgreSQL as production database

Accepted. ACID transactions, row locks, and unique indexes are required for idempotency and outbox. SQLite is a non-production `sqlite-demo` option only.
