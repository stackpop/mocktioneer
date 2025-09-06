Deploying Mocktioneer on Fastly Compute@Edge

Prereqs
- Fastly CLI installed and logged in: `fastly profile create` (or `fastly login`)
- Rust toolchain (stable), wasm32-wasi target installed
- In `mocktioneer/` directory

Local Dev
- `fastly compute serve` to run a local dev server on http://127.0.0.1:7676
- Test: `./examples/curl_local_test.sh`

First-time Project Setup
- If this was not initialized via `fastly compute init`, you can publish directly:
  - `fastly compute publish`
  - Fastly will prompt to create a service, attach a default domain, and deploy

Custom Domain (optional)
- After publishing, add a custom domain in the Fastly UI or via CLI:
  - `fastly domain create --service-id <SERVICE_ID> --name <your.domain.example>`

Environment / Config
- This mock service is originless; no backends required.
- CORS is enabled for `*` by default.

Endpoints
- `POST /openrtb2/auction` — accepts an OpenRTB 2.x BidRequest and responds with synthetic banner bids
- `GET /creative/<id>` — returns a simple HTML creative (for debugging/preview)

Using with Prebid.js
- Set the adapter endpoint to your deployed URL if different from the default:
  - `params: { endpoint: 'https://<your-domain>/openrtb2/auction' }`

Using with Prebid Server
- In PBS config: `adapters.mocktioneer.enabled: true` and set `adapters.mocktioneer.endpoint`
- Or per-imp override: `imp[i].ext.bidder.endpoint: 'https://<your-domain>/openrtb2/auction'`

