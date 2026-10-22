# Cloudflare Workers Adapter

The Cloudflare adapter runs Mocktioneer on Cloudflare Workers, providing global edge deployment through Cloudflare's network.

## Overview

| Property | Value                            |
| -------- | -------------------------------- |
| Crate    | `mocktioneer-adapter-cloudflare` |
| Target   | `wasm32-unknown-unknown`         |
| Platform | Cloudflare Workers               |
| Use Case | Production edge deployment       |

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

3. **worker-build** (used by the default `wrangler.toml` build command)

   ```bash
   cargo install worker-build
   ```

4. **Cloudflare account** with Workers enabled

5. **Authenticate Wrangler**
   ```bash
   wrangler login
   ```

## Local Development

Run locally using Wrangler's local mode:

```bash
# Using the CLI
edgezero-cli serve --adapter cloudflare
# or, in-repo (no external install):
cargo run -p mocktioneer-cli -- serve --adapter cloudflare

# Or directly
wrangler dev --cwd crates/mocktioneer-adapter-cloudflare
```

This starts a local server that emulates the Workers environment.

::: warning Config is KV-backed on Cloudflare
Unlike Axum (local file) and Fastly, Cloudflare reads `bid_cpm` from a **KV
namespace**, so `/openrtb2/auction` and `/e/dtb/bid` are fail-loud until the
config blob is pushed into that namespace. The KV namespace must be bound in
`wrangler.toml` as `[[kv_namespaces]] binding = "mocktioneer_config"`:

- **Remote/deploy:** `mocktioneer-cli provision --adapter cloudflare` creates the
  namespace and writes the binding into `wrangler.toml`, then
  `config push --adapter cloudflare`.
- **Local (`wrangler dev`):** `provision` is remote-only (it calls the Cloudflare
  API). For a local-only KV, add the binding to `wrangler.toml` manually with a
  placeholder id, then push to local state:

  ```toml
  [[kv_namespaces]]
  binding = "mocktioneer_config"
  id = "local-dev-placeholder"
  ```

  ```bash
  cp mocktioneer.toml.example mocktioneer.toml
  cargo run -p mocktioneer-cli -- config push --adapter cloudflare --local
  ```

Static/pixel/sizes endpoints work without any of this.
:::

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
   name = "mocktioneer-adapter-cloudflare"
   main = "build/worker/shim.mjs"
   compatibility_date = "2023-05-01"
   compatibility_flags = ["enable_request_signal", "enable_weak_ref", "internal_stream_byob_return_view"]

   [build]
   command = "worker-build --release"
   ```

2. Provision the real KV binding, then push the config blob:

   ```bash
   cargo run -p mocktioneer-cli -- provision --adapter cloudflare
   cp mocktioneer.toml.example mocktioneer.toml
   cargo run -p mocktioneer-cli -- config push --adapter cloudflare --yes
   ```

   Provisioning writes `binding = "mocktioneer_config"` and the namespace ID
   into `wrangler.toml`. Keep production namespace IDs out of commits.

3. Deploy:
   ```bash
   wrangler deploy --config crates/mocktioneer-adapter-cloudflare/wrangler.toml
   ```

### Subsequent Deployments

```bash
# Using EdgeZero CLI
edgezero-cli deploy --adapter cloudflare

# Or directly
wrangler deploy --config crates/mocktioneer-adapter-cloudflare/wrangler.toml
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
name = "mocktioneer-adapter-cloudflare"
main = "build/worker/shim.mjs"
compatibility_date = "2023-05-01"
compatibility_flags = ["enable_request_signal", "enable_weak_ref", "internal_stream_byob_return_view"]

[vars]
# Environment variables
ENVIRONMENT = "production"

# Added by provision; replace the placeholder for your own environment.
[[kv_namespaces]]
binding = "mocktioneer_config"
id = "your-kv-namespace-id"
```

All three compatibility flags are required for the request/config lifecycle.
Without `enable_weak_ref`, bounded KV reads cannot use `FinalizationRegistry`;
`AppConfig` returns 503 `service_unavailable` without `Retry-After`, even when
KV is bound. A missing KV binding produces the same status/kind. Variables such
as `EDGEZERO__STORES__CONFIG__*` only rename which binding is looked up; they do
not create the binding.

With the retained `2023-05-01` date, `internal_stream_byob_return_view` is also
required: it makes the final KV byte-stream read return the empty view expected
by the bounded reader. Without it, even a valid pushed config returns 503.
See Cloudflare's [BYOB compatibility flag](https://developers.cloudflare.com/workers/configuration/compatibility-flags/#returning-empty-uint8array-on-final-byob-read).

JWKS fetches retain `PlatformDefault` outbound caching. `Bypass` would require
a later compatibility date or `cache_option_enabled`; keep the committed
`2023-05-01` date and its flags together. Outbound HTTP is Native, while inbound,
outbound, and config-read deadlines remain BestEffort. See the
[capability table](./#capability-support).

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
wrangler dev --cwd crates/mocktioneer-adapter-cloudflare --local
```

### Deployment Failures

```bash
# Check authentication
wrangler whoami

# Validate configuration
wrangler deploy --dry-run --config crates/mocktioneer-adapter-cloudflare/wrangler.toml
```

## Performance Considerations

Cloudflare Workers has these limits:

| Limit       | Free   | Paid   |
| ----------- | ------ | ------ |
| CPU time    | 10ms   | 50ms   |
| Memory      | 128 MB | 128 MB |
| Script size | 1 MB   | 10 MB  |
| Subrequests | 50     | 1000   |

Bid routes read Workers KV, and signed trusted-server requests can make an
outbound JWKS subrequest. The local cache's 2 MiB owned-allocation budget does
not bound total Worker memory. Check current provider limits for your plan;
this migration does not qualify hosted provider behavior.

## Workers KV Integration

The required config namespace is `mocktioneer_config`; follow the provisioning
and push steps above. Additional KV bindings do not replace that config binding.

## Next Steps

- Configure custom domain
- Set up monitoring and alerts
- Review [API reference](/api/) for endpoint testing
- Consider [Fastly adapter](./fastly) as an alternative
