Mocktioneer (Fastly Compute@Edge)

Overview
- A tiny Rust service meant to run at Fastly edge that mocks an OpenRTB bidder.
- Endpoints:
  - POST `/openrtb2/auction`: accepts OpenRTB2.x BidRequest, returns a synthetic BidResponse with banner HTML in `adm`.
  - GET `/creative/<id>`: serves simple HTML creative by id.

Local Dev (optional)
- Requires the Fastly CLI and Rust toolchain. Example commands:
  - `fastly compute serve` to run locally (will use `fastly.toml`).
  - `fastly compute publish` to deploy to a Fastly service.

Notes
- This project is originless. No backend is needed.
- Response HTML is deliberately simple; adjust as needed.

