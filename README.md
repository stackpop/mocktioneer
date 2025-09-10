mocktioneer (Fastly Compute@Edge)

Overview
- Minimal OpenRTB mock bidder written in Rust for Fastly Compute@Edge. It returns deterministic banner bids and simple creatives to validate client integrations (Prebid.js, Prebid Server).
- Built on AnyEdge core types and router; originless (no backends).

Workspace Layout
- `crates/mocktioneer-core`: shared logic (OpenRTB types, routes, rendering, config, build_app()).
- `crates/mocktioneer-fastly`: Fastly Compute@Edge binary (+ Fastly and logging config).
- `crates/mocktioneer-cloudflare`: Cloudflare Workers binary (Wrangler config for dev/deploy).

HTTP Endpoints
- GET `/` — Info page with basic service metadata.
- POST `/openrtb2/auction` — Accepts OpenRTB 2.x BidRequest JSON, returns banner BidResponse. Bids include `impid`, `price`, `adm` (HTML iframe), `crid`, `w`, `h`, `mtype=1`, `adomain`.
- GET `/static/creatives/{W}x{H}.html?crid=...` — Minimal HTML wrapper that displays the SVG and links to `/click`.
- GET `/static/img/{W}x{H}.svg` — Dynamic SVG labeled “mocktioneer {W}×{H}”, optional `?bid=2.50` overlay.
- GET `/click?crid=...&w=...&h=...` — Simple landing page that echoes values.

Standard Sizes
- Supported: 300x250, 320x50, 728x90, 160x600, 300x50, 300x600, 970x250, 468x60, 336x280, 320x100.
- Static assets: non‑standard sizes return 404. Auction responses: non‑standard sizes default to 300x250.

OpenRTB Mapping
- See `src/openrtb.rs`.
- `imp[].banner` with `w/h` or `format[]` is supported.
- Optional echo price: `imp[i].ext.mocktioneer.bid` (number). If set, used as `seatbid.bid[].price` and echoed in `seatbid.bid[].ext.mocktioneer.bid`. The iframe creative also shows the price badge.

Local Development
- Prereqs: Fastly CLI logged in (`fastly profile create` or `fastly login`), Rust stable, target `wasm32-wasip1`.
- From `mocktioneer/`:
  - Run dev server (Fastly): `cd crates/mocktioneer-fastly && fastly compute serve` (serves http://127.0.0.1:7676)
  - Run dev server (Cloudflare): `cd crates/mocktioneer-cloudflare && wrangler dev` (requires `cargo install worker-build`)
  - Quick test: `./examples/curl_local_test.sh`
  - Unit tests: `cargo test`

Logging
- Config: `mocktioneer/mocktioneer.toml` is embedded at build time for the Fastly target.

  ```toml
  [logging]
  endpoint = "mocktioneerlog"   # Fastly log endpoint name on your service
  level = "info"                # one of: off|error|warn|info|debug|trace
  ```

- Behavior:
  - Fastly (`wasm32-wasip1`): uses Fastly log streaming. During `fastly compute serve`, logs are also echoed to the local console.
  - Changing `mocktioneer.toml` requires rebuild (compiled in via `include_str!`).

Deploy to Fastly
- First publish (run inside the Fastly crate):
  - `cd crates/mocktioneer-fastly`
  - `fastly compute publish` (CLI guides through creating a service, attaching a domain, deploying).
- Optional: add a custom domain: `fastly domain create --service-id <SERVICE_ID> --name <your.domain.example>`
- Ensure your service has a log endpoint named in `mocktioneer.toml` (e.g., `mocktioneerlog`).

Using with Prebid.js
- Adapter: see `Prebid.js/modules/mocktioneerBidAdapter.js`. Default endpoint: `https://mocktioneer.edgecompute.app/openrtb2/auction`.
- Per‑adunit override:
  - `params: { endpoint: 'https://<your-domain>/openrtb2/auction', bid: 2.5 }`
  - `params.bid` (number) is passed through as `imp.ext.mocktioneer.bid` and echoed as `bid.meta.mocktioneer.bid`.

Using with Prebid Server (PBS)
- Enable bidder and set endpoint (host config), or override per‑imp:
  - Host config: `adapters.mocktioneer.enabled: true`, `adapters.mocktioneer.endpoint: https://<your-domain>/openrtb2/auction`
  - Per‑imp override: `imp[i].ext.bidder.endpoint`
  - Optional echo price: `imp[i].ext.prebid.bidder.mocktioneer.bid: 2.5`

Examples
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

Notes
- CORS: `Access-Control-Allow-Origin: *` and preflight supported via OPTIONS with `Allow` header.
- Host selection: the service builds creative URLs from the `Host` header; if absent, defaults to `mocktioneer.edgecompute.app`.
- Scope: banner only; video/native are not implemented.
