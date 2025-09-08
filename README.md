mocktioneer (Fastly Compute@Edge)

Overview
- A tiny Rust service that mocks an OpenRTB bidder, designed to run on Fastly Compute@Edge. It returns deterministic banner bids and simple creatives to help test client integrations (Prebid.js, PBS, etc.).

Key Features
- OpenRTB 2.x request/response structs (`src/openrtb.rs`) with typed `mtype` enum.
- Banner bids only (for now): HTML in `adm`, `mtype=1 (banner)`, `adomain=["example.com"]`.
- Template-based creatives and images under `static/templates`.
- Simple click handler (`/click`) that echoes `crid`, `w`, `h`.
- CORS enabled for `*` and `OPTIONS` preflight support.

HTTP Endpoints
- POST `/openrtb2/auction`
  - Accepts an OpenRTB 2.x BidRequest JSON and returns a synthetic banner BidResponse.
  - Response bids include: `impid`, `price`, `adm` (HTML), `crid`, `w`, `h`, `mtype=1`, `adomain`.
- GET `/static/creatives/{W}x{H}.html?crid=...`
  - Serves a minimal HTML wrapper that displays the SVG image and links to `/click` with the provided `crid`, `w`, `h`.
- GET `/static/img/{W}x{H}.svg`
  - Dynamic SVG that renders the label “mocktioneer {W}×{H}”.
- GET `/click?crid=...&w=...&h=...`
  - Simple landing page that echoes the passed values.

Standard Sizes
- Static assets accept standard banner sizes only; non-standard sizes 404. Supported: 300x250, 320x50, 728x90, 160x600, 300x50, 300x600, 970x250, 468x60, 336x280, 320x100.
- Auction responses will default non-standard sizes to 300x250.

OpenRTB Types
- See `src/openrtb.rs`. Important bits:
  - `OpenRTBRequest` with `imp: Vec<Imp>` supporting `banner` with `w/h` or `format[]`.
  - `OpenRTBResponse` with `seatbid[].bid[]`.
  - `Bid.mtype: Option<MediaType>` where `MediaType` is an enum: Banner=1, Video=2, Native=4.

Building & Running (Local)
Prereqs
- Fastly CLI installed and logged in: `fastly profile create` (or `fastly login`)
- Rust toolchain (stable) and the `wasm32-wasi` target
- From `mocktioneer/` directory

Commands
- Run locally: `fastly compute serve`
  - Local dev server on http://127.0.0.1:7676
  - Quick test: `./examples/curl_local_test.sh`
- Build tests: `cargo test --no-run`

Logging
- Configure logging via `mocktioneer.toml` (embedded at build time):

  ```toml
  [logging]
  # Provider: "fastly" (wasm32-wasi Compute@Edge) or "stdout" (local/native)
  provider = "fastly"
  # Fastly log endpoint name (must exist on your Fastly service)
  endpoint = "mocktioneerlog"
  # one of: off|error|warn|info|debug|trace
  level = "info"
  ```

- Behavior by target:
  - wasm32-wasi (Fastly): uses the Fastly logger; `echo_stdout` mirrors logs in `fastly compute serve`.
  - Native/local: uses the stdout logger when `provider = "stdout"` (Fastly provider falls back to stdout).

- Notes:
  - Changes to `mocktioneer.toml` require rebuilding because it’s compiled in via `include_str!`.
  - Legacy env vars are no longer used; use the TOML file instead.

Deploying to Fastly
- First-time publish (Fastly will prompt to create a service, attach a domain, and deploy):
  - `fastly compute publish`
- Add a custom domain (optional):
  - `fastly domain create --service-id <SERVICE_ID> --name <your.domain.example>`

Configuration & Notes
- This service is originless; no backends required.
- CORS is enabled for `*` by default.
- Templates live in `static/templates/`:
  - `iframe.html`: Used by the response builder when an iframe creative is preferred.
  - `creative.html`: The page shown inside the iframe; displays the SVG image.
  - `image.svg`: Generates the labeled creative “mocktioneer {W}×{H}”.

Using with Prebid.js
- If using the included mock Prebid adapter (in this repo’s `Prebid.js/modules/mocktioneerBidAdapter.js`), the default endpoint is `https://mocktioneer.edgecompute.app/openrtb2/auction`.
- You can override per ad unit: `params: { endpoint: 'https://<your-domain>/openrtb2/auction' }`.
- Optional echo param (decimal): set `params.bid` (a number, e.g. `2.5`). mocktioneer will set `seatbid.bid[].price` to that value and echo it in `seatbid.bid[].ext.mocktioneer.bid` and PBJS `bid.meta.mocktioneer.bid`.

Using with Prebid Server
- In PBS config: `adapters.mocktioneer.enabled: true` and set `adapters.mocktioneer.endpoint`.
- Or per-imp override: `imp[i].ext.bidder.endpoint: 'https://<your-domain>/openrtb2/auction'`.
- Optional echo param (decimal): set `imp[i].ext.prebid.bidder.mocktioneer.bid` to a number (e.g. `2.5`). PBS adapter passes it upstream. mocktioneer will use it as `seatbid.bid[].price` and echo it in `seatbid.bid[].ext.mocktioneer.bid`.

Example: Minimal OpenRTB BidRequest (banner)
```json
{
  "id": "r1",
  "imp": [
    {"id": "1", "banner": {"w": 300, "h": 250}}
  ]
}
```

Example: Response (truncated)
```json
{
  "id": "r1",
  "cur": "USD",
  "seatbid": [{
    "seat": "mocktioneer",
    "bid": [{
      "id": "...",
      "impid": "1",
      "price": 1.23,
      "adm": "<html>...",
      "crid": "mocktioneer-1",
      "w": 300,
      "h": 250,
      "mtype": 1,
      "adomain": ["example.com"]
    }]
  }]
}
```

Examples

- cURL: POST an auction request

  ```bash
  # Local dev
  curl -sS -X POST \
    -H 'Content-Type: application/json' \
    --data '{
      "id": "r1",
      "imp": [
        {"id": "1", "banner": {"w": 300, "h": 250}}
      ]
    }' \
    http://127.0.0.1:7676/openrtb2/auction | jq .

  # Deployed (replace with your domain)
  curl -sS -X POST \
    -H 'Content-Type: application/json' \
    --data '{
      "id": "r1",
      "imp": [
        {"id": "1", "banner": {"w": 300, "h": 250}}
      ]
    }' \
    https://mocktioneer.edgecompute.app/openrtb2/auction | jq .
  ```

- HTML: embed the iframe creative directly

  ```html
  <!-- Replace host and size as needed; crid is optional -->
  <iframe
    src="//mocktioneer.edgecompute.app/static/creatives/300x250.html?crid=demo-creative"
    width="300"
    height="250"
    frameborder="0"
    scrolling="no">
  </iframe>
  ```

Troubleshooting
- Prebid error “Cannot determine mediaType for response”: ensure the response contains `seatbid[].bid[].mtype` (we set `mtype=1` for banner).
- Non-standard sizes for static assets return 404; use one of the supported sizes listed above.
