# Prebid Server Integration

Mocktioneer can be used as a bidder in Prebid Server for server-side header bidding testing.

::: warning Fork Required
The Mocktioneer adapter is not yet merged into upstream Prebid Server. Use the Stackpop fork:

**[github.com/stackpop/prebid-server](https://github.com/stackpop/prebid-server)**
:::

## Configuration

### Host Configuration

Add Mocktioneer to your Prebid Server host configuration:

```yaml
# config.yaml
adapters:
  mocktioneer:
    enabled: true
    endpoint: https://mocktioneer.edgecompute.app/openrtb2/auction
```

### Local Development

For local testing, point to your Mocktioneer instance:

```yaml
adapters:
  mocktioneer:
    enabled: true
    endpoint: http://localhost:8787/openrtb2/auction
```

## Request Format

### Basic Request

```json
{
  "id": "test-request",
  "imp": [
    {
      "id": "imp-1",
      "banner": {
        "w": 300,
        "h": 250
      },
      "ext": {
        "prebid": {
          "bidder": {
            "mocktioneer": {}
          }
        }
      }
    }
  ],
  "site": {
    "page": "https://example.com/article"
  }
}
```

### With Price Override

```json
{
  "id": "test-request",
  "imp": [
    {
      "id": "imp-1",
      "banner": {
        "w": 300,
        "h": 250
      },
      "ext": {
        "prebid": {
          "bidder": {
            "mocktioneer": {
              "bid": 5.0
            }
          }
        }
      }
    }
  ]
}
```

### Custom Endpoint Per Request

Override the endpoint for specific requests:

```json
{
  "imp": [
    {
      "ext": {
        "bidder": {
          "endpoint": "http://custom-mocktioneer:8787/openrtb2/auction"
        },
        "prebid": {
          "bidder": {
            "mocktioneer": {
              "bid": 2.5
            }
          }
        }
      }
    }
  ]
}
```

## Parameters

| Parameter  | Location                              | Type   | Description               |
| ---------- | ------------------------------------- | ------ | ------------------------- |
| `endpoint` | `imp[].ext.bidder`                    | string | Override auction endpoint |
| `bid`      | `imp[].ext.prebid.bidder.mocktioneer` | float  | Override bid price        |

## Response Handling

Prebid Server processes Mocktioneer responses like any other bidder:

```json
{
  "seatbid": [
    {
      "seat": "mocktioneer",
      "bid": [
        {
          "id": "019abc-mocktioneer",
          "impid": "imp-1",
          "price": 2.5,
          "adm": "<iframe>...</iframe>",
          "crid": "019abc-mocktioneer",
          "w": 300,
          "h": 250
        }
      ]
    }
  ]
}
```

## Testing with cURL

### Direct to Prebid Server

```bash
curl -X POST http://localhost:8000/openrtb2/auction \
  -H 'Content-Type: application/json' \
  -d '{
    "id": "pbs-test",
    "imp": [{
      "id": "1",
      "banner": {"w": 300, "h": 250},
      "ext": {
        "prebid": {
          "bidder": {
            "mocktioneer": {"bid": 3.00}
          }
        }
      }
    }],
    "site": {"page": "https://example.com"}
  }' | jq .
```

### Direct to Mocktioneer

Test the endpoint directly:

```bash
curl -X POST http://localhost:8787/openrtb2/auction \
  -H 'Content-Type: application/json' \
  -d '{
    "id": "direct-test",
    "imp": [{
      "id": "1",
      "banner": {"w": 300, "h": 250},
      "ext": {"mocktioneer": {"bid": 2.50}}
    }]
  }' | jq .
```

## Multiple Impressions

```json
{
  "id": "multi-imp",
  "imp": [
    {
      "id": "header",
      "banner": { "w": 728, "h": 90 },
      "ext": {
        "prebid": {
          "bidder": {
            "mocktioneer": { "bid": 2.0 }
          }
        }
      }
    },
    {
      "id": "sidebar",
      "banner": { "w": 300, "h": 250 },
      "ext": {
        "prebid": {
          "bidder": {
            "mocktioneer": { "bid": 2.5 }
          }
        }
      }
    },
    {
      "id": "footer",
      "banner": { "w": 970, "h": 250 },
      "ext": {
        "prebid": {
          "bidder": {
            "mocktioneer": { "bid": 4.0 }
          }
        }
      }
    }
  ]
}
```

## Bidder Info

If implementing a proper Prebid Server adapter, include bidder info:

```yaml
# static/bidder-info/mocktioneer.yaml
maintainer:
  email: maintainer@example.com
capabilities:
  app:
    mediaTypes:
      - banner
  site:
    mediaTypes:
      - banner
```

## Docker Compose Setup

::: details Docker Compose Example (click to expand)

```yaml
# docker-compose.yml
services:
  prebid-server:
    image: prebid/prebid-server
    ports: ['8000:8000']
    volumes: ['./pbs-config.yaml:/config.yaml']
    environment: { PBS_CONFIG_FILE: /config.yaml }
    depends_on: [mocktioneer]

  mocktioneer:
    image: mocktioneer:latest
    ports: ['8787:8787']
```

```yaml
# pbs-config.yaml
adapters:
  mocktioneer:
    enabled: true
    endpoint: http://mocktioneer:8787/openrtb2/auction
```

:::

## Testing Scenarios

### Comparing Bidders

Include Mocktioneer alongside real bidders:

```json
{
  "imp": [
    {
      "ext": {
        "prebid": {
          "bidder": {
            "mocktioneer": { "bid": 2.0 },
            "appnexus": { "placementId": "12345" }
          }
        }
      }
    }
  ]
}
```

### Price Floor Testing

Test floor enforcement:

```json
{
  "imp": [
    {
      "bidfloor": 3.0,
      "ext": {
        "prebid": {
          "bidder": {
            "mocktioneer": { "bid": 2.5 } // Below floor
          }
        }
      }
    }
  ]
}
```

### Timeout Testing

Mocktioneer responds instantly, making it ideal for timeout testing:

```json
{
  "tmax": 100, // 100ms timeout
  "imp": [
    {
      "ext": {
        "prebid": {
          "bidder": {
            "mocktioneer": {},
            "slow-bidder": {} // Compare response times
          }
        }
      }
    }
  ]
}
```

## Troubleshooting

### Bidder Not Found

Ensure Mocktioneer is enabled in config:

```yaml
adapters:
  mocktioneer:
    enabled: true # Must be true
```

### Connection Refused

Check Mocktioneer is running and accessible from Prebid Server:

```bash
# From PBS container
curl http://mocktioneer:8787/
```

### No Bids Returned

1. Check impression has `banner` media type
2. Verify size is standard
3. Check PBS logs for errors

### Invalid Response Format

Mocktioneer returns standard OpenRTB 2.x - check PBS adapter compatibility.

## Monitoring

### Prebid Server Metrics

PBS exposes metrics for bidder performance:

- `adapter_bids_received{adapter="mocktioneer"}`
- `adapter_request_time{adapter="mocktioneer"}`
- `adapter_errors{adapter="mocktioneer"}`

### Mocktioneer Logs

Check Mocktioneer logs for request details:

```
auction id=pbs-123, imps=3
```
