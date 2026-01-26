# Cloudflare Workers Adapter

The Cloudflare adapter runs Mocktioneer on Cloudflare Workers, providing global edge deployment through Cloudflare's network.

## Overview

| Property | Value |
|----------|-------|
| Crate | `mocktioneer-adapter-cloudflare` |
| Target | `wasm32-unknown-unknown` |
| Platform | Cloudflare Workers |
| Use Case | Production edge deployment |

## Prerequisites

1. **Wrangler CLI**
   ```bash
   npm install -g wrangler
   # Or use npx wrangler
   ```

2. **WASM target**
   ```bash
   rustup target add wasm32-unknown-unknown
   ```

3. **Cloudflare account** with Workers enabled

4. **Authenticate Wrangler**
   ```bash
   wrangler login
   ```

## Local Development

Run locally using Wrangler's local mode:

```bash
# Using EdgeZero CLI
edgezero-cli serve --adapter cloudflare

# Or directly
wrangler dev --config crates/mocktioneer-adapter-cloudflare/wrangler.toml
```

This starts a local server that emulates the Workers environment.

## Building

```bash
# Using EdgeZero CLI
edgezero-cli build --adapter cloudflare

# Or directly
cargo build --release --target wasm32-unknown-unknown -p mocktioneer-adapter-cloudflare
```

The build produces a WASM binary at:
```
target/wasm32-unknown-unknown/release/mocktioneer_adapter_cloudflare.wasm
```

## Deployment

### First-Time Setup

1. Edit `crates/mocktioneer-adapter-cloudflare/wrangler.toml`:
   ```toml
   name = "mocktioneer"
   main = "build/worker/shim.mjs"
   compatibility_date = "2024-01-01"
   
   [build]
   command = "cargo build --release --target wasm32-unknown-unknown"
   ```

2. Deploy:
   ```bash
   wrangler publish --config crates/mocktioneer-adapter-cloudflare/wrangler.toml
   ```

### Subsequent Deployments

```bash
# Using EdgeZero CLI
edgezero-cli deploy --adapter cloudflare

# Or directly
wrangler publish --config crates/mocktioneer-adapter-cloudflare/wrangler.toml
```

## Configuration

### Build Settings

```toml
[adapters.cloudflare.build]
target = "wasm32-unknown-unknown"
profile = "release"
features = ["cloudflare"]
```

### Logging

```toml
[adapters.cloudflare.logging]
level = "info"
echo_stdout = true
```

Cloudflare Workers logs are visible in:
- `wrangler tail` for real-time logs
- Workers Analytics in the dashboard

### wrangler.toml

The `crates/mocktioneer-adapter-cloudflare/wrangler.toml` contains Workers-specific configuration:

```toml
name = "mocktioneer"
main = "build/worker/shim.mjs"
compatibility_date = "2024-01-01"

[vars]
# Environment variables
ENVIRONMENT = "production"

[[kv_namespaces]]
# KV store bindings if needed
# binding = "MY_KV"
# id = "xxx"
```

## Custom Domains

### Using workers.dev

By default, your Worker is available at:
```
https://mocktioneer.<your-subdomain>.workers.dev
```

### Custom Domain

1. Add domain to Cloudflare (if not already)
2. In Workers dashboard, go to your Worker
3. Click "Triggers" > "Custom Domains"
4. Add your domain

Or via wrangler.toml:
```toml
routes = [
  { pattern = "mocktioneer.example.com/*", zone_name = "example.com" }
]
```

## Environment Variables

Set environment variables in wrangler.toml:

```toml
[vars]
LOG_LEVEL = "debug"
```

Or as secrets:
```bash
wrangler secret put API_KEY
```

## Monitoring

### Real-Time Logs

```bash
wrangler tail --config crates/mocktioneer-adapter-cloudflare/wrangler.toml
```

### Dashboard Analytics

In the Cloudflare dashboard under Workers:
- Request count
- CPU time
- Error rate
- Geographic distribution

## Troubleshooting

### Build Errors

```bash
# Ensure target is installed
rustup target add wasm32-unknown-unknown

# Clean and rebuild
cargo clean
cargo build --release --target wasm32-unknown-unknown -p mocktioneer-adapter-cloudflare
```

### Local Development Issues

```bash
# Check wrangler version
wrangler --version

# Run with verbose output
wrangler dev --config crates/mocktioneer-adapter-cloudflare/wrangler.toml --local
```

### Deployment Failures

```bash
# Check authentication
wrangler whoami

# Validate configuration
wrangler publish --dry-run --config crates/mocktioneer-adapter-cloudflare/wrangler.toml
```

## Performance Considerations

Cloudflare Workers has these limits:

| Limit | Free | Paid |
|-------|------|------|
| CPU time | 10ms | 50ms |
| Memory | 128 MB | 128 MB |
| Script size | 1 MB | 10 MB |
| Subrequests | 50 | 1000 |

Mocktioneer typically uses:
- < 5ms CPU time per request
- < 10 MB memory
- No subrequests

## Workers KV Integration

If you need persistent storage, add KV bindings:

```toml
[[kv_namespaces]]
binding = "MOCKTIONEER_KV"
id = "your-kv-namespace-id"
```

Create the namespace:
```bash
wrangler kv:namespace create "MOCKTIONEER_KV"
```

## Next Steps

- Configure custom domain
- Set up monitoring and alerts
- Review [API reference](/api/) for endpoint testing
- Consider [Fastly adapter](./fastly) as an alternative
