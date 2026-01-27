# API Reference

Mocktioneer exposes several HTTP endpoints for bid requests, creative serving, and tracking.

## Base URL

| Environment         | URL                                   | Notes                    |
| ------------------- | ------------------------------------- | ------------------------ |
| Local (Axum)        | `http://127.0.0.1:8787`               | Default development port |
| Local (Fastly)      | `http://127.0.0.1:7676`               | Viceroy default port     |
| Local (Cloudflare)  | `http://127.0.0.1:8787`               | Wrangler default port    |
| Production (Fastly) | `https://mocktioneer.edgecompute.app` | Example deployment       |

## Endpoints Overview

### Auction Endpoints

| Method | Path                                     | Description             |
| ------ | ---------------------------------------- | ----------------------- |
| POST   | [`/openrtb2/auction`](./openrtb-auction) | OpenRTB 2.x bid request |
| POST   | [`/e/dtb/bid`](./aps-bid)                | APS TAM bid request     |
| POST   | [`/adserver/mediate`](./mediation)       | Auction mediation       |

### Asset Endpoints

| Method | Path                                            | Description           |
| ------ | ----------------------------------------------- | --------------------- |
| GET    | [`/static/creatives/{W}x{H}.html`](./creatives) | HTML creative wrapper |
| GET    | [`/static/img/{W}x{H}.svg`](./creatives)        | SVG creative image    |

### Tracking Endpoints

| Method | Path                    | Description          |
| ------ | ----------------------- | -------------------- |
| GET    | [`/pixel`](./tracking)  | Tracking pixel       |
| GET    | [`/click`](./tracking)  | Click landing page   |
| GET    | [`/aps/win`](./aps-win) | APS win notification |

### Utility Endpoints

| Method | Path       | Description                  |
| ------ | ---------- | ---------------------------- |
| GET    | `/`        | Service info page            |
| GET    | `/_/sizes` | Supported sizes with pricing |

## Common Headers

### Request Headers

| Header         | Required   | Description                                  |
| -------------- | ---------- | -------------------------------------------- |
| `Content-Type` | Yes (POST) | Must be `application/json` for POST requests |
| `Host`         | No         | Used to construct creative URLs              |

### Response Headers

All responses include CORS headers (`Access-Control-Allow-Origin: *`, etc.). See [CORS Preflight](#cors-preflight) for details.

## Supported Ad Sizes {#supported-sizes}

Mocktioneer supports these standard IAB sizes with fixed CPM pricing:

| Size    | Name                          | CPM   |
| ------- | ----------------------------- | ----- |
| 970x250 | Billboard                     | $4.20 |
| 970x90  | Large Leaderboard             | $3.80 |
| 300x600 | Half Page                     | $3.50 |
| 160x600 | Wide Skyscraper               | $3.20 |
| 728x90  | Leaderboard                   | $3.00 |
| 320x480 | Mobile Interstitial Portrait  | $2.80 |
| 480x320 | Mobile Interstitial Landscape | $2.80 |
| 336x280 | Large Rectangle               | $2.60 |
| 300x250 | Medium Rectangle              | $2.50 |
| 320x100 | Large Mobile Banner           | $2.20 |
| 468x60  | Banner                        | $2.00 |
| 320x50  | Mobile Leaderboard            | $1.80 |
| 300x50  | Mobile Banner                 | $1.70 |

::: tip Programmatic Access
Use the [`/_/sizes`](#sizes-endpoint) endpoint to get this list programmatically.
:::

Non-standard sizes:

- Return 404 for HTML creative endpoints and 422 for SVG creative endpoints
- Are coerced to 300x250 for auction endpoints
- Are skipped for APS responses

## Error Responses

Errors are returned as JSON:

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Description of the error"
  }
}
```

### HTTP Status Codes

| Code | Meaning                      |
| ---- | ---------------------------- |
| 200  | Success                      |
| 204  | Success (no content)         |
| 400  | Bad request (malformed JSON) |
| 404  | Not found                    |
| 422  | Validation error             |
| 500  | Internal server error        |

## CORS Preflight {#cors-preflight}

All endpoints support OPTIONS requests for CORS preflight and include these headers in responses:

| Header                         | Value                |
| ------------------------------ | -------------------- |
| `Access-Control-Allow-Origin`  | `*`                  |
| `Access-Control-Allow-Methods` | `GET, POST, OPTIONS` |
| `Access-Control-Allow-Headers` | `*, content-type`    |

```bash
curl -X OPTIONS http://127.0.0.1:8787/openrtb2/auction \
  -H "Origin: https://example.com" \
  -H "Access-Control-Request-Method: POST"
# Returns 204 No Content with CORS headers
```

## Sizes Endpoint {#sizes-endpoint}

Returns all supported ad sizes with their CPM values as JSON.

```
GET /_/sizes
```

### Response

```json
{
  "sizes": [
    { "width": 160, "height": 600, "cpm": 3.2 },
    { "width": 300, "height": 50, "cpm": 1.7 },
    { "width": 300, "height": 250, "cpm": 2.5 },
    { "width": 300, "height": 600, "cpm": 3.5 },
    { "width": 320, "height": 50, "cpm": 1.8 },
    { "width": 320, "height": 100, "cpm": 2.2 },
    { "width": 320, "height": 480, "cpm": 2.8 },
    { "width": 336, "height": 280, "cpm": 2.6 },
    { "width": 468, "height": 60, "cpm": 2.0 },
    { "width": 480, "height": 320, "cpm": 2.8 },
    { "width": 728, "height": 90, "cpm": 3.0 },
    { "width": 970, "height": 90, "cpm": 3.8 },
    { "width": 970, "height": 250, "cpm": 4.2 }
  ]
}
```

### Example

```bash
curl http://127.0.0.1:8787/_/sizes | jq .
```

This endpoint is useful for:

- Generating test fixtures
- Keeping external configurations in sync
- Validating supported sizes programmatically
