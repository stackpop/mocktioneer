# Fastly Compute Adapter

The Fastly adapter runs Mocktioneer on Fastly's Compute platform, providing global edge deployment with low latency.

## Overview

| Property | Value |
|----------|-------|
| Crate | `mocktioneer-adapter-fastly` |
| Target | `wasm32-wasip1` |
| Platform | Fastly Compute |
| Use Case | Production edge deployment |

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

Run locally using Fastly's Viceroy runtime:

```bash
# Using EdgeZero CLI
edgezero-cli serve --adapter fastly

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

```bash
cd crates/mocktioneer-adapter-fastly
fastly compute publish
```

The CLI will prompt you to:
1. Create a new service or select existing
2. Configure the domain
3. Deploy the WASM bundle

### Subsequent Deployments

```bash
# Using EdgeZero CLI
edgezero-cli deploy --adapter fastly

# Or directly
fastly compute deploy -C crates/mocktioneer-adapter-fastly
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

| Limit | Value |
|-------|-------|
| Memory | 128 MB |
| Request timeout | 60 seconds |
| Package size | 100 MB |

Mocktioneer is well within these limits for typical usage.

## Next Steps

- Set up logging endpoints in Fastly console
- Configure custom domains
- Review [API reference](/api/) for endpoint testing
- Consider [Cloudflare adapter](./cloudflare) as an alternative
