# mocktioneer (Fastly Compute@Edge)

## Overview

- Minimal OpenRTB mock bidder written in Rust for Fastly Compute@Edge. It returns deterministic banner bids and simple creatives to validate client integrations (Prebid.js, Prebid Server).
- Built on AnyEdge core types and router; originless (no backends).

## Workspace Layout

- `crates/mocktioneer-core`: shared logic (OpenRTB types, routes, rendering, config, build_app()).
- `crates/mocktioneer-fastly`: Fastly Compute@Edge binary (+ Fastly and logging config).
- `crates/mocktioneer-cloudflare`: Cloudflare Workers binary (Wrangler config for dev/deploy).

## HTTP Endpoints

- GET `/` — Info page with basic service metadata.
  - Headers
    - `Host`: used for display only on the info page.

- POST `/openrtb2/auction` — Accepts OpenRTB 2.x BidRequest JSON, returns banner BidResponse.
  - Body (JSON)
    - `id` (string, required): request ID echoed in response.
    - `imp[]` (array, required): impressions to bid on.
      - `id` (string, optional): impression ID; defaults to "1" if empty.
      - `banner.w` (int, optional): width in pixels.
      - `banner.h` (int, optional): height in pixels.
      - `banner.format[]` (array, optional): fallback size candidates; first element used when `w/h` missing.
      - `ext.mocktioneer.bid` (number, optional): explicit price to return; echoed in `bid.ext.mocktioneer.bid` and rendered on creatives.
  - Headers
    - `Host` (string, optional): base host used to build creative iframe URLs.
  - Response (JSON): contains `seatbid[].bid[]` with `impid`, `price`, `adm` (iframe HTML), `crid`, `w`, `h`, `mtype=1`, `adomain`.

- GET `/static/creatives/{W}x{H}.html` — Minimal HTML wrapper that displays the SVG and links to `/click`.
  - Path params
    - `W` (int, required): width; must be a standard size.
    - `H` (int, required): height; must be a standard size.
  - Query params
    - `crid` (string, optional): creative ID, forwarded to click URL.
    - `bid` (number, optional): displayed as a price badge on the underlying SVG.
  - Notes: non‑standard sizes return 404.

- GET `/static/img/{W}x{H}.svg` — Dynamic SVG labeled “mocktioneer {W}×{H}``.
  - Path params
    - `W` (int, required): width; must be a standard size.
    - `H` (int, required): height; must be a standard size.
  - Query params
    - `bid` (number, optional): displays a `-$` price badge.
  - Notes: non‑standard sizes return 404.

- GET `/click` — Simple landing page that echoes values.
  - Query params
    - `crid` (string, optional): creative ID to echo.
    - `w` (int, optional): width to echo.
    - `h` (int, optional): height to echo.

- GET `/pixel` — 1×1 transparent GIF for tracking.
  - Behavior: sets a `mtkid` cookie if absent.
  - Cookie
    - `name`: `mtkid`
    - `value`: UUIDv7 string
    - `attributes`: `Path=/; Max-Age=31536000; SameSite=None; Secure; HttpOnly`

## Standard Sizes

- Supported: 300x250, 320x50, 728x90, 160x600, 300x50, 300x600, 970x250, 468x60, 336x280, 320x100.
- Static assets: non‑standard sizes return 404. Auction responses: non‑standard sizes default to 300x250.

## OpenRTB Mapping

- See `src/openrtb.rs`.
- `imp[].banner` with `w/h` or `format[]` is supported.
- Optional echo price: `imp[i].ext.mocktioneer.bid` (number). If set, used as `seatbid.bid[].price` and echoed in `seatbid.bid[].ext.mocktioneer.bid`. The iframe creative also shows the price badge.

### Minimal BidRequest example

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

## Local Development

- Prereqs: Fastly CLI logged in (`fastly profile create` or `fastly login`), Rust stable, target `wasm32-wasip1`.
- From `mocktioneer/`:
  - Run dev server (Fastly): `cd crates/mocktioneer-fastly && fastly compute serve` (serves http://127.0.0.1:7676)
  - Run dev server (Cloudflare): `cd crates/mocktioneer-cloudflare && wrangler dev` (requires `cargo install worker-build`)
  - Quick test: `./examples/curl_local_test.sh`
  - Unit tests: `cargo test`

## Logging

- Config: `mocktioneer/mocktioneer.toml` is embedded at build time for the Fastly target.

```toml
[logging]
endpoint = "mocktioneerlog"   # Fastly log endpoint name on your service
level = "info"                # one of: off|error|warn|info|debug|trace
provider = "fastly"           # one of: fastly|stdout (optional; default fastly)
```

### Logging parameters

- `logging.provider`: log sink. `fastly` streams to a Fastly logging endpoint; `stdout` prints to console (useful locally).
- `logging.endpoint`: Fastly log endpoint name on your service (required when `provider=fastly`).
- `logging.level`: minimum level to emit (`off|error|warn|info|debug|trace`).

- Behavior:
  - Fastly (`wasm32-wasip1`): uses Fastly log streaming. During `fastly compute serve`, logs are also echoed to the local console.
  - Changing `mocktioneer.toml` requires rebuild (compiled in via `include_str!`).

## Deploy to Fastly

- First publish (run inside the Fastly crate):
  - `cd crates/mocktioneer-fastly`
  - `fastly compute publish` (CLI guides through creating a service, attaching a domain, deploying)
- Optional: add a custom domain: `fastly domain create --service-id <SERVICE_ID> --name <your.domain.example>`
- Ensure your service has a log endpoint named in `mocktioneer.toml` (e.g., `mocktioneerlog`).

## Using with Prebid.js

- Adapter: see `Prebid.js/modules/mocktioneerBidAdapter.js`. Default endpoint: `https://mocktioneer.edgecompute.app/openrtb2/auction`.
- Per‑adunit override:
  - `params: { endpoint: 'https://<your-domain>/openrtb2/auction', bid: 2.5 }`
  - `params.bid` (number) is passed through as `imp.ext.mocktioneer.bid` and echoed as `bid.meta.mocktioneer.bid`.

### Adapter params

- `endpoint` (string, required): URL of `/openrtb2/auction` on your deployment.
- `bid` (number, optional): fixed CPM to return for that ad unit.

## Using with Prebid Server (PBS)

- Enable bidder and set endpoint (host config), or override per‑imp:
  - Host config: `adapters.mocktioneer.enabled: true`, `adapters.mocktioneer.endpoint: https://<your-domain>/openrtb2/auction`
  - Per‑imp override: `imp[i].ext.bidder.endpoint`
  - Optional echo price: `imp[i].ext.prebid.bidder.mocktioneer.bid: 2.5`

### PBS parameters

- `adapters.mocktioneer.enabled` (bool): enable the bidder in host config.
- `adapters.mocktioneer.endpoint` (string): default endpoint for all requests.
- `imp[i].ext.bidder.endpoint` (string): per‑imp endpoint override.
- `imp[i].ext.prebid.bidder.mocktioneer.bid` (number): fixed CPM to return for the impression.

## Examples

- Local cURL:

```bash
curl -sS -X POST \
  -H 'Content-Type: application/json' \
  --data '{"id":"r1","imp":[{"id":"1","banner":{"w":300,"h":250}}]}' \
  http://127.0.0.1:7676/openrtb2/auction | jq .
```

- Embed creative:

```html
<iframe src="//mocktioneer.edgecompute.app/static/creatives/300x250.html?crid=demo" width="300" height="250" frameborder="0" scrolling="no"></iframe>
```

## Notes

- CORS: `Access-Control-Allow-Origin: *` and preflight supported via OPTIONS with `Allow` header.
- Host selection: the service builds creative URLs from the `Host` header; if absent, defaults to `mocktioneer.edgecompute.app`.
- Scope: banner only; video/native are not implemented.
