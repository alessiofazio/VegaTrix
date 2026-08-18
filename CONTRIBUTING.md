# Contributing

Thank you for helping improve OpenPay Protocol. Issues and pull requests are welcome on GitHub.

## Before you start

- Search [existing issues](https://github.com/alessiofazio/VegaTrix/issues) to avoid duplicates.
- For security vulnerabilities, follow [`SECURITY.md`](SECURITY.md) — do not open public issues for exploitable defects.

## Development guidelines

1. Keep **business logic in Rust crates**, not in TypeScript UIs or SDKs.
2. Never add float money types.
3. Never log raw payment payloads, tokens, or secrets.
4. New connectors implement `PaymentConnector` and ship contract tests.
5. State transitions go through the domain state machine.
6. Run `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` before opening a PR.
7. Dashboard/demo changes: `npm run typecheck` in the relevant `web/*` app.

## Pull requests

1. Fork the repo and create a feature branch from `main`.
2. Keep PRs focused — one logical change per PR when possible.
3. Update docs when you change behavior or public APIs.
4. CI must pass (Rust fmt/clippy/test, web typecheck, Docker build).

## License

By contributing, you agree that your contributions are licensed under the [Apache License 2.0](LICENSE). No separate DCO or CLA is required — inbound contributions use the same license as the project (inbound = outbound).

## Code of conduct

Be respectful and constructive. Harassment and bad-faith behavior are not tolerated. Maintainers may close issues or PRs that do not follow these expectations.
