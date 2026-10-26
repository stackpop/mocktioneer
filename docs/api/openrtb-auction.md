# OpenRTB Auction

The `/openrtb2/auction` endpoint accepts OpenRTB 2.x bid requests and returns deterministic bid responses.

::: warning Requires a pushed config
This endpoint reads `bid_cpm` from the typed app config via the fail-loud
`AppConfig` extractor, so it returns 503 `config_out_of_date` with `Retry-After: 60` until the
config blob has been pushed once for the target adapter:

```bash
cargo run -p mocktioneer-cli -- config push --adapter axum --yes
```

See [Configuration › Typed App Config](/guide/configuration#typed-app-config)
for per-adapter details. (Static, `/pixel`, and `/_/sizes` endpoints work
without it.)
:::

## Endpoint

```
POST /openrtb2/auction
Content-Type: application/json
```

Requests are capped at **1 MiB** with a **10-second body-read budget**. JSON
is accepted without a `Content-Type` header and with `text/plain`, although
`application/json` is recommended. `AppConfig` is extracted before the body, so
config failures take precedence over parsing and size errors. See
[request limits and config errors](./#request-limits-and-deadlines).

## Request Format

### Minimal Request

```json
{
  "id": "request-123",
  "imp": [
    {
      "id": "imp-1",
      "banner": {
        "w": 300,
        "h": 250
      }
    }
  ]
}
```

### Full Request

```json
{
  "id": "request-123",
  "site": {
    "domain": "example.com",
    "page": "https://example.com/article"
  },
  "imp": [
    {
      "id": "imp-1",
      "banner": {
        "w": 300,
        "h": 250,
        "format": [
          { "w": 300, "h": 250 },
          { "w": 320, "h": 50 }
        ]
      }
    }
  ],
  "ext": {
    "trusted_server": {
      "version": "1.1",
      "signature": "base64-encoded-signature",
      "kid": "key-id",
      "request_host": "example.com",
      "request_scheme": "https",
      "ts": 1706900000000
    }
  }
}
```

### Request Fields

| Field                               | Type    | Required | Description                           |
| ----------------------------------- | ------- | -------- | ------------------------------------- |
| `id`                                | string  | Yes      | Request ID                            |
| `imp`                               | array   | Yes      | Array of impressions (min 1)          |
| `imp[].id`                          | string  | Yes      | Impression ID                         |
| `imp[].banner`                      | object  | Yes\*    | Banner object (\*or other media type) |
| `imp[].banner.w`                    | integer | No       | Width in pixels                       |
| `imp[].banner.h`                    | integer | No       | Height in pixels                      |
| `imp[].banner.format`               | array   | No       | Array of size objects                 |
| `ext.trusted_server.version`        | string  | No       | Signing protocol version (`1.1`)      |
| `ext.trusted_server.signature`      | string  | No       | Signature for canonical payload       |
| `ext.trusted_server.kid`            | string  | No       | Key ID used for signature             |
| `ext.trusted_server.request_host`   | string  | No       | Host included in signed payload       |
| `ext.trusted_server.request_scheme` | string  | No       | Scheme included in signed payload     |
| `ext.trusted_server.ts`             | integer | No       | Unix timestamp (milliseconds)         |
| `site`                              | object  | No       | Site information                      |
| `site.domain`                       | string  | No       | Domain for signature verification     |

### Size Resolution

Size is determined in this order:

1. `imp[].banner.w` and `imp[].banner.h`
2. First entry in `imp[].banner.format[]`
3. Default: 300x250

## Response Format

```json
{
  "id": "request-123",
  "seatbid": [
    {
      "seat": "mocktioneer",
      "bid": [
        {
          "id": "019abc123",
          "impid": "imp-1",
          "price": 0.2,
          "adm": "<iframe src=\"//localhost:8787/static/creatives/300x250.html?crid=mocktioneer-imp-1&sig=not_present\" width=\"300\" height=\"250\" frameborder=\"0\" scrolling=\"no\"></iframe>",
          "adomain": ["example.com"],
          "crid": "mocktioneer-imp-1",
          "w": 300,
          "h": 250,
          "mtype": 1
        }
      ]
    }
  ],
  "cur": "USD"
}
```

### Response Fields

| Field                     | Type    | Description                 |
| ------------------------- | ------- | --------------------------- |
| `id`                      | string  | Echoed request ID           |
| `seatbid`                 | array   | Array of seat bids          |
| `seatbid[].seat`          | string  | Always "mocktioneer"        |
| `seatbid[].bid`           | array   | Array of bids               |
| `seatbid[].bid[].id`      | string  | Unique bid ID (UUIDv7)      |
| `seatbid[].bid[].impid`   | string  | Corresponding impression ID |
| `seatbid[].bid[].price`   | float   | Bid price in USD            |
| `seatbid[].bid[].adm`     | string  | Ad markup (iframe HTML)     |
| `seatbid[].bid[].adomain` | array   | Advertiser domains          |
| `seatbid[].bid[].crid`    | string  | Creative ID                 |
| `seatbid[].bid[].w`       | integer | Creative width              |
| `seatbid[].bid[].h`       | integer | Creative height             |
| `seatbid[].bid[].mtype`   | integer | Media type (1 = banner)     |
| `cur`                     | string  | Currency (USD)              |

## Pricing

Mocktioneer returns a default bid price of `$0.20` CPM for auction responses (configurable via `bid_cpm` in mocktioneer.toml).

If `imp[].ext.mocktioneer.bid` is present, it is ignored.

## Examples

### cURL

```bash
curl -X POST http://127.0.0.1:8787/openrtb2/auction \
  -H 'Content-Type: application/json' \
  -d '{
    "id": "test-request",
    "imp": [{
      "id": "imp-1",
      "banner": {"w": 300, "h": 250}
    }]
  }' | jq .
```

### Multiple Impressions

```bash
curl -X POST http://127.0.0.1:8787/openrtb2/auction \
  -H 'Content-Type: application/json' \
  -d '{
    "id": "multi-imp",
    "imp": [
      {"id": "1", "banner": {"w": 300, "h": 250}},
      {"id": "2", "banner": {"w": 728, "h": 90}},
      {"id": "3", "banner": {"w": 320, "h": 50}}
    ]
  }' | jq .
```

## Error Responses

| Failure                                                           | Status | `error.kind`        |
| ----------------------------------------------------------------- | ------ | ------------------- |
| Empty impressions or an impression without a supported media type | 422    | `validation`        |
| Malformed JSON or missing required JSON fields                    | 400    | `bad_request`       |
| Body over 1 MiB                                                   | 413    | `payload_too_large` |
| Observed body-read deadline expiry                                | 408    | `request_timeout`   |

An oversized body returns 413 without `Retry-After`:

```json
{
  "error": {
    "kind": "payload_too_large",
    "message": "request body too large",
    "status": 413
  }
}
```

Config failures have their own [503/500 response rules](./#bid-route-config-errors).

## Request Signature Verification

Mocktioneer supports optional request signature verification. When `site.domain` is present, it attempts to verify the request signature using:

- `ext.trusted_server.version` - Signing protocol version (`1.1`)
- `ext.trusted_server.signature` - Base64 URL-safe Ed25519 signature
- `ext.trusted_server.kid` - Key ID for signature verification
- `ext.trusted_server.request_host` - Host bound into the signed payload
- `ext.trusted_server.request_scheme` - Scheme bound into the signed payload
- `ext.trusted_server.ts` - Unix timestamp in milliseconds

The signed payload is canonical JSON:

```json
{
  "version": "1.1",
  "kid": "...",
  "host": "...",
  "scheme": "https",
  "id": "...",
  "ts": 1706900000000
}
```

The JWKS is fetched from `https://{site.domain}/.well-known/trusted-server.json`
without following redirects, with a five-second timeout and a 64 KiB final
buffered-body cap after recognized gzip, Brotli, or deflate decoding. Axum and
Fastly default `Accept-Encoding` to `identity` when it is absent. A valid decoded
JWKS can therefore verify successfully even when its encoded form was previously
rejected.

Buffered send-time framing, body, size, decode, and timeout failures take
precedence over a non-200 status. Transport/body/decode failures use the
`HTTP error: JWKS fetch failed:` prefix, while size failures keep
`HTTP error: JWKS response body exceeds 65536 byte limit`. Internal details are
redacted. Verification failures are logged and included in creative metadata;
the auction still serves bids. See the [JWKS policy and cache details](/integrations/trusted-server#signature-verification-and-jwks).
