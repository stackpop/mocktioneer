# mocktioneer

Deterministic OpenRTB banner bidder for edge platforms. Mocktioneer helps test client integrations (Prebid.js, Prebid Server, custom SDKs) without depending on third-party bidders or origin backends.

## Highlights

- Built on the adapter-agnostic [AnyEdge](https://github.com/stackpop/prebid/blob/main/anyedge/README.md) core so the same app runs on Fastly Compute@Edge and Cloudflare Workers.
- Manifest-driven: a single `anyedge.toml` defines routes, adapters, logging, and the commands the AnyEdge CLI executes for build/serve/deploy.
- Deterministic banner bids and simple creative templates for predictable QA flows.
- Zero backend requirements: all routes render locally from embedded assets and the AnyEdge manifest.
- Batteries included for integrations: ready-made responses for Prebid.js and Prebid Server, plus static assets for creative previews.

## Workspace Layout

- `crates/mocktioneer-core`: shared logic (OpenRTB types, request handlers, rendering, config, `MocktioneerApp` Hooks entrypoint).
- `crates/mocktioneer-adapter-fastly`: Fastly Compute@Edge binary powered by the shared AnyEdge manifest.
- `crates/mocktioneer-adapter-cloudflare`: Cloudflare Workers binary (`wrangler` manifests for dev/deploy).
- `examples/`: helper scripts like `openrtb_request.sh`, `iframe_request.sh`, and `pixel_request.sh` for smoke testing.

## Quick Start

1. Install prerequisites:
   - Rust (stable toolchain).
   - Fastly CLI (`brew install fastly/tap/fastly`) and/or Cloudflare `wrangler` if you plan to run those targets.
   - Add `wasm32-wasip1` (Fastly) or `wasm32-unknown-unknown` (Cloudflare) targets via `rustup target add`.
   - AnyEdge CLI (`cargo install --path anyedge/crates/anyedge-cli --features cli`, or run via `cargo run --manifest-path ../anyedge/Cargo.toml -p anyedge-cli --features cli -- --help`).
2. Clone this repo and enter `mocktioneer/`.
3. Run unit tests: `cargo test`.
4. Serve adapters via the manifest-driven CLI (reads `anyedge.toml` automatically):
   - `anyedge-cli serve --adapter fastly` (wraps `fastly compute serve -C crates/mocktioneer-adapter-fastly`).
   - `anyedge-cli serve --adapter cloudflare` (wraps `wrangler dev --config crates/mocktioneer-adapter-cloudflare/wrangler.toml`).
   - Without installing the binary, use `cargo run --manifest-path ../anyedge/Cargo.toml -p anyedge-cli --features cli -- serve --adapter fastly` (and similar for other adapters).

## Running the Edge Bundles

### Fastly Compute@Edge
- `anyedge-cli serve --adapter fastly` to iterate locally; this shells out to `fastly compute serve -C crates/mocktioneer-adapter-fastly` after embedding `anyedge.toml`.
- Logging is controlled by `anyedge.toml` (embedded during build). See [Configuration & Logging](#configuration--logging).

### Cloudflare Workers
- `anyedge-cli serve --adapter cloudflare` wraps `wrangler dev --config crates/mocktioneer-adapter-cloudflare/wrangler.toml`.
- The adapter reuses the same AnyEdge app and translates Worker requests/responses in-process.

## Configuration & Logging

`anyedge.toml` is compiled into both adapter binaries and drives the AnyEdge CLI:

```toml
[app]
name = "Mocktioneer"
entry = "crates/mocktioneer-core"
middleware = ["anyedge_core::middleware::RequestLogger", "mocktioneer_core::routes::Cors"]

[[triggers.http]]
id = "openrtb_auction"
path = "/openrtb2/auction"
methods = ["POST"]
handler = "mocktioneer_core::routes::handle_openrtb_auction"
adapters = ["fastly", "cloudflare"]

[adapters.fastly.adapter]
crate = "crates/mocktioneer-adapter-fastly"
manifest = "crates/mocktioneer-adapter-fastly/fastly.toml"

[adapters.fastly.commands]
serve = "fastly compute serve -C crates/mocktioneer-adapter-fastly"
deploy = "fastly compute deploy -C crates/mocktioneer-adapter-fastly"

[logging.fastly]
endpoint = "mocktioneerlog"
level = "info"
echo_stdout = false

[logging.cloudflare]
level = "info"
```

- Routes and adapters are declared in the manifest; adding a new handler means updating the `[[triggers.http]]` block once and both adapters inherit it.
- Adapter command strings are what `anyedge-cli` executes for `build`, `serve`, and `deploy`, so tweaks to local/dev flows happen in one place.
- Logging defaults live under `[logging.*]` and are turned into adapter-specific logger configs at runtime.
- Changing the file requires rebuilding because it is embedded with `include_str!`.

## HTTP API Overview

| Method | Path                               | Purpose |
| ------ | ---------------------------------- | ------- |
| GET    | `/`                                | Service info page rendered from template (uses `Host` header for display). |
| POST   | `/openrtb2/auction`                | Accepts OpenRTB 2.x banner requests and returns deterministic bids. |
| GET    | `/static/creatives/{W}x{H}.html`   | HTML wrapper around the SVG creative and optional tracking pixel. |
| GET    | `/static/img/{W}x{H}.svg`          | Dynamic SVG showing size + optional bid badge. |
| GET    | `/click`                           | Landing page that echoes creative metadata. |
| GET    | `/pixel?pid={id}`                  | 1×1 transparent GIF that sets a long-lived `mtkid` cookie (requires `pid` query). |

### Auction (`/openrtb2/auction`)
- Supports `imp[].banner.w/h` or `imp[].banner.format[0]` size hints.
- Optional price override via `imp[i].ext.mocktioneer.bid` (float). When set, it drives the returned CPM and is echoed in the creative.
- Builds iframe HTML pointing to `/static/creatives/{size}.html?crid=...&bid=...` using the incoming `Host` header (defaults to `mocktioneer.edgecompute.app`).

### Creatives & Assets
- `pixel` query parameter on `/static/creatives/...` toggles the tracking pixel (`true` by default). Accepts `false|0|no|off` to disable. When enabled, the rendered HTML auto-generates a random `pid` query string for `/pixel`.
- `/static/img/...` accepts a `bid` query parameter (rendered as a badge like `-$2.50`).
- `/pixel` (with a required `pid` query parameter) issues `Set-Cookie: mtkid=<UUIDv7>; Path=/; Max-Age=31536000; SameSite=None; Secure; HttpOnly` when no cookie is present and marks the response as non-cacheable.
  Provide any non-empty ID when calling the endpoint directly; creatives rendered by Mocktioneer generate a random value automatically.

### Standard Sizes

Supported creative sizes:

`300x250`, `320x50`, `728x90`, `160x600`, `300x50`, `300x600`, `970x250`, `468x60`, `336x280`, `320x100`

- Static asset routes return `404` for non-standard sizes.
- Auction responses coerce unknown sizes to `300x250` to keep creatives valid.

### OpenRTB Mapping

- Definition lives in [`crates/mocktioneer-core/src/openrtb.rs`](crates/mocktioneer-core/src/openrtb.rs).
- `Bid.mtype` is set to `1` (Banner). `adomain` is populated with `example.com` for compatibility.
- Creatives are rendered via [`render::iframe_html`](crates/mocktioneer-core/src/render.rs).
- Example request:

```json
{
  "id": "r1",
  "imp": [
    {
      "id": "1",
      "banner": { "w": 300, "h": 250 },
      "ext": { "mocktioneer": { "bid": 2.5 } }
    }
  ]
}
```

## Testing & Tooling

- Unit tests: `cargo test` (covers routes, OpenRTB mapping, SVG rendering, cookie behaviour).
- Smoke test: `./examples/openrtb_request.sh` posts a sample OpenRTB request against a local Fastly dev server (override the host with `MOCKTIONEER_BASE_URL`).
- When adjusting routes or templates, update `crates/mocktioneer-core/tests/endpoints.rs` to keep coverage meaningful.

## Deployment

### Fastly
1. `cd crates/mocktioneer-adapter-fastly`
2. `fastly compute publish` (CLI walks you through service creation, domain binding, deploy).
3. Configure a log endpoint that matches `anyedge.toml` (e.g., `mocktioneerlog`).
4. Optional: `fastly domain create --service-id <SERVICE_ID> --name <your.domain>` to add custom domains.

### Cloudflare Workers
- `wrangler publish` from `crates/mocktioneer-adapter-cloudflare` builds and deploys the WASM bundle.
- Ensure the account/project configuration in `wrangler.toml` is correct before publishing.

## Integrations

### Prebid.js
- Adapter source: `Prebid.js/modules/mocktioneerBidAdapter.js`.
- Default endpoint: `https://mocktioneer.edgecompute.app/openrtb2/auction`.
- Per-ad unit override example:

```js
params: {
  endpoint: 'https://<your-domain>/openrtb2/auction',
  bid: 2.5
}
```

### Prebid Server (PBS)
- Enable and set the bidder endpoint in host config:
  - `adapters.mocktioneer.enabled: true`
  - `adapters.mocktioneer.endpoint: https://<your-domain>/openrtb2/auction`
- Per-impression overrides:
  - `imp[i].ext.bidder.endpoint` to point at a custom deployment.
  - `imp[i].ext.prebid.bidder.mocktioneer.bid` to force a CPM.

## Examples

- Helper scripts in `examples/` (override the host with `MOCKTIONEER_BASE_URL`):
  - `./examples/openrtb_request.sh [payload.json] [/openrtb2/auction]` - posts the bundled payload (or supplied file) and pretty-prints the response.
  - `./examples/iframe_request.sh [size] [crid] [bid] [pixel]` - fetches the rendered creative iframe HTML.
  - `./examples/pixel_request.sh [base64|raw|hexdump]` - requests `/pixel` (supplying a random `pid`, or override with `MOCKTIONEER_PIXEL_ID`) and streams the encoded response body.

- Local cURL smoke test:

```bash
curl -sS -X POST \
  -H 'Content-Type: application/json' \
  --data '{"id":"r1","imp":[{"id":"1","banner":{"w":300,"h":250}}]}' \
  http://127.0.0.1:7676/openrtb2/auction | jq .
```

- Embedding a creative in HTML:

```html
<iframe
  src="//mocktioneer.edgecompute.app/static/creatives/300x250.html?crid=demo"
  width="300"
  height="250"
  frameborder="0"
  scrolling="no"></iframe>
```

## Notes

- CORS: CORS headers (`Access-Control-Allow-Origin: *`) and OPTIONS preflight responses are handled by middleware + the AnyEdge router.
- Host detection: Creative URLs fall back to `mocktioneer.edgecompute.app` when the request lacks a `Host` header.
- Scope: Only banner inventory is implemented today; video/native can be layered on with additional route handlers.
