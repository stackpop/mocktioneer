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

[stores.config]
ids = ["mocktioneer_config"]
```

## App Section

The `[app]` section defines the core application:

| Field        | Description                               |
| ------------ | ----------------------------------------- |
| `name`       | Application identifier                    |
| `entry`      | Path to the core crate                    |
| `middleware` | List of middleware to apply to all routes |

## Config Store

The `[stores.config]` section declares the logical config store(s) backing the
typed app config (`mocktioneer.toml`):

```toml
[stores.config]
ids = ["mocktioneer_config"]
```

The `mocktioneer_config` store holds the typed config blob (see the
`MocktioneerConfig` struct). Seed it per adapter with
`mocktioneer-cli config push --adapter <name>`; the OpenRTB/APS handlers read it
at runtime via the **fail-loud `AppConfig` extractor** — a `config push` is
required before those endpoints serve (see [Typed App Config](#typed-app-config)
below).

## HTTP Triggers

Routes are defined as `[[triggers.http]]` blocks:

```toml
[[triggers.http]]
id = "openrtb_auction"
path = "/openrtb2/auction"
methods = ["POST"]
handler = "mocktioneer_core::routes::handle_openrtb_auction"
adapters = ["axum", "cloudflare", "fastly", "spin"]
```

| Field      | Description                                |
| ---------- | ------------------------------------------ |
| `id`       | Unique route identifier                    |
| `path`     | URL path (supports `{param}` placeholders) |
| `methods`  | HTTP methods to accept                     |
| `handler`  | Rust function path                         |
| `adapters` | Which adapters support this route          |

### Available Routes

| Path                       | Methods | Handler                   | Description              |
| -------------------------- | ------- | ------------------------- | ------------------------ |
| `/`                        | GET     | `handle_root`             | Service info page        |
| `/openrtb2/auction`        | POST    | `handle_openrtb_auction`  | OpenRTB 2.x bid request  |
| `/e/dtb/bid`               | POST    | `handle_aps_bid`          | APS TAM bid request      |
| `/static/img/{size}`       | GET     | `handle_static_img`       | SVG creative image       |
| `/static/creatives/{size}` | GET     | `handle_static_creatives` | HTML creative wrapper    |
| `/click`                   | GET     | `handle_click`            | Click landing page       |
| `/pixel`                   | GET     | `handle_pixel`            | Tracking pixel           |
| `/aps/win`                 | GET     | `handle_aps_win`          | APS win notification     |
| `/adserver/mediate`        | POST    | `handle_adserver_mediate` | Auction mediation        |
| `/_/sizes`                 | GET     | `handle_sizes`            | Supported sizes as JSON  |
| `/sync/start`              | GET     | `handle_sync_start`       | EC pixel sync initiation |
| `/sync/done`               | GET     | `handle_sync_done`        | EC pixel sync callback   |
| `/resolve`                 | GET     | `handle_resolve`          | EC pull sync resolution  |
| `/_mocktioneer/manifest`   | GET     | `introspection::manifest` | Full manifest as JSON    |
| `/_mocktioneer/config`     | GET     | `introspection::config`   | Effective app config     |
| `/_mocktioneer/routes`     | GET     | `introspection::routes`   | Route table as JSON      |

All routes also have OPTIONS handlers for CORS preflight.

### Introspection Routes

The `/_mocktioneer/{manifest,config,routes}` endpoints are **framework-supplied**
handlers from `edgezero_core::introspection`, bound like any other route in
`edgezero.toml`:

- **`manifest`** — the full `edgezero.toml` manifest as JSON (baked at compile
  time; `[environment.secrets]` values are redacted).
- **`config`** — the effective app config from the default config store (the
  pushed `bid_cpm` blob's `.data`), with any `#[secret]` fields left as
  unresolved key-name references (secret-safe).
- **`routes`** — the live route table as `[{ "method", "path" }]`.

::: warning Unauthenticated
These endpoints are unauthenticated wherever bound — restrict access at the
network/middleware layer before exposing them publicly. `manifest` emits
`[environment.variables]` values verbatim (only `[environment.secrets]` are
redacted), so keep secrets out of `[environment.variables]`. Mocktioneer
declares no `[environment]` section, so nothing sensitive is exposed today.
:::

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
build = "fastly compute build -C crates/mocktioneer-adapter-fastly"
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
build = "wrangler build --cwd crates/mocktioneer-adapter-cloudflare"
serve = "wrangler dev --cwd crates/mocktioneer-adapter-cloudflare"
deploy = "wrangler deploy --cwd crates/mocktioneer-adapter-cloudflare"

[adapters.cloudflare.logging]
level = "info"
echo_stdout = true
```

### Spin Adapter

Spin targets `wasm32-wasip2` (spin-sdk 6). Its config store is KV-backed, so the
`serve`/`deploy` commands pass a `--runtime-config-file` declaring the KV label:

```toml
[adapters.spin.adapter]
crate = "crates/mocktioneer-adapter-spin"
manifest = "crates/mocktioneer-adapter-spin/spin.toml"

[adapters.spin.build]
target = "wasm32-wasip2"
profile = "release"
features = ["spin"]

[adapters.spin.commands]
build = "spin build --from crates/mocktioneer-adapter-spin/spin.toml"
serve = "spin up --from crates/mocktioneer-adapter-spin/spin.toml --runtime-config-file crates/mocktioneer-adapter-spin/runtime-config.toml"
deploy = "spin deploy --from crates/mocktioneer-adapter-spin/spin.toml --runtime-config-file crates/mocktioneer-adapter-spin/runtime-config.toml"

[adapters.spin.logging]
level = "info"
echo_stdout = true
```

## Typed App Config

`mocktioneer.toml` (repo root) maps 1:1 onto the `MocktioneerConfig` struct —
there is no `[config]` wrapper:

```toml
bid_cpm = 0.20
```

`mocktioneer.toml` is **gitignored** (per-environment); the repo commits
`mocktioneer.toml.example` as the template. Create your local copy first:

```bash
cp mocktioneer.toml.example mocktioneer.toml   # then edit bid_cpm as needed
```

Validate it, preview the diff against the live store, then push it:

```bash
cargo run -p mocktioneer-cli -- config validate --strict
cargo run -p mocktioneer-cli -- config diff --adapter axum
cargo run -p mocktioneer-cli -- config push --adapter axum --yes
```

`config push` writes the whole struct as a single **blob envelope** (canonical
JSON + a SHA for drift detection) under the store's key — for axum that's
`.edgezero/local-config-mocktioneer_config.json` as
`{ "mocktioneer_config": "<envelope>" }`. The handlers read it back through the
typed `AppConfig` extractor.

::: warning Config is required at runtime
The OpenRTB (`/openrtb2/auction`) and APS (`/e/dtb/bid`) endpoints read
`bid_cpm` via the fail-loud `AppConfig` extractor. **A fresh deploy must run
`config push` once** before those endpoints serve bids — until then they return
an error (the static/creative/pixel endpoints are unaffected). `bid_cpm = 0.20`
is the shipped default value, not a runtime fallback.
:::

Any key can be overridden via the `MOCKTIONEER__<KEY>` env overlay
(e.g. `MOCKTIONEER__BID_CPM=0.35`). The overlay is applied **when the CLI loads
`mocktioneer.toml`** (during `config validate` / `diff` / `push`), so it changes
the value pushed into the config-store blob — set it before `config push`. It
does **not** mutate config the running server has already loaded; re-push to
roll out a change.

## Logging Configuration

| Field         | Description                                          |
| ------------- | ---------------------------------------------------- |
| `endpoint`    | Log endpoint name (Fastly-specific)                  |
| `level`       | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `echo_stdout` | Whether to print logs to stdout                      |

## Environment Variables

Mocktioneer reads these optional environment variables at runtime for Edge Cookie sync configuration:

| Variable                 | Description                                                             | Default                                                        |
| ------------------------ | ----------------------------------------------------------------------- | -------------------------------------------------------------- |
| `MOCKTIONEER_TS_DOMAINS` | Comma-separated allowlist of trusted-server hostnames for `/sync/start` | Unset (all syntactically valid domains allowed; demo/dev mode) |
| `MOCKTIONEER_PULL_TOKEN` | Bearer token required for `/resolve` authentication                     | Unset (auth disabled); empty values fail closed                |

```bash
# Example: restrict sync to specific trusted-server instances
export MOCKTIONEER_TS_DOMAINS="ts.publisher.com,ts.staging.publisher.com"
export MOCKTIONEER_PULL_TOKEN="<YOUR_PULL_TOKEN>"
```

::: warning Production Security
Set both `MOCKTIONEER_TS_DOMAINS` and a non-empty `MOCKTIONEER_PULL_TOKEN` for production-style deployments. On Cloudflare Workers, these values are currently read with `std::env::var`, so `wrangler.toml` bindings are not enforced by this core code path.
:::

See the [Trusted Server integration guide](/integrations/trusted-server) for full setup details.

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
