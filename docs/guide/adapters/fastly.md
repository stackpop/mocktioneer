# Fastly Compute Adapter

The Fastly adapter runs Mocktioneer on Fastly's Compute platform, providing global edge deployment with low latency.

## Overview

| Property | Value                        |
| -------- | ---------------------------- |
| Crate    | `mocktioneer-adapter-fastly` |
| Target   | `wasm32-wasip1`              |
| Platform | Fastly Compute               |
| Use Case | Production edge deployment   |

## Prerequisites

1. **Fastly CLI**

   ```bash
   brew install fastly/tap/fastly
   # Or download from https://developer.fastly.com/tools/cli
   ```

2. **WASM target**

   ```bash
   rustup target add wasm32-wasip1
   ```

3. **Fastly account** with Compute enabled

## Local Development

::: tip Push the config first
`/openrtb2/auction` and `/e/dtb/bid` are fail-loud — push the typed config to
the Fastly config store before serving (static/pixel/sizes work without it):

```bash
cp mocktioneer.toml.example mocktioneer.toml
cargo run -p mocktioneer-cli -- config push --adapter fastly --local
```

:::

Run locally using Fastly's Viceroy runtime:

```bash
# Using the CLI
edgezero-cli serve --adapter fastly
# or, in-repo (no external install):
cargo run -p mocktioneer-cli -- serve --adapter fastly

# Or directly
fastly compute serve -C crates/mocktioneer-adapter-fastly
```

This starts a local server that emulates the Fastly Compute environment.

## Building

```bash
# Using EdgeZero CLI
edgezero-cli build --adapter fastly

# Or directly
cargo build --release --target wasm32-wasip1 -p mocktioneer-adapter-fastly
```

The build produces a WASM binary at:

```
target/wasm32-wasip1/release/mocktioneer-adapter-fastly.wasm
```

## Deployment

### First-Time Setup

`/openrtb2/auction` and `/e/dtb/bid` read `bid_cpm` from the **remote Fastly
config store** via the fail-loud `AppConfig` extractor. Deploying alone is not
enough — without the config store created, seeded and linked, those endpoints
return `503 config_out_of_date`. Run the three steps in this order:

```bash
# 1. Create the remote config store and append [setup.config_stores.*] to fastly.toml
cargo run -p mocktioneer-cli -- provision --adapter fastly

# 2. Push the typed config blob into the store just created
cp mocktioneer.toml.example mocktioneer.toml   # if you haven't already
cargo run -p mocktioneer-cli -- config push --adapter fastly --yes

# 3. Deploy — creating the service consumes [setup] and links the store to it
cargo run -p mocktioneer-cli -- deploy --adapter fastly
```

The deploy will prompt you to:

1. Create a new service or select existing
2. Configure the domain
3. Deploy the WASM bundle

::: warning Already-deployed services skip `[setup]`
Fastly consumes `[setup.config_stores.*]` **only when `deploy` creates a new
service**. If `fastly.toml` already declares a `service_id`, the store is created
in your account but is **not linked** to the service, so the runtime cannot open
it. `provision` detects this and prints the exact one-shot command to finish the
job — look up the store id with `fastly config-store list --json`, then:

```bash
fastly resource-link create --service-id=<SERVICE-ID> --resource-id=<STORE-ID> \
  --version=latest --autoclone --name=mocktioneer_config
```

The link clones the active version, so live traffic is unaffected until you run
`fastly service-version activate`.
:::

### Subsequent Deployments

```bash
# Using EdgeZero CLI
edgezero-cli deploy --adapter fastly

# Or directly
fastly compute deploy -C crates/mocktioneer-adapter-fastly
```

### Updating the Config

`bid_cpm` lives in the config store, not the WASM bundle — after editing
`mocktioneer.toml`, **re-push**; no redeploy is needed:

```bash
cargo run -p mocktioneer-cli -- config diff --adapter fastly   # preview
cargo run -p mocktioneer-cli -- config push --adapter fastly --yes
```

## Configuration

### Build Settings

```toml
[adapters.fastly.build]
target = "wasm32-wasip1"
profile = "release"
features = ["fastly"]
```

### Logging

```toml
[adapters.fastly.logging]
endpoint = "mocktioneerlog"
level = "info"
echo_stdout = false
```

Fastly logging requires a configured log endpoint. Create one in the Fastly console:

1. Go to your service configuration
2. Add a logging endpoint (e.g., S3, BigQuery, or HTTPS)
3. Name it to match `endpoint` in the config

### fastly.toml

The `crates/mocktioneer-adapter-fastly/fastly.toml` contains Fastly-specific configuration:

```toml
[local_server]
# Local development settings
[local_server.backends]
# Backend configurations if needed
```

## Custom Domains

Add custom domains to your Fastly service:

```bash
fastly domain create --service-id <SERVICE_ID> --name mocktioneer.example.com
```

Or configure in the Fastly console under Domains.

## Environment Variables

Fastly Compute doesn't support traditional environment variables. Instead, use:

- **Config stores** for configuration
- **Secret stores** for sensitive data
- **Edge dictionaries** for key-value lookups

## Monitoring

### Logs

View logs in real-time:

```bash
fastly log-tail --service-id <SERVICE_ID>
```

### Metrics

Monitor in the Fastly dashboard:

- Request rate
- Error rate
- Response time percentiles
- Cache hit ratio (if caching enabled)

## Troubleshooting

### Build Errors

If you see WASM-related errors:

```bash
# Ensure target is installed
rustup target add wasm32-wasip1

# Clean and rebuild
cargo clean
cargo build --release --target wasm32-wasip1 -p mocktioneer-adapter-fastly
```

### Local Server Issues

```bash
# Check Viceroy is working
fastly compute serve --verbose -C crates/mocktioneer-adapter-fastly
```

### Deployment Failures

```bash
# Validate the package
fastly compute validate -C crates/mocktioneer-adapter-fastly

# Check service status
fastly service describe --service-id <SERVICE_ID>
```

## Performance Considerations

Fastly Compute has some constraints:

| Limit           | Value      |
| --------------- | ---------- |
| Memory          | 128 MB     |
| Request timeout | 60 seconds |
| Package size    | 100 MB     |

Mocktioneer is well within these limits for typical usage.

## Next Steps

- Set up logging endpoints in Fastly console
- Configure custom domains
- Review [API reference](/api/) for endpoint testing
- Consider [Cloudflare adapter](./cloudflare) as an alternative
