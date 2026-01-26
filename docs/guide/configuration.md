# Configuration

Mocktioneer is configured through `edgezero.toml`, which is compiled into every adapter binary. This manifest-driven approach ensures consistent behavior across all platforms.

## Configuration File

The `edgezero.toml` file lives at the root of the mocktioneer workspace:

```toml
[app]
name = "mocktioneer"
entry = "crates/mocktioneer-core"
middleware = [
  "edgezero_core::middleware::RequestLogger",
  "mocktioneer_core::routes::Cors"
]
```

## App Section

The `[app]` section defines the core application:

| Field | Description |
|-------|-------------|
| `name` | Application identifier |
| `entry` | Path to the core crate |
| `middleware` | List of middleware to apply to all routes |

## HTTP Triggers

Routes are defined as `[[triggers.http]]` blocks:

```toml
[[triggers.http]]
id = "openrtb_auction"
path = "/openrtb2/auction"
methods = ["POST"]
handler = "mocktioneer_core::routes::handle_openrtb_auction"
adapters = ["axum", "cloudflare", "fastly"]
```

| Field | Description |
|-------|-------------|
| `id` | Unique route identifier |
| `path` | URL path (supports `{param}` placeholders) |
| `methods` | HTTP methods to accept |
| `handler` | Rust function path |
| `adapters` | Which adapters support this route |

### Available Routes

| Path | Methods | Handler |
|------|---------|---------|
| `/` | GET | `handle_root` |
| `/openrtb2/auction` | POST | `handle_openrtb_auction` |
| `/e/dtb/bid` | POST | `handle_aps_bid` |
| `/static/img/{size}` | GET | `handle_static_img` |
| `/static/creatives/{size}` | GET | `handle_static_creatives` |
| `/click` | GET | `handle_click` |
| `/pixel` | GET | `handle_pixel` |
| `/aps/win` | GET | `handle_aps_win` |
| `/adserver/mediate` | POST | `handle_adserver_mediate` |

All routes also have OPTIONS handlers for CORS preflight.

## Adapter Configuration

Each adapter has its own configuration section:

### Axum Adapter

```toml
[adapters.axum.adapter]
crate = "crates/mocktioneer-adapter-axum"
manifest = "crates/mocktioneer-adapter-axum/axum.toml"

[adapters.axum.build]
target = "native"
profile = "dev"

[adapters.axum.commands]
build = "cargo build -p mocktioneer-adapter-axum"
serve = "cargo run -p mocktioneer-adapter-axum"
deploy = "# configure deployment for Axum"

[adapters.axum.logging]
level = "info"
echo_stdout = true
```

### Fastly Adapter

```toml
[adapters.fastly.adapter]
crate = "crates/mocktioneer-adapter-fastly"
manifest = "crates/mocktioneer-adapter-fastly/fastly.toml"

[adapters.fastly.build]
target = "wasm32-wasip1"
profile = "release"
features = ["fastly"]

[adapters.fastly.commands]
build = "cargo build --release --target wasm32-wasip1 -p mocktioneer-adapter-fastly"
serve = "fastly compute serve -C crates/mocktioneer-adapter-fastly"
deploy = "fastly compute deploy -C crates/mocktioneer-adapter-fastly"

[adapters.fastly.logging]
endpoint = "mocktioneerlog"
level = "info"
echo_stdout = false
```

### Cloudflare Adapter

```toml
[adapters.cloudflare.adapter]
crate = "crates/mocktioneer-adapter-cloudflare"
manifest = "crates/mocktioneer-adapter-cloudflare/wrangler.toml"

[adapters.cloudflare.build]
target = "wasm32-unknown-unknown"
profile = "release"
features = ["cloudflare"]

[adapters.cloudflare.commands]
build = "cargo build --release --target wasm32-unknown-unknown -p mocktioneer-adapter-cloudflare"
serve = "wrangler dev --config crates/mocktioneer-adapter-cloudflare/wrangler.toml"
deploy = "wrangler publish --config crates/mocktioneer-adapter-cloudflare/wrangler.toml"

[adapters.cloudflare.logging]
level = "info"
echo_stdout = true
```

## Logging Configuration

| Field | Description |
|-------|-------------|
| `endpoint` | Log endpoint name (Fastly-specific) |
| `level` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `echo_stdout` | Whether to print logs to stdout |

## Rebuilding After Changes

Since `edgezero.toml` is embedded at compile time via `include_str!`, you must rebuild the adapter after making changes:

```bash
cargo build -p mocktioneer-adapter-axum
```

## Environment-Specific Configuration

For adapter-specific settings not covered by `edgezero.toml`:

- **Axum**: Edit `crates/mocktioneer-adapter-axum/axum.toml`
- **Fastly**: Edit `crates/mocktioneer-adapter-fastly/fastly.toml`
- **Cloudflare**: Edit `crates/mocktioneer-adapter-cloudflare/wrangler.toml`
