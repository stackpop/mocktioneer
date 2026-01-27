# APS TAM Bid

The `/e/dtb/bid` endpoint accepts Amazon Publisher Services (APS) Transparent Ad Marketplace bid requests and returns bids in APS format.

## Endpoint

```
POST /e/dtb/bid
Content-Type: application/json
```

## Request Format

```json
{
  "pubId": "1234",
  "slots": [
    {
      "slotID": "header-banner",
      "slotName": "header-banner",
      "sizes": [
        [728, 90],
        [970, 250]
      ]
    }
  ],
  "pageUrl": "https://example.com/article",
  "ua": "Mozilla/5.0...",
  "timeout": 800
}
```

### Request Fields

| Field              | Type    | Required | Description                      |
| ------------------ | ------- | -------- | -------------------------------- |
| `pubId`            | string  | Yes      | Publisher ID                     |
| `slots`            | array   | Yes      | Array of ad slots (min 1)        |
| `slots[].slotID`   | string  | Yes      | Unique slot identifier           |
| `slots[].slotName` | string  | No       | Slot name                        |
| `slots[].sizes`    | array   | Yes      | Array of `[width, height]` pairs |
| `pageUrl`          | string  | No       | Page URL                         |
| `ua`               | string  | No       | User agent                       |
| `timeout`          | integer | No       | Request timeout in ms            |

## Response Format

The response matches the real Amazon APS API format with a `contextual` wrapper:

```json
{
  "contextual": {
    "slots": [
      {
        "slotID": "header-banner",
        "size": "970x250",
        "crid": "019b7f82e8de7e13-mocktioneer",
        "mediaType": "d",
        "fif": "1",
        "targeting": ["amzniid", "amznp", "amznsz", "amznbid", "amznactt"],
        "meta": ["slotID", "mediaType", "size"],
        "amzniid": "019b7f82e8de7e139d6d6a593171e7a0",
        "amznbid": "NC4yMA==",
        "amznp": "NC4yMA==",
        "amznsz": "970x250",
        "amznactt": "OPEN"
      }
    ],
    "host": "https://mocktioneer.edgecompute.app",
    "status": "ok",
    "cfe": true,
    "ev": true,
    "cfn": "bao-csm/direct/csm_othersv6.js",
    "cb": "6"
  }
}
```

### Response Fields

| Field                          | Type   | Description                       |
| ------------------------------ | ------ | --------------------------------- |
| `contextual`                   | object | Wrapper object (matches real APS) |
| `contextual.slots`             | array  | Array of bid responses            |
| `contextual.slots[].slotID`    | string | Slot identifier                   |
| `contextual.slots[].size`      | string | Selected size (e.g., "970x250")   |
| `contextual.slots[].crid`      | string | Creative ID                       |
| `contextual.slots[].mediaType` | string | Media type ("d" = display)        |
| `contextual.slots[].fif`       | string | Fill indicator ("1" = filled)     |
| `contextual.slots[].targeting` | array  | Targeting key names               |
| `contextual.slots[].meta`      | array  | Metadata field names              |
| `contextual.slots[].amzniid`   | string | Amazon impression ID              |
| `contextual.slots[].amznbid`   | string | Base64-encoded bid price          |
| `contextual.slots[].amznp`     | string | Base64-encoded price              |
| `contextual.slots[].amznsz`    | string | Size string                       |
| `contextual.slots[].amznactt`  | string | Account type ("OPEN")             |
| `contextual.host`              | string | Service host                      |
| `contextual.status`            | string | Status ("ok")                     |

## Size Selection

When multiple sizes are provided, Mocktioneer selects the size with the highest CPM. See the [complete pricing table](/api/#supported-sizes) for all supported sizes and their CPM values.

Non-standard sizes are skipped (no bid returned for that slot).

## Price Encoding

::: warning Important
The price encoding differs between real APS and Mocktioneer:

- **Real Amazon APS**: Uses proprietary encoding that only Amazon and trusted partners can decode
- **Mocktioneer**: Uses Base64 encoding for testing purposes
  :::

Decode Mocktioneer prices:

```bash
echo "Mi41MA==" | base64 -d
# Output: 2.50
```

## Examples

### cURL

```bash
curl -X POST http://127.0.0.1:8787/e/dtb/bid \
  -H 'Content-Type: application/json' \
  -d '{
    "pubId": "1234",
    "slots": [{
      "slotID": "header",
      "slotName": "header",
      "sizes": [[728, 90], [970, 250]]
    }]
  }' | jq .
```

### Multiple Slots

```bash
curl -X POST http://127.0.0.1:8787/e/dtb/bid \
  -H 'Content-Type: application/json' \
  -d '{
    "pubId": "5555",
    "slots": [
      {
        "slotID": "header-banner",
        "slotName": "header-banner",
        "sizes": [[728, 90], [970, 250]]
      },
      {
        "slotID": "sidebar",
        "slotName": "sidebar",
        "sizes": [[300, 250], [300, 600]]
      },
      {
        "slotID": "mobile-banner",
        "slotName": "mobile-banner",
        "sizes": [[320, 50], [320, 100]]
      }
    ],
    "pageUrl": "https://example.com/article",
    "timeout": 800
  }' | jq .
```

### Using Example Script

```bash
./examples/aps_request.sh
```

## Error Responses

### Empty Slots (422)

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "slots: must have at least 1 item"
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

## Differences from Real APS

| Feature            | Real APS                   | Mocktioneer                |
| ------------------ | -------------------------- | -------------------------- |
| Price encoding     | Proprietary                | Base64                     |
| Creative rendering | Client-side with targeting | Client-side with targeting |
| Fill rate          | Variable                   | 100% for standard sizes    |
| Response time      | Network latency            | Instant                    |
| `adm` field        | Not provided               | Not provided               |

## Creative Rendering

Unlike OpenRTB responses, APS does not return an `adm` field with creative markup. Instead, creatives are rendered client-side using the targeting keys. For testing with Mocktioneer, you can render creatives directly using the static asset endpoints:

```javascript
// After receiving APS bid response
const slot = response.contextual.slots[0]
const size = slot.amznsz // e.g., "300x250"
const [width, height] = size.split('x')

// Render Mocktioneer creative directly (for testing)
const iframe = document.createElement('iframe')
iframe.src = `${response.contextual.host}/static/creatives/${size}.html`
iframe.width = width
iframe.height = height
iframe.frameBorder = '0'
document.getElementById('ad-container').appendChild(iframe)
```

## Integration with GAM

APS targeting keys can be passed to Google Ad Manager:

```javascript
googletag.pubads().setTargeting('amzniid', slot.amzniid)
googletag.pubads().setTargeting('amznbid', slot.amznbid)
googletag.pubads().setTargeting('amznsz', slot.amznsz)
```

In production, GAM line items configured with APS targeting will serve the Amazon creative. For Mocktioneer testing, configure GAM line items to redirect to Mocktioneer's creative endpoints based on the `amznsz` targeting key.

See the [APS Win Notification](./aps-win) endpoint for reporting wins.
