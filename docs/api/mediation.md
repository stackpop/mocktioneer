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
        "bids": [
          {
            "imp_id": "imp-1",
            "price": 2.5,
            "adm": "<creative html>",
            "w": 300,
            "h": 250,
            "crid": "creative-a",
            "adomain": ["example.com"]
          }
        ]
      },
      {
        "bidder": "bidder-b",
        "bids": [
          {
            "imp_id": "imp-1",
            "price": 3.0,
            "w": 300,
            "h": 250
          }
        ]
      }
    ],
    "config": {
      "price_floor": 1.5
    }
  }
}
```

### Request Fields

| Field                                   | Type    | Required | Description                                        |
| --------------------------------------- | ------- | -------- | -------------------------------------------------- |
| `id`                                    | string  | Yes      | Auction ID                                         |
| `imp`                                   | array   | Yes      | Array of impressions                               |
| `imp[].id`                              | string  | Yes      | Impression ID                                      |
| `imp[].banner`                          | object  | No       | Banner object                                      |
| `ext.bidder_responses`                  | array   | Yes      | Array of bidder responses                          |
| `ext.bidder_responses[].bidder`         | string  | Yes      | Bidder identifier                                  |
| `ext.bidder_responses[].bids`           | array   | Yes      | Bids from this bidder                              |
| `ext.bidder_responses[].bids[].imp_id`  | string  | Yes      | Impression ID this bid targets                     |
| `ext.bidder_responses[].bids[].price`   | float   | Yes      | Bid price (CPM)                                    |
| `ext.bidder_responses[].bids[].w`       | integer | Yes      | Creative width                                     |
| `ext.bidder_responses[].bids[].h`       | integer | Yes      | Creative height                                    |
| `ext.bidder_responses[].bids[].adm`     | string  | No       | Creative markup; if omitted, Mocktioneer generates |
| `ext.bidder_responses[].bids[].crid`    | string  | No       | Creative ID                                        |
| `ext.bidder_responses[].bids[].adomain` | array   | No       | Advertiser domains                                 |
| `ext.config.price_floor`                | float   | No       | Minimum acceptable bid price (CPM)                 |

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
          "id": "019abc123",
          "impid": "imp-1",
          "price": 3.0,
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

| Field            | Type   | Description                  |
| ---------------- | ------ | ---------------------------- |
| `id`             | string | Echoed auction ID            |
| `seatbid`        | array  | Winning bids grouped by seat |
| `seatbid[].seat` | string | Winning bidder               |
| `seatbid[].bid`  | array  | Winning bids                 |
| `cur`            | string | Currency (USD)               |

## Mediation Logic

The mediation process:

1. Collects all bids grouped by impression ID
2. Applies `price_floor` if provided
3. Selects the highest-priced bid for each impression
4. Generates creative HTML if the winning bid omits `adm`
5. Returns the winning bids in OpenRTB format

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
          "bids": [
            {"imp_id": "imp-1", "price": 2.00, "w": 300, "h": 250, "adm": "<mock creative>"}
          ]
        },
        {
          "bidder": "other-bidder",
          "bids": [
            {"imp_id": "imp-1", "price": 2.50, "w": 300, "h": 250}
          ]
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
          "bids": [
            {"imp_id": "imp-1", "price": 2.00, "w": 300, "h": 250},
            {"imp_id": "imp-2", "price": 1.50, "w": 728, "h": 90}
          ]
        },
        {
          "bidder": "bidder-b",
          "bids": [
            {"imp_id": "imp-1", "price": 1.80, "w": 300, "h": 250},
            {"imp_id": "imp-2", "price": 2.00, "w": 728, "h": 90}
          ]
        }
      ]
    }
  }' | jq .
```

In this example:

- `imp-1` winner: bidder-a with $2.00
- `imp-2` winner: bidder-b with $2.00

### Price Floor

```bash
curl -X POST http://127.0.0.1:8787/adserver/mediate \
  -H 'Content-Type: application/json' \
  -d '{
    "id": "floor-auction",
    "imp": [{"id": "imp-1", "banner": {"w": 300, "h": 250}}],
    "ext": {
      "config": {"price_floor": 1.50},
      "bidder_responses": [
        {"bidder": "a", "bids": [{"imp_id": "imp-1", "price": 0.50, "w": 300, "h": 250}]},
        {"bidder": "b", "bids": [{"imp_id": "imp-1", "price": 1.75, "w": 300, "h": 250}]}
      ]
    }
  }' | jq .
```

## Error Responses

### Malformed or Missing Fields (400)

```json
{
  "error": {
    "code": "BAD_REQUEST",
    "message": "..."
  }
}
```

### Validation Errors (422)

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "..."
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

Validate price floor logic by including bids below and above the floor.
