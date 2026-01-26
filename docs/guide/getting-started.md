# Getting Started

Get up and running with Mocktioneer quickly.

## Prerequisites

Before you begin, ensure you have:

- **Rust** (stable toolchain)
- **Fastly CLI** (optional, for Fastly adapter): `brew install fastly/tap/fastly`
- **Wrangler** (optional, for Cloudflare adapter): `npm install -g wrangler`
- **WASM targets** (for edge adapters):
  - Fastly: `rustup target add wasm32-wasip1`
  - Cloudflare: `rustup target add wasm32-unknown-unknown`

## Installation

### Clone the Repository

```bash
git clone https://github.com/stackpop/mocktioneer.git
cd mocktioneer
```

### Install EdgeZero CLI (Optional)

The EdgeZero CLI provides a unified interface for all adapters:

```bash
cargo install --path edgezero/crates/edgezero-cli --features cli
```

Or run it directly without installing:

```bash
cargo run --manifest-path edgezero/Cargo.toml -p edgezero-cli --features cli -- --help
```

## Running Locally

### Option 1: Native Axum Server (Recommended for Development)

The fastest way to get started:

```bash
cargo run -p mocktioneer-adapter-axum
```

The server starts at `http://127.0.0.1:8787`.

### Option 2: Using EdgeZero CLI

```bash
edgezero-cli serve --adapter axum
```

### Option 3: Fastly Local Development

```bash
edgezero-cli serve --adapter fastly
# Or directly:
fastly compute serve -C crates/mocktioneer-adapter-fastly
```

### Option 4: Cloudflare Local Development

```bash
edgezero-cli serve --adapter cloudflare
# Or directly:
wrangler dev --config crates/mocktioneer-adapter-cloudflare/wrangler.toml
```

## Verify Installation

### Test the Root Endpoint

```bash
curl http://127.0.0.1:8787/
```

You should see an HTML info page.

### Test an OpenRTB Auction

```bash
curl -X POST http://127.0.0.1:8787/openrtb2/auction \
  -H 'Content-Type: application/json' \
  -d '{
    "id": "test-request",
    "imp": [{
      "id": "imp-1",
      "banner": {"w": 300, "h": 250}
    }]
  }' | jq .
```

You should receive an OpenRTB bid response with a creative URL.

### Test APS TAM Endpoint

```bash
curl -X POST http://127.0.0.1:8787/e/dtb/bid \
  -H 'Content-Type: application/json' \
  -d '{
    "pubId": "1234",
    "slots": [{
      "slotID": "header",
      "slotName": "header",
      "sizes": [[728, 90]]
    }]
  }' | jq .
```

## Using Example Scripts

The `examples/` directory contains helper scripts:

```bash
# OpenRTB auction
./examples/openrtb_request.sh

# APS TAM bid
./examples/aps_request.sh

# Fetch creative iframe
./examples/iframe_request.sh 300x250

# Test pixel endpoint
./examples/pixel_request.sh
```

Override the host with `MOCKTIONEER_BASE_URL`:

```bash
MOCKTIONEER_BASE_URL=https://mocktioneer.edgecompute.app ./examples/openrtb_request.sh
```

## Run Tests

```bash
cargo test
```

## Next Steps

- Learn about [configuration](./configuration) options
- Understand the [architecture](./architecture)
- Explore [adapter-specific deployment](./adapters/)
- Check the [API reference](/api/) for all endpoints
