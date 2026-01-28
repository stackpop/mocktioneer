# Playwright E2E Tests

End-to-end tests for Mocktioneer using Playwright.

## Prerequisites

```bash
npm install
npx playwright install
```

**Note:** The Cloudflare adapter requires `edgezero-cli` (not `edgezero`):
```bash
cargo install --git ssh://git@github.com/stackpop/edgezero.git edgezero-cli
```

## Running Tests

### Test Axum Adapter (default)

```bash
npx playwright test
```

### Test Cloudflare Adapter

```bash
ADAPTER=cloudflare npx playwright test
```

### Run Both Adapters

```bash
npx playwright test && ADAPTER=cloudflare npx playwright test
```

## View Test Report

```bash
npx playwright show-report
```

## Configuration

The `ADAPTER` environment variable controls which adapter is tested:

| Value | Command | Description |
|-------|---------|-------------|
| `axum` (default) | `cargo run -p mocktioneer-adapter-axum` | Native Axum server |
| `cloudflare` | `edgezero-cli serve --adapter cloudflare` | Cloudflare Workers (WASM) |

Both adapters run on `http://127.0.0.1:8787`.
