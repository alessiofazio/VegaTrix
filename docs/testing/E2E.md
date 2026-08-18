# Playwright (optional)

Install Playwright and run against a live compose stack:

```bash
npx playwright install
npx playwright test
```

The suite is intentionally thin: create order → wallet page reachable. Full settlement still needs the Rust API.
