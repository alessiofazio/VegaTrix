# Contributing

Thank you for helping with OpenPay Protocol.

1. Keep **business logic in Rust crates**, not in TypeScript UIs or SDKs.
2. Never add float money types.
3. Never log raw payment payloads, tokens, or secrets.
4. New connectors implement `PaymentConnector` and ship contract tests.
5. State transitions go through the domain state machine.
6. Run `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` before opening a PR.
7. Dashboard/demo changes: `npm run typecheck` in the relevant `web/*` app.

By contributing you agree the work is licensed under the project’s draft Sustainable Use License until counsel replaces it.
