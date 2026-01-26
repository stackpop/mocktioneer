# Axum Adapter

The Axum adapter runs Mocktioneer as a native Rust HTTP server. It's the recommended choice for local development and integration testing.

## Overview

| Property | Value |
|----------|-------|
| Crate | `mocktioneer-adapter-axum` |
| Target | Native (no WASM) |
| Default Port | 8787 |
| Use Case | Development, testing, CI/CD |

## Quick Start

```bash
cargo run -p mocktioneer-adapter-axum
```

The server starts at `http://127.0.0.1:8787`.

## Using EdgeZero CLI

```bash
edgezero-cli serve --adapter axum
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

## Docker Deployment

For containerized deployment:

```bash
# Build the image
docker build -t mocktioneer:latest .

# Run on default port
docker run -p 8787:8787 mocktioneer:latest

# Run on custom port
docker run -p 3000:8787 mocktioneer:latest
```

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

The Axum adapter behaves identically to edge adapters for request handling, but:

| Feature | Axum | Edge (Fastly/Cloudflare) |
|---------|------|--------------------------|
| Startup time | Instant | Cold start possible |
| Debugging | Full access | Limited |
| KV stores | Not available | Platform-specific |
| Logging | stdout | Platform logging |

## Customization

To customize the Axum adapter behavior, edit:

- `crates/mocktioneer-adapter-axum/src/main.rs` - Entrypoint
- `crates/mocktioneer-adapter-axum/axum.toml` - Adapter-specific config

Example: changing the listen address would require modifying the adapter code.

## Next Steps

- Try the [Fastly adapter](./fastly) for edge deployment
- Try the [Cloudflare adapter](./cloudflare) for Workers deployment
- Check the [API reference](/api/) for testing endpoints
