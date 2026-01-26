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
      },
      "ext": {
        "mocktioneer": {
          "bid": 2.50
        }
      }
    }
  ],
  "ext": {
    "signature": "base64-encoded-signature",
    "kid": "key-id"
  }
}
```

### Request Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | Yes | Request ID |
| `imp` | array | Yes | Array of impressions (min 1) |
| `imp[].id` | string | Yes | Impression ID |
| `imp[].banner` | object | Yes* | Banner object (*or other media type) |
| `imp[].banner.w` | integer | No | Width in pixels |
| `imp[].banner.h` | integer | No | Height in pixels |
| `imp[].banner.format` | array | No | Array of size objects |
| `imp[].ext.mocktioneer.bid` | float | No | Override bid price |
| `site` | object | No | Site information |
| `site.domain` | string | No | Domain for signature verification |

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
          "id": "019abc123-mocktioneer",
          "impid": "imp-1",
          "price": 1.50,
          "adm": "<iframe src=\"//localhost:8787/static/creatives/300x250.html?crid=019abc123-mocktioneer&bid=1.50\" width=\"300\" height=\"250\" frameborder=\"0\" scrolling=\"no\"></iframe>",
          "adomain": ["example.com"],
          "crid": "019abc123-mocktioneer",
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

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Echoed request ID |
| `seatbid` | array | Array of seat bids |
| `seatbid[].seat` | string | Always "mocktioneer" |
| `seatbid[].bid` | array | Array of bids |
| `seatbid[].bid[].id` | string | Unique bid ID (UUIDv7) |
| `seatbid[].bid[].impid` | string | Corresponding impression ID |
| `seatbid[].bid[].price` | float | Bid price in USD |
| `seatbid[].bid[].adm` | string | Ad markup (iframe HTML) |
| `seatbid[].bid[].adomain` | array | Advertiser domains |
| `seatbid[].bid[].crid` | string | Creative ID |
| `seatbid[].bid[].w` | integer | Creative width |
| `seatbid[].bid[].h` | integer | Creative height |
| `seatbid[].bid[].mtype` | integer | Media type (1 = banner) |
| `cur` | string | Currency (USD) |

## Price Override

Override the bid price using the `ext.mocktioneer.bid` field:

```json
{
  "id": "test",
  "imp": [
    {
      "id": "1",
      "banner": { "w": 300, "h": 250 },
      "ext": {
        "mocktioneer": {
          "bid": 5.00
        }
      }
    }
  ]
}
```

The creative will display this bid amount.

## Default Pricing

Without a price override, Mocktioneer uses fixed prices based on size:

| Size | Default CPM |
|------|-------------|
| 970x250 | $4.20 |
| 300x600 | $3.50 |
| 728x90 | $2.00 |
| 300x250 | $2.50 |
| 320x50 | $1.80 |
| Other | $1.50 |

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

### With Price Override

```bash
curl -X POST http://127.0.0.1:8787/openrtb2/auction \
  -H 'Content-Type: application/json' \
  -d '{
    "id": "custom-price",
    "imp": [{
      "id": "1",
      "banner": {"w": 300, "h": 250},
      "ext": {"mocktioneer": {"bid": 10.00}}
    }]
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

- `ext.signature` - Base64-encoded signature
- `ext.kid` - Key ID for signature verification

Verification failures are logged but don't reject the request.
