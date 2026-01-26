# Mediation

The `/adserver/mediate` endpoint performs auction mediation, selecting winning bids from multiple bidder responses.

## Endpoint

```
POST /adserver/mediate
Content-Type: application/json
```

## Request Format

```json
{
  "id": "auction-123",
  "imp": [
    {
      "id": "imp-1",
      "banner": {
        "w": 300,
        "h": 250
      }
    }
  ],
  "ext": {
    "bidder_responses": [
      {
        "bidder": "bidder-a",
        "seatbid": [
          {
            "seat": "bidder-a",
            "bid": [
              {
                "id": "bid-1",
                "impid": "imp-1",
                "price": 2.50,
                "adm": "<creative html>",
                "w": 300,
                "h": 250
              }
            ]
          }
        ]
      },
      {
        "bidder": "bidder-b",
        "seatbid": [
          {
            "seat": "bidder-b",
            "bid": [
              {
                "id": "bid-2",
                "impid": "imp-1",
                "price": 3.00,
                "adm": "<creative html>",
                "w": 300,
                "h": 250
              }
            ]
          }
        ]
      }
    ]
  }
}
```

### Request Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | Yes | Auction ID |
| `imp` | array | Yes | Array of impressions |
| `imp[].id` | string | Yes | Impression ID |
| `imp[].banner` | object | No | Banner object |
| `ext.bidder_responses` | array | Yes | Array of bidder responses |
| `ext.bidder_responses[].bidder` | string | Yes | Bidder identifier |
| `ext.bidder_responses[].seatbid` | array | Yes | Standard OpenRTB seatbid array |

## Response Format

The response contains the winning bids for each impression:

```json
{
  "id": "auction-123",
  "seatbid": [
    {
      "seat": "bidder-b",
      "bid": [
        {
          "id": "bid-2",
          "impid": "imp-1",
          "price": 3.00,
          "adm": "<creative html>",
          "w": 300,
          "h": 250
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
| `id` | string | Echoed auction ID |
| `seatbid` | array | Winning bids grouped by seat |
| `seatbid[].seat` | string | Winning bidder |
| `seatbid[].bid` | array | Winning bids |
| `cur` | string | Currency (USD) |

## Mediation Logic

The mediation process:

1. Collects all bids from all bidder responses
2. Groups bids by impression ID
3. Selects the highest-priced bid for each impression
4. Returns the winning bids in OpenRTB format

### Winner Selection

For each impression:
- All bids targeting that impression are compared
- The bid with the highest `price` wins
- Ties are resolved by first bid received

## Examples

### cURL

```bash
curl -X POST http://127.0.0.1:8787/adserver/mediate \
  -H 'Content-Type: application/json' \
  -d '{
    "id": "test-auction",
    "imp": [{"id": "imp-1", "banner": {"w": 300, "h": 250}}],
    "ext": {
      "bidder_responses": [
        {
          "bidder": "mocktioneer",
          "seatbid": [{
            "seat": "mocktioneer",
            "bid": [{
              "id": "m-1",
              "impid": "imp-1",
              "price": 2.00,
              "adm": "<mock creative>"
            }]
          }]
        },
        {
          "bidder": "other-bidder",
          "seatbid": [{
            "seat": "other-bidder",
            "bid": [{
              "id": "o-1",
              "impid": "imp-1",
              "price": 2.50,
              "adm": "<other creative>"
            }]
          }]
        }
      ]
    }
  }' | jq .
```

### Multiple Impressions

```bash
curl -X POST http://127.0.0.1:8787/adserver/mediate \
  -H 'Content-Type: application/json' \
  -d '{
    "id": "multi-imp-auction",
    "imp": [
      {"id": "imp-1", "banner": {"w": 300, "h": 250}},
      {"id": "imp-2", "banner": {"w": 728, "h": 90}}
    ],
    "ext": {
      "bidder_responses": [
        {
          "bidder": "bidder-a",
          "seatbid": [{
            "seat": "bidder-a",
            "bid": [
              {"id": "a-1", "impid": "imp-1", "price": 2.00, "adm": "..."},
              {"id": "a-2", "impid": "imp-2", "price": 1.50, "adm": "..."}
            ]
          }]
        },
        {
          "bidder": "bidder-b",
          "seatbid": [{
            "seat": "bidder-b",
            "bid": [
              {"id": "b-1", "impid": "imp-1", "price": 1.80, "adm": "..."},
              {"id": "b-2", "impid": "imp-2", "price": 2.00, "adm": "..."}
            ]
          }]
        }
      ]
    }
  }' | jq .
```

In this example:
- `imp-1` winner: bidder-a with $2.00
- `imp-2` winner: bidder-b with $2.00

## Error Responses

### Missing Bidder Responses (400)

```json
{
  "error": {
    "code": "BAD_REQUEST",
    "message": "ext.bidder_responses is required"
  }
}
```

### Empty Impressions (422)

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "imp: must have at least 1 item"
  }
}
```

## Use Cases

### Testing Ad Server Logic

Use mediation to test your ad server's winner selection:

1. Send multiple mock bid responses
2. Verify correct winner selection
3. Check response format handling

### Prebid Server Testing

Simulate Prebid Server auction mediation:

1. Collect responses from mock bidders
2. Send to mediation endpoint
3. Verify winning bid selection

### Price Floor Testing

Test price floor logic by including bids below and above thresholds:

```json
{
  "ext": {
    "bidder_responses": [
      {"bidder": "a", "seatbid": [{"bid": [{"price": 0.50}]}]},
      {"bidder": "b", "seatbid": [{"bid": [{"price": 1.00}]}]},
      {"bidder": "c", "seatbid": [{"bid": [{"price": 2.00}]}]}
    ]
  }
}
```

The highest bid wins regardless of floor (floor enforcement is external).
