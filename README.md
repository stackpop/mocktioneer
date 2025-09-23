# mocktioneer

Deterministic OpenRTB banner bidder for edge platforms. Mocktioneer helps test client integrations (Prebid.js, Prebid Server, custom SDKs) without depending on third-party bidders or origin backends.

## Highlights

- Built on the provider-agnostic [AnyEdge](https://github.com/stackpop/prebid/blob/main/anyedge/README.md) core so the same app runs on Fastly Compute@Edge and Cloudflare Workers.
- Deterministic banner bids and simple creative templates for predictable QA flows.
- Zero backend requirements: all routes render locally from embedded assets and config.
- Batteries included for integrations: ready-made responses for Prebid.js and Prebid Server, plus static assets for creative previews.

## Workspace Layout

- `crates/mocktioneer-core`: shared logic (OpenRTB types, request handlers, rendering, config, `MocktioneerApp` Hooks entrypoint).
- `crates/mocktioneer-adapter-fastly`: Fastly Compute@Edge binary + embedded logging config.
- `crates/mocktioneer-adapter-cloudflare`: Cloudflare Workers binary (`wrangler` manifests for dev/deploy).
- `examples/`: helper scripts like `openrtb_request.sh`, `iframe_request.sh`, and `pixel_request.sh` for smoke testing.

## Quick Start

1. Install prerequisites:
   - Rust (stable toolchain).
   - Fastly CLI (`brew install fastly/tap/fastly`) and/or Cloudflare `wrangler` if you plan to run those targets.
   - Add `wasm32-wasip1` (Fastly) or `wasm32-unknown-unknown` (Cloudflare) targets via `rustup target add`.
2. Clone this repo and enter `mocktioneer/`.
3. Run unit tests: `cargo test`.
4. Start a local Fastly sandbox: `cd crates/mocktioneer-adapter-fastly && fastly compute serve` (serves http://127.0.0.1:7676).
5. (Optional) Start Cloudflare Workers: `cd crates/mocktioneer-adapter-cloudflare && wrangler dev`.

## Running the Edge Bundles

### Fastly Compute@Edge
- `fastly compute serve -C crates/mocktioneer-adapter-fastly` to iterate locally; responses stream over the embedded AnyEdge router.
- Logging is controlled by `mocktioneer.toml` (embedded during build). See [Configuration & Logging](#configuration--logging).

### Cloudflare Workers
- `wrangler dev` inside `crates/mocktioneer-adapter-cloudflare` runs the app with the Workers runtime.
- The adapter reuses the same AnyEdge app and translates Worker requests/responses in-process.

## Configuration & Logging

`mocktioneer.toml` is compiled into the Fastly binary and controls logging behaviour:

```toml
[logging]
provider = "fastly"      # fastly | stdout
endpoint = "mocktioneerlog"
level = "info"           # off|error|warn|info|debug|trace
```

- `provider=fastly` streams logs to the named endpoint and echoes to stdout when served locally.
- `provider=stdout` is convenient for native/unit tests.
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
3. Configure a log endpoint that matches `mocktioneer.toml` (e.g., `mocktioneerlog`).
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
