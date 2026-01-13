# mocktioneer

Deterministic OpenRTB banner bidder for edge platforms. Mocktioneer helps test client integrations (Prebid.js, Prebid Server, custom SDKs) without depending on third-party bidders or origin backends.

## Highlights

- Built on the adapter-agnostic [EdgeZero](https://github.com/stackpop/edgezero) core so the same app runs on Fastly Compute@Edge, Cloudflare Workers, and a native Axum server.
- Manifest-driven: a single `edgezero.toml` defines routes, adapters, logging, and the commands the EdgeZero CLI executes for build/serve/deploy.
- Deterministic banner bids and simple creative templates for predictable QA flows.
- Zero backend requirements: all routes render locally from embedded assets and the EdgeZero manifest.
- Batteries included for integrations: ready-made responses for Prebid.js and Prebid Server, plus static assets for creative previews.

## Workspace Layout

- `crates/mocktioneer-core`: shared logic (OpenRTB types, request handlers, rendering, config, `MocktioneerApp` Hooks entrypoint).
- `crates/mocktioneer-adapter-fastly`: Fastly Compute@Edge binary powered by the shared EdgeZero manifest.
- `crates/mocktioneer-adapter-cloudflare`: Cloudflare Workers binary (`wrangler` manifests for dev/deploy).
- `crates/mocktioneer-adapter-axum`: Native Axum HTTP server for local integration testing.
- `examples/`: helper scripts like `openrtb_request.sh`, `iframe_request.sh`, and `pixel_request.sh` for smoke testing.

## Quick Start

1. Install prerequisites:
   - Rust (stable toolchain).
   - Fastly CLI (`brew install fastly/tap/fastly`) and/or Cloudflare `wrangler` if you plan to run those targets.
   - Add `wasm32-wasip1` (Fastly) or `wasm32-unknown-unknown` (Cloudflare) targets via `rustup target add`.
   - No extra tooling is required for Axum—the native adapter runs with the default Rust toolchain (`cargo run -p mocktioneer-adapter-axum`).
   - EdgeZero CLI (`cargo install --path edgezero/crates/edgezero-cli --features cli`, or run via `cargo run --manifest-path ../edgezero/Cargo.toml -p edgezero-cli --features cli -- --help`).
2. Clone this repo and enter `mocktioneer/`.
3. Run unit tests: `cargo test`.
4. Serve adapters via the manifest-driven CLI (reads `edgezero.toml` automatically):
   - `edgezero-cli serve --adapter axum` (spawns the native Axum server for local iteration).
   - `edgezero-cli serve --adapter fastly` (wraps `fastly compute serve -C crates/mocktioneer-adapter-fastly`).
   - `edgezero-cli serve --adapter cloudflare` (wraps `wrangler dev --config crates/mocktioneer-adapter-cloudflare/wrangler.toml`).
   - Without installing the binary, use `cargo run --manifest-path ../edgezero/Cargo.toml -p edgezero-cli --features cli -- serve --adapter <axum|fastly|cloudflare>`.

> EdgeZero crates are now pulled directly from GitHub over SSH, so CI/CD no longer needs a sibling checkout. Provide an `EDGEZERO_SSH_KEY` secret (deploy key or personal access key) in GitHub Actions so the workflows can authenticate, and copy `.cargo/config.local-example` to `.cargo/config.toml` when iterating on EdgeZero locally to point back at `../edgezero`.

## Running the Edge Bundles

### Axum (native)
- `edgezero-cli serve --adapter axum` launches the native HTTP server (listens on `127.0.0.1:8787` by default; tweak the manifest command if you need a different address).
- Alternatively, run `cargo run -p mocktioneer-adapter-axum` for direct access without the CLI wrapper.
- Logging is plain stdout/stderr unless you override `[adapters.axum.logging]` in the manifest.

### Fastly Compute@Edge
- `edgezero-cli serve --adapter fastly` to iterate locally; this shells out to `fastly compute serve -C crates/mocktioneer-adapter-fastly` after embedding `edgezero.toml`.
- Logging is controlled by `edgezero.toml` (embedded during build). See [Configuration & Logging](#configuration--logging).

### Cloudflare Workers
- `edgezero-cli serve --adapter cloudflare` wraps `wrangler dev --config crates/mocktioneer-adapter-cloudflare/wrangler.toml`.
- The adapter reuses the same EdgeZero app and translates Worker requests/responses in-process.

## Configuration & Logging

`edgezero.toml` is compiled into every adapter binary and drives the EdgeZero CLI:

```toml
[app]
name = "mocktioneer"
entry = "crates/mocktioneer-core"
middleware = ["edgezero_core::middleware::RequestLogger", "mocktioneer_core::routes::Cors"]

[[triggers.http]]
id = "openrtb_auction"
path = "/openrtb2/auction"
methods = ["POST"]
handler = "mocktioneer_core::routes::handle_openrtb_auction"
adapters = ["axum", "cloudflare", "fastly"]

[adapters.axum.adapter]
crate = "crates/mocktioneer-adapter-axum"
manifest = "crates/mocktioneer-adapter-axum/axum.toml"

[adapters.axum.commands]
serve = "cargo run -p mocktioneer-adapter-axum"
deploy = "# configure deployment for Axum"

[adapters.axum.logging]
level = "info"
echo_stdout = true

[adapters.cloudflare.commands]
serve = "wrangler dev --config crates/mocktioneer-adapter-cloudflare/wrangler.toml"

[adapters.cloudflare.logging]
level = "info"
echo_stdout = true

[adapters.fastly.commands]
serve = "fastly compute serve -C crates/mocktioneer-adapter-fastly"

[adapters.fastly.logging]
endpoint = "mocktioneerlog"
level = "info"
echo_stdout = false
```

- Routes and adapters are declared in the manifest; adding a new handler means updating the `[[triggers.http]]` block once and every adapter inherits it.
- Adapter command strings under `[adapters.<name>.commands]` are what `edgezero-cli` executes for `build`, `serve`, and `deploy`, so tweaks to local/dev flows happen in one place.
- Logging defaults live under `[adapters.<name>.logging]` and are turned into adapter-specific logger configs at runtime.
- Changing the file requires rebuilding because it is embedded with `include_str!`.

## HTTP API Overview

| Method | Path                               | Purpose |
| ------ | ---------------------------------- | ------- |
| GET    | `/`                                | Service info page rendered from template (uses `Host` header for display). |
| POST   | `/openrtb2/auction`                | Accepts OpenRTB 2.x banner requests and returns deterministic bids. |
| POST   | `/e/dtb/bid`                       | **APS TAM bid endpoint** - accepts APS-specific request format and returns APS bids. |
| GET    | `/static/creatives/{W}x{H}.html`   | HTML wrapper around the SVG creative and optional tracking pixel. |
| GET    | `/static/img/{W}x{H}.svg`          | Dynamic SVG showing size + optional bid badge. |
| GET    | `/click`                           | Landing page that echoes creative metadata. |
| GET    | `/pixel?pid={id}`                  | 1×1 transparent GIF that sets a long-lived `mtkid` cookie (requires `pid` query). |
| GET    | `/aps/win`                         | **APS win notification endpoint** - logs win events (requires `slot` and `price` query params). |

### Auction (`/openrtb2/auction`)
- Supports `imp[].banner.w/h` or `imp[].banner.format[0]` size hints.
- Optional price override via `imp[i].ext.mocktioneer.bid` (float). When set, it drives the returned CPM and is echoed in the creative.
- Builds iframe HTML pointing to `/static/creatives/{size}.html?crid=...&bid=...` using the incoming `Host` header (defaults to `mocktioneer.edgecompute.app`).

### Creatives & Assets
- `pixel` query parameter on `/static/creatives/...` toggles the tracking pixel (`true` by default). Accepts `false|0|no|off` to disable. When enabled, the rendered HTML auto-generates a random `pid` query string for `/pixel`.
- `/static/img/...` accepts a `bid` query parameter (rendered as a badge like `-$2.50`).
- `/pixel` (with a required `pid` query parameter) issues `Set-Cookie: mtkid=<UUIDv7>; Path=/; Max-Age=31536000; SameSite=None; Secure; HttpOnly` when no cookie is present and marks the response as non-cacheable.
  Provide any non-empty ID when calling the endpoint directly; creatives rendered by Mocktioneer generate a random value automatically.

### APS TAM API (`/e/dtb/bid`)

Amazon Publisher Services (APS) Transparent Ad Marketplace endpoint. Accepts APS-specific bid requests and returns bids in APS format.

- Request format matches APS TAM API: `{ "pubId": "...", "slots": [...], "pageUrl": "...", "ua": "...", "timeout": 800 }`
- Response format: `{ "bids": [{ "slotID": "...", "price": 2.50, "adm": "...", "w": 300, "h": 250, ... }] }`
- Variable bid prices based on ad size: $1.50 - $4.50 CPM (e.g., 300x250 = $2.50, 970x250 = $4.20, 320x50 = $1.80)
- 100% fill rate for standard sizes
- Creatives rendered as iframes pointing to mocktioneer's `/static/creatives/` endpoints
- APS targeting key-value pairs: `amzniid`, `amznbid`, `amznsz`

Example request:
```json
{
  "pubId": "1234",
  "slots": [
    {
      "slotID": "header-banner",
      "slotName": "header-banner",
      "sizes": [[728, 90], [970, 250]]
    }
  ],
  "pageUrl": "https://example.com",
  "ua": "Mozilla/5.0...",
  "timeout": 800
}
```

Example response (real Amazon API format):
```json
{
  "contextual": {
    "slots": [
      {
        "slotID": "header-banner",
        "size": "728x90",
        "crid": "019b7f82e8de7e13-mocktioneer",
        "mediaType": "d",
        "fif": "1",
        "targeting": ["amzniid", "amznp", "amznsz", "amznbid", "amznactt"],
        "meta": ["slotID", "mediaType", "size"],
        "amzniid": "019b7f82e8de7e139d6d6a593171e7a0",
        "amznbid": "mjuw",
        "amznp": "mjuw",
        "amznsz": "728x90",
        "amznactt": "OPEN"
      }
    ],
    "host": "https://mocktioneer.edgecompute.app",
    "status": "ok",
    "cfe": true,
    "ev": true,
    "cfn": "bao-csm/direct/csm_othersv6.js",
    "cb": "6"
  }
}
```

**Important Notes:**
- Response format matches real Amazon APS API exactly (with `contextual` wrapper)
- **Price encoding differs between real APS and our mock**:
  - **Real Amazon APS**: Uses proprietary encoding that only Amazon and trusted partners (like GAM) can decode
  - **Our mock**: Uses base64 encoding for testing purposes - prices ARE recoverable
  - Example decoding: `echo "Mi41MA==" | base64 -d` → `2.50`
  - Mock encoding is transparent to support debugging and test validation workflows
- **No creative HTML (`adm` field)** - Real APS expects publishers to render creatives client-side using targeting keys
- Targeting keys are returned as flat fields on each slot object
- Mock uses **variable pricing based on ad size** ($1.50 - $4.50 CPM)
- 100% fill rate (`fif: "1"`) for standard sizes only

Test locally:
```bash
./examples/aps_request.sh
```

### Standard Sizes

Supported creative sizes:

`300x250`, `320x50`, `728x90`, `160x600`, `300x50`, `300x600`, `970x250`, `468x60`, `336x280`, `320x100`

- Static asset routes return `404` for non-standard sizes.
- OpenRTB auction responses coerce unknown sizes to `300x250` to keep creatives valid.
- APS endpoint skips non-standard sizes (returns empty `slots` array for those slots).

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

### Axum
- Native server is mainly for local integration testing; package and deploy it like any other Rust binary if you want to run it outside CI.
- The manifest ships with a placeholder `deploy` command—replace it with whatever fits your environment (e.g., Docker build, systemd service, or container platform CLI).

### Fastly
1. `cd crates/mocktioneer-adapter-fastly`
2. `fastly compute publish` (CLI walks you through service creation, domain binding, deploy).
3. Configure a log endpoint that matches `edgezero.toml` (e.g., `mocktioneerlog`).
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
  - `./examples/aps_request.sh [payload.json]` - posts an APS TAM bid request and pretty-prints the response.
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

- CORS: CORS headers (`Access-Control-Allow-Origin: *`) and OPTIONS preflight responses are handled by middleware + the EdgeZero router.
- Host detection: Creative URLs fall back to `mocktioneer.edgecompute.app` when the request lacks a `Host` header.
- Scope: Only banner inventory is implemented today; video/native can be layered on with additional route handlers.
