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

1. **Fastly CLI 16.0.0** and **Viceroy 0.21.0** for local serving

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

::: warning Breaking change in EdgeZero v0.0.10
Mocktioneer declares the `mocktioneer_config` store, so Fastly deployments now
use EdgeZero's managed lifecycle and require `--application-release`. The old
checkout-based `provision` → `config push` → bare `deploy --adapter fastly`
sequence is no longer supported.

Application-release packaging is deferred. This repository currently provides
no workflow or command to produce the required bundle. A production deployment
requires a separately prepared release following
[EdgeZero's application-release contract](https://github.com/IABTechLab/edgezero/blob/v0.0.10/docs/guide/deploy-action-adoption.md).
:::

### Deploying a Prepared Application Release

The release contains `release.json`, the prebuilt Fastly package, the application
CLI archive, `edgezero.toml`, and its referenced Fastly manifest. EdgeZero verifies
their hashes and the exact release member set before deployment. Adding the flag
to a source checkout alone does not satisfy that contract.

With a verified release extracted at `$RELEASE_ROOT`, an existing Compute service
ID, and `FASTLY_API_TOKEN` configured, deploy from a checkout matching the release's
source revision:

```bash
EDGEZERO_MANIFEST="$RELEASE_ROOT/edgezero.toml" \
  cargo run --locked -p mocktioneer-cli -- deploy --adapter fastly \
  --service-id "$FASTLY_SERVICE_ID" --application-release "$RELEASE_ROOT"
```

Managed deployment uploads the prebuilt package, reconciles store links, and
activates the service version. Create the service and provision and seed its
stores beforehand. `/openrtb2/auction` and `/e/dtb/bid` read `bid_cpm` from
that remote store through `AppConfig`. A missing config blob returns
`503 config_out_of_date` with `Retry-After: 60`; a store that cannot be opened
returns `500 internal`.

Keep the extracted release unchanged: run `provision` from the mutable source
checkout, and keep config files and extracted CLI binaries outside `$RELEASE_ROOT`.
Provisioning modifies `fastly.toml`; modifying a packaged manifest invalidates
the release. Use a new prepared release for subsequent deployments.

### Updating the Config

`bid_cpm` lives in the config store, not the WASM bundle — after editing
`mocktioneer.toml`, **re-push**; no redeploy is needed:

```bash
cargo run -p mocktioneer-cli -- config diff --adapter fastly   # preview
cargo run -p mocktioneer-cli -- config push --adapter fastly --yes
```

### Deployment Lifecycle (staging)

`mocktioneer-cli` also exposes the Fastly staging-lifecycle commands from
EdgeZero for a deploy → verify → roll-forward-or-back loop. Staging deployment
has the same application-release requirement. Inspect each command's options
before selecting the service version and domain to probe or roll back:

```bash
# Capture the current live version BEFORE deploying (your rollback target)
cargo run -p mocktioneer-cli -- active-version --adapter fastly \
  --service-id "$FASTLY_SERVICE_ID"

cargo run -p mocktioneer-cli -- healthcheck --help
cargo run -p mocktioneer-cli -- rollback --help
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

### Outbound JWKS Requests

Keep dynamic backends enabled for signed trusted-server verification; this
requirement is unchanged. The JWKS fetch does not follow redirects, uses a
five-second budget, and caps the final decoded buffer at 64 KiB. Fastly defaults
`Accept-Encoding` to `identity` when absent. Outbound HTTP and deadline support
are BestEffort, so a configured timeout does not guarantee provider teardown.
A fetch failure is reflected in creative signature metadata while the auction
still returns bids. See the [JWKS policy](/integrations/trusted-server#signature-verification-and-jwks).

The CLI declares outbound HTTP as optional and warns about Fastly's BestEffort
support. Hosted provider qualification is outside this migration; see the
[capability table](./#capability-support).

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

`managed Fastly deployment requires --application-release` means the deploy
needs the prepared bundle described above. A raw WASM binary or source checkout
is insufficient. Digest or extra-member errors require a fresh extraction of
the original verified release.

```bash
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
