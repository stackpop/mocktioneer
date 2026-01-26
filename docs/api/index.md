# API Reference

Mocktioneer exposes several HTTP endpoints for bid requests, creative serving, and tracking.

## Base URL

| Environment | URL |
|-------------|-----|
| Local (Axum) | `http://127.0.0.1:8787` |
| Local (Fastly) | `http://127.0.0.1:7676` |
| Production | `https://mocktioneer.edgecompute.app` |

## Endpoints Overview

### Auction Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | [`/openrtb2/auction`](./openrtb-auction) | OpenRTB 2.x bid request |
| POST | [`/e/dtb/bid`](./aps-bid) | APS TAM bid request |
| POST | [`/adserver/mediate`](./mediation) | Auction mediation |

### Asset Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | [`/static/creatives/{W}x{H}.html`](./creatives) | HTML creative wrapper |
| GET | [`/static/img/{W}x{H}.svg`](./creatives) | SVG creative image |

### Tracking Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | [`/pixel`](./tracking) | Tracking pixel |
| GET | [`/click`](./tracking) | Click landing page |
| GET | [`/aps/win`](./aps-win) | APS win notification |

### Utility Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | Service info page |

## Common Headers

### Request Headers

| Header | Required | Description |
|--------|----------|-------------|
| `Content-Type` | Yes (POST) | Must be `application/json` for POST requests |
| `Host` | No | Used to construct creative URLs |

### Response Headers

All responses include CORS headers:

```
Access-Control-Allow-Origin: *
Access-Control-Allow-Methods: GET, POST, OPTIONS
Access-Control-Allow-Headers: *, content-type
```

## Supported Ad Sizes

Mocktioneer supports these standard IAB sizes:

| Size | Name |
|------|------|
| 300x250 | Medium Rectangle |
| 320x50 | Mobile Leaderboard |
| 728x90 | Leaderboard |
| 160x600 | Wide Skyscraper |
| 300x50 | Mobile Banner |
| 300x600 | Half Page |
| 970x250 | Billboard |
| 468x60 | Full Banner |
| 336x280 | Large Rectangle |
| 320x100 | Large Mobile Banner |

Non-standard sizes:
- Return 404 for static asset endpoints
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

| Code | Meaning |
|------|---------|
| 200 | Success |
| 204 | Success (no content) |
| 400 | Bad request (malformed JSON) |
| 404 | Not found |
| 422 | Validation error |
| 500 | Internal server error |

## CORS Preflight

All endpoints support OPTIONS requests for CORS preflight:

```bash
curl -X OPTIONS http://127.0.0.1:8787/openrtb2/auction \
  -H "Origin: https://example.com" \
  -H "Access-Control-Request-Method: POST"
```

Response:
```
HTTP/1.1 204 No Content
Allow: GET, POST, OPTIONS
Access-Control-Allow-Origin: *
Access-Control-Allow-Methods: GET, POST, OPTIONS
Access-Control-Allow-Headers: *, content-type
```
