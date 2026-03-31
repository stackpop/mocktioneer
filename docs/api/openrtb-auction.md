# OpenRTB Auction

The `/openrtb2/auction` endpoint accepts OpenRTB 2.x bid requests and returns deterministic bid responses.

## Endpoint

```
POST /openrtb2/auction
Content-Type: application/json
```

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
      "request_host": "publisher.example",
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
          "adm": "<iframe src=\"//localhost:8787/static/creatives/300x250.html?crid=mocktioneer-imp-1\" width=\"300\" height=\"250\" frameborder=\"0\" scrolling=\"no\"></iframe>",
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

Mocktioneer returns a fixed bid price of `$0.20` CPM for auction responses.

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

### Missing Impressions (422)

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "imp: must have at least 1 item"
  }
}
```

### Missing Media Type (422)

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "imp[0]: must have banner, video, or native"
  }
}
```

### Invalid JSON (400)

```json
{
  "error": {
    "code": "PARSE_ERROR",
    "message": "expected value at line 1 column 1"
  }
}
```

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

The JWKS is fetched from `https://{site.domain}/.well-known/trusted-server.json`. Verification failures are logged but don't reject the request.
