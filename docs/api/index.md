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

### Edge Cookie Sync Endpoints

| Method | Path                    | Description                         |
| ------ | ----------------------- | ----------------------------------- |
| GET    | [`/sync/start`](./sync) | Initiate pixel sync redirect        |
| GET    | [`/sync/done`](./sync)  | Pixel sync callback (returns pixel) |
| GET    | [`/resolve`](./resolve) | Pull sync identity resolution (S2S) |

### Utility Endpoints

| Method | Path       | Description        |
| ------ | ---------- | ------------------ |
| GET    | `/`        | Service info page  |
| GET    | `/_/sizes` | Supported ad sizes |

## Common Headers

### Request Headers

| Header         | Required | Description                                                                            |
| -------------- | -------- | -------------------------------------------------------------------------------------- |
| `Content-Type` | No       | `application/json` is recommended; JSON is also parsed without it or with `text/plain` |
| `Host`         | No       | Used to construct creative URLs                                                        |

### Response Headers

All matched-route responses include the CORS headers listed under
[CORS Preflight](#cors-preflight), including framework-rendered errors: malformed
JSON or oversized bodies (400), validation failures (422), body-read expiry
(408), config failures (503), and matched asset requests that return 404.
Routing fallback 404/405 responses and normalized-head validation failures do
not include CORS headers. A host parser may also reject a request before
application dispatch.

## Supported Ad Sizes {#supported-sizes}

Mocktioneer supports these standard IAB sizes. All auction bids use a default price of `$0.20` CPM (configurable via `bid_cpm` in mocktioneer.toml).

| Size    | Name                          |
| ------- | ----------------------------- |
| 970x250 | Billboard                     |
| 970x90  | Large Leaderboard             |
| 300x600 | Half Page                     |
| 160x600 | Wide Skyscraper               |
| 728x90  | Leaderboard                   |
| 320x480 | Mobile Interstitial Portrait  |
| 480x320 | Mobile Interstitial Landscape |
| 336x280 | Large Rectangle               |
| 300x250 | Medium Rectangle              |
| 320x100 | Large Mobile Banner           |
| 468x60  | Banner                        |
| 320x50  | Mobile Leaderboard            |
| 300x50  | Mobile Banner                 |

::: tip Programmatic Access
Use the [`/_/sizes`](#sizes-endpoint) endpoint to get this list programmatically.
:::

Non-standard sizes:

- Return 404 for HTML creative endpoints and 422 for SVG creative endpoints
- Are coerced to 300x250 for auction endpoints
- Are skipped for APS responses

## Error Responses

Framework errors are returned as JSON. Some sync/auth handlers intentionally
return empty error responses, as described on their endpoint pages:

```json
{
  "error": {
    "kind": "validation",
    "message": "Description of the error",
    "status": 422
  }
}
```

### HTTP Status Codes

| Code | Meaning                                                                               |
| ---- | ------------------------------------------------------------------------------------- |
| 200  | Success                                                                               |
| 204  | Success (no content)                                                                  |
| 302  | Redirect (sync flow)                                                                  |
| 400  | Malformed JSON/query, body over its cap (`bad_request`), or visible framing ambiguity |
| 401  | Unauthorized (invalid token)                                                          |
| 403  | Forbidden (domain not allowed)                                                        |
| 404  | Route or asset not found                                                              |
| 405  | Method not allowed; `Allow` is sorted and deduplicated                                |
| 408  | Observed inbound body-read deadline expiry                                            |
| 414  | Visible request target over 8192 bytes                                                |
| 422  | Request validator failure (`validation`)                                              |
| 431  | More than 100 normalized headers or over 64 KiB of normalized headers                 |
| 500  | Internal or config-store failure                                                      |
| 503  | Config unavailable, out of date, or observed config-read deadline expiry              |

### Request Limits and Deadlines

`POST /openrtb2/auction` and `POST /e/dtb/bid` accept at most **1 MiB** of JSON.
`POST /adserver/mediate` keeps the framework default of **8 MiB**. At the audited
EdgeZero revision `f6c708ccb5be19ba9fa92575db3c08c0f17e0d72`, overflow returns
400 with `error.kind: "bad_request"` and `error.message: "request body too large"`.
Malformed JSON is 400; successfully parsed input that fails a validator is 422.

All three POST routes have a **10-second body-read budget** measured from ingress
start. Other routes use the default 30-second budget. An observed body-read
expiry returns 408; enforcement is Native on Axum and BestEffort on the edge
adapters. A read budget does not set a total handler or response-delivery timeout.

Normalized-head checks run before routing and CORS. The target limit counts the
absolute URL on Fastly and Cloudflare. These checks apply to the head visible to
EdgeZero: raw ingress framing validation is Unsupported on every adapter. Hosts
can reject or normalize input first; for example, Axum's Hyper parser can merge
identical `Content-Length` fields or remove `Content-Length` when
`Transfer-Encoding` is present. Those raw inputs need not produce an application 400. See [Axum](../guide/adapters/axum#http-transport) for its HTTP/1 transport limits.

### Bid-Route Config Errors

Both bid handlers extract `AppConfig` **before reading the JSON body**. An
unavailable or invalid config therefore takes precedence over body parsing and
the 1 MiB cap. Config extraction has a two-second deadline, enforced BestEffort
on every adapter.

| Config failure                                                                                      | Status | `error.kind`          | `Retry-After` |
| --------------------------------------------------------------------------------------------------- | ------ | --------------------- | ------------- |
| Missing blob, wrong data type/deserialization failure, or config validation failure                 | 503    | `config_out_of_date`  | `60`          |
| Unavailable backend, including missing Cloudflare KV binding or unreadable/malformed Axum snapshot  | 503    | `service_unavailable` | Absent        |
| Observed config-read deadline expiry                                                                | 503    | `service_unavailable` | Absent        |
| No registered config store, malformed envelope, integrity mismatch, or unsupported envelope version | 500    | `internal`            | Absent        |
| Backend read failure, including Fastly store open failure                                           | 500    | `internal`            | Absent        |
| Config value over 8 MiB or JSON structure over depth 64 / 8192 nodes                                | 500    | `internal`            | Absent        |
| Invalid store key (unreachable with Mocktioneer's fixed key)                                        | 400    | `bad_request`         | Absent        |

An oversized Axum snapshot fails server startup. Ordinary `EdgeError::internal`
errors expose the fixed message `internal server error`; config-extraction 500s
retain their specific public diagnostic messages with sensitive details redacted.

### Response Framing

Ordinary empty buffered responses, including sync redirects and auth failures,
carry `Content-Length: 0`. Egress strips hop-by-hop headers and canonicalizes
lengths. No `Content-Length` is emitted for 1xx or 204; 205 may carry zero. HEAD
and 304 suppress bodies while preserving a permitted representation length
rather than replacing it with zero. Provider delivery can still add its own
transport framing.

## CORS Preflight {#cors-preflight}

Registered OPTIONS routes return 204 No Content with these CORS headers.
Matched-route responses, including errors, carry the same headers:

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

Returns all supported standard ad sizes as JSON.

```
GET /_/sizes
```

### Response

```json
{
  "sizes": [
    { "width": 160, "height": 600 },
    { "width": 300, "height": 50 },
    { "width": 300, "height": 250 },
    { "width": 300, "height": 600 },
    { "width": 320, "height": 50 },
    { "width": 320, "height": 100 },
    { "width": 320, "height": 480 },
    { "width": 336, "height": 280 },
    { "width": 468, "height": 60 },
    { "width": 480, "height": 320 },
    { "width": 728, "height": 90 },
    { "width": 970, "height": 90 },
    { "width": 970, "height": 250 }
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
