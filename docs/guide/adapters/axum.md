# Axum Adapter

The Axum adapter runs Mocktioneer as a native Rust HTTP server. It's the recommended choice for local development and integration testing.

## Overview

| Property     | Value                       |
| ------------ | --------------------------- |
| Crate        | `mocktioneer-adapter-axum`  |
| Target       | Native (no WASM)            |
| Default Port | 8787                        |
| Use Case     | Development, testing, CI/CD |

## Quick Start

```bash
# One-time: create the local config and push it (auction/APS are fail-loud)
cp mocktioneer.toml.example mocktioneer.toml
cargo run -p mocktioneer-cli -- config push --adapter axum --yes

cargo run -p mocktioneer-adapter-axum
```

The server starts at `http://127.0.0.1:8787`. Axum reads the pushed blob from
`./.edgezero/local-config-mocktioneer_config.json` **once at startup**, so
re-run `config push` and restart after changing `bid_cpm`. (Static/pixel/sizes
endpoints work without a pushed config.)

## Using the CLI

```bash
edgezero-cli serve --adapter axum
# or, in-repo (no external install):
cargo run -p mocktioneer-cli -- serve --adapter axum
```

This executes the command defined in `edgezero.toml`:

```toml
[adapters.axum.commands]
serve = "cargo run -p mocktioneer-adapter-axum"
```

## Configuration

### Build Settings

```toml
[adapters.axum.build]
target = "native"
profile = "dev"
```

The Axum adapter uses:

- Native compilation (not WASM)
- Dev profile for faster builds during development

### Logging

```toml
[adapters.axum.logging]
level = "info"
echo_stdout = true
```

Logs are written to stdout. Adjust `level` for more or less verbosity:

- `trace` - Most verbose
- `debug` - Debug information
- `info` - Normal operation (default)
- `warn` - Warnings only
- `error` - Errors only

## HTTP Transport

Axum serves **HTTP/1 only**, admits at most 256 connections, and closes a
connection without a response after a 10-second head-read or idle timeout.
Hyper applies raw head limits before dispatch. EdgeZero then checks the visible
request target (8192 bytes), normalized header count (100), and normalized header
size (64 KiB); failures return 414 or 431 without CORS headers.

These normalized checks do not guarantee rejection of every ambiguous raw head.
Hyper can merge identical duplicate `Content-Length` fields and remove
`Content-Length` when `Transfer-Encoding` is present. Raw ingress framing
validation is Unsupported. JSON is accepted without `Content-Type` and with
`text/plain`; `application/json` remains recommended.

Body-read deadlines are Native on Axum: 10 seconds for the three POST bid routes,
30 seconds by default. Observed expiry returns 408. Config extraction has a
BestEffort two-second deadline; observed expiry returns 503 `service_unavailable`
without `Retry-After`. See [API limits and config errors](/api/#request-limits-and-deadlines).

## Docker Deployment

The repository ships a multi-stage `Dockerfile` (also built and pushed to
`ghcr.io/stackpop/mocktioneer` by CI). The build **seeds the default typed-config
blob** into `/app/.edgezero/`, so the OpenRTB/APS endpoints work out of the box
(they read `bid_cpm` via the fail-loud `AppConfig` extractor and would otherwise
503 until a blob is pushed):

```bash
# Build the image
docker build -t mocktioneer:latest .

# Run on default port
docker run -p 8787:8787 mocktioneer:latest

# Run on custom port
docker run -p 3000:8787 mocktioneer:latest
```

To override the baked-in config, mount your own blob over the seeded file:

```bash
docker run -p 8787:8787 \
  -v "$PWD/.edgezero/local-config-mocktioneer_config.json:/app/.edgezero/local-config-mocktioneer_config.json:ro" \
  mocktioneer:latest
```

After updating that mounted blob with `config push`, restart the container to
load the new snapshot. This startup-snapshot behavior is unchanged.

## Development Workflow

### Hot Reloading

For automatic rebuilds during development, use `cargo-watch`:

```bash
cargo install cargo-watch
cargo watch -x 'run -p mocktioneer-adapter-axum'
```

### Running Tests

```bash
# All tests
cargo test

# Core tests only
cargo test -p mocktioneer-core

# Specific test
cargo test handle_openrtb_auction
```

### Debugging

Standard Rust debugging works with the Axum adapter:

```bash
# With RUST_BACKTRACE
RUST_BACKTRACE=1 cargo run -p mocktioneer-adapter-axum

# With debug logging
RUST_LOG=debug cargo run -p mocktioneer-adapter-axum
```

## Performance Testing

The Axum adapter is suitable for local performance testing:

```bash
# Using wrk
wrk -t4 -c100 -d30s http://127.0.0.1:8787/

# Using hey
hey -n 10000 -c 100 http://127.0.0.1:8787/openrtb2/auction \
  -m POST \
  -H "Content-Type: application/json" \
  -d '{"id":"test","imp":[{"id":"1","banner":{"w":300,"h":250}}]}'
```

## Differences from Edge Adapters

The adapters share handlers, with different transport and deadline capabilities
listed in the [capability table](./#capability-support):

| Feature      | Axum          | Edge (Fastly/Cloudflare) |
| ------------ | ------------- | ------------------------ |
| Startup time | Instant       | Cold start possible      |
| Debugging    | Full access   | Limited                  |
| KV stores    | Not available | Platform-specific        |
| Logging      | stdout        | Platform logging         |

## Customization

To customize the Axum adapter behavior, edit:

- `crates/mocktioneer-adapter-axum/src/main.rs` - Entrypoint
- `crates/mocktioneer-adapter-axum/axum.toml` - Adapter-specific config

Example: changing the listen address would require modifying the adapter code.

## Next Steps

- Try the [Fastly adapter](./fastly) for edge deployment
- Try the [Cloudflare adapter](./cloudflare) for Workers deployment
- Check the [API reference](/api/) for testing endpoints
