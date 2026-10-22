# Architecture

Mocktioneer is built on [EdgeZero](https://github.com/IABTechLab/edgezero), an adapter-agnostic framework for edge computing. This architecture allows the same application logic to run across multiple platforms.

## Workspace Layout

```
mocktioneer/
├── Cargo.toml              # Workspace manifest
├── edgezero.toml           # EdgeZero configuration
├── crates/
│   ├── mocktioneer-core/   # Shared business logic
│   ├── mocktioneer-adapter-axum/       # Native HTTP server
│   ├── mocktioneer-adapter-fastly/     # Fastly Compute binary
│   ├── mocktioneer-adapter-cloudflare/ # Cloudflare Workers binary
│   ├── mocktioneer-adapter-spin/       # Spin component
│   └── mocktioneer-cli/                # CLI and typed config commands
└── examples/               # Helper scripts
```

EdgeZero is consumed via git dependencies (see `Cargo.toml`).

## Crate Responsibilities

### mocktioneer-core

The core crate contains all shared logic:

```
mocktioneer-core/
├── src/
│   ├── lib.rs          # App entrypoint, exports modules
│   ├── routes.rs       # HTTP handlers
│   ├── openrtb.rs      # OpenRTB types and parsing
│   ├── aps.rs          # APS TAM types and parsing
│   ├── auction.rs      # Bid generation logic
│   ├── mediation.rs    # Auction mediation
│   ├── render.rs       # HTML/SVG rendering
│   └── verification.rs # Request signature verification
├── static/
│   ├── pixel.gif       # 1x1 transparent GIF
│   └── templates/      # Handlebars templates
└── tests/
    ├── endpoints.rs    # Integration tests
    └── aps_endpoints.rs
```

### Adapter Crates

Each adapter crate is minimal - it just wires up the EdgeZero runtime:

```rust
// mocktioneer-adapter-axum/src/main.rs
fn main() -> Result<(), Box<dyn std::error::Error>> {
    edgezero_adapter_axum::dev_server::run_app::<mocktioneer_core::MocktioneerApp>()?;
    Ok(())
}
```

The adapter crates handle platform-specific concerns:

- Request/response translation
- Runtime initialization
- Platform-specific logging

## EdgeZero Integration

### App Macro

The core crate uses the `edgezero_core::app!` macro to generate the app structure:

```rust
edgezero_core::app!("../../edgezero.toml", MocktioneerApp, configure = crate::configure_app);
```

This macro:

1. Parses `edgezero.toml` at compile time
2. Generates route registration code
3. Creates the `MocktioneerApp` hooks type, including a fallible configuration hook

Runtime entrypoints assemble the app with platform metadata and propagate any
configuration error. Tests can build it directly:

```rust
use edgezero_core::app::App;
use edgezero_core::PlatformMetadata;
use mocktioneer_core::MocktioneerApp;

let app = App::build::<MocktioneerApp>(PlatformMetadata::default())
    .expect("build Mocktioneer app");
```

The configuration hook sets a two-second config-extraction deadline and a
10-second body-read budget for the three POST bid routes, whose manifest class
is `bid`. Other routes keep the 30-second default. See [request limits](/api/#request-limits-and-deadlines).

### Middleware

Middleware is applied to all routes in order:

1. **RequestLogger** - Logs incoming requests
2. **Cors** - Adds CORS headers to responses

### Request Context

Handlers receive a `RequestContext` that provides:

- Request body and headers
- Path parameters
- Query string parsing
- Validated JSON extraction and bounded body reads
- Outbound HTTP via `ctx.http_client()` and a portable `ctx.monotonic_clock()`

Read method, URI, and headers through `ctx.method()`, `ctx.uri()`, and
`ctx.headers()`. Converting the context back to a request is fallible:
`ctx.into_request()?`.

## Data Flow

```
┌─────────────────┐
│  HTTP Request   │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│    Adapter      │  Platform-specific request handling
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│   Middleware    │  Logging, CORS
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│     Router      │  Match path to handler
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│    Handler      │  Business logic in mocktioneer-core
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│   Response      │  JSON, HTML, or binary
└─────────────────┘
```

## Module Details

### routes.rs

HTTP handlers for all endpoints. Uses extractors for type-safe request parsing:

```rust
#[action]
pub async fn handle_openrtb_auction(
    RequestContext(ctx): RequestContext,
    ForwardedHost(host): ForwardedHost,
    AppConfig(config): AppConfig<MocktioneerConfig>,
    ValidatedJsonWithin(req): ValidatedJsonWithin<OpenRTBRequest, MAX_BID_REQUEST_BYTES>,
) -> Result<Response, EdgeError> {
    // ...
}
```

`MAX_BID_REQUEST_BYTES` is 1 MiB; config extraction happens before the body.
Mediation retains `ValidatedJson` with the default 8 MiB cap. Imports come from
`edgezero_core::extractor`, `edgezero_core::context`, and
`edgezero_core::http::Response`, with `EdgeError` and `action` from `edgezero_core`.

### openrtb.rs

OpenRTB 2.x type definitions with serde serialization:

- `OpenRTBRequest` - Bid request structure
- `OpenRTBResponse` - Bid response structure
- `Impression`, `Banner`, `Bid`, etc.

### auction.rs

Bid generation logic:

- `build_openrtb_response()` - Generate bids for OpenRTB
- `build_aps_response()` - Generate bids for APS TAM
- `is_standard_size()` - Check if dimensions are supported

### render.rs

Template rendering:

- `render_svg()` - Generate SVG creative with size/bid badge
- `creative_html()` - Generate HTML wrapper for creative
- `info_html()` - Generate service info page

## Supported Sizes

Mocktioneer supports 13 standard IAB ad sizes, each with a default CPM price (configurable via `bid_cpm` in mocktioneer.toml). See the [full size list with pricing](/api/#supported-sizes) in the API reference.

Non-standard sizes return 404 for static assets or are coerced to 300x250 for auction responses. Use the [`/_/sizes`](/api/#sizes-endpoint) endpoint to get the current list programmatically.
