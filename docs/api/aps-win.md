# APS Win Notification

The `/aps/win` endpoint receives win notifications for APS TAM bids. This is used to report when an APS bid wins the auction.

## Endpoint

```
GET /aps/win?slot={slotID}&price={price}
```

## Parameters

| Parameter | Location | Type | Required | Description |
|-----------|----------|------|----------|-------------|
| `slot` | Query | string | Yes | Slot ID that won |
| `price` | Query | float | Yes | Winning price (min 0) |

## Response

Returns `204 No Content` on success.

```
HTTP/1.1 204 No Content
Access-Control-Allow-Origin: *
```

## Logging

Win notifications are logged at the `info` level:

```
APS win notification slot=header-banner, price=2.50
```

## Examples

### cURL

```bash
# Basic win notification
curl -v "http://127.0.0.1:8787/aps/win?slot=header-banner&price=2.50"

# Response: 204 No Content
```

### From APS Integration

In a real APS integration, you would fire this endpoint when your ad server reports a win:

```javascript
// After GAM reports APS line item win
function reportApsWin(slotId, price) {
  const url = new URL('https://mocktioneer.edgecompute.app/aps/win');
  url.searchParams.set('slot', slotId);
  url.searchParams.set('price', price);
  
  navigator.sendBeacon(url.toString());
}

// Example usage
reportApsWin('header-banner', 2.50);
```

### Using Fetch

```javascript
async function reportWin(slot, price) {
  const response = await fetch(
    `http://127.0.0.1:8787/aps/win?slot=${slot}&price=${price}`
  );
  
  if (response.status === 204) {
    console.log('Win reported successfully');
  }
}
```

## Error Responses

### Missing slot (400)

```bash
curl "http://127.0.0.1:8787/aps/win?price=2.50"
```

```json
{
  "error": {
    "code": "VALIDATION_ERROR", 
    "message": "slot: missing required field"
  }
}
```

### Missing price (400)

```bash
curl "http://127.0.0.1:8787/aps/win?slot=header-banner"
```

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "price: missing required field"
  }
}
```

### Negative price (422)

```bash
curl "http://127.0.0.1:8787/aps/win?slot=header-banner&price=-1.0"
```

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "price: must be >= 0"
  }
}
```

### Empty slot (400)

```bash
curl "http://127.0.0.1:8787/aps/win?slot=&price=2.50"
```

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "slot: must have at least 1 character"
  }
}
```

## Integration Flow

The typical APS win notification flow:

```
1. APS bid request (/e/dtb/bid)
   ↓
2. APS response with targeting keys
   ↓
3. GAM request with APS targeting
   ↓
4. GAM selects APS line item (win)
   ↓
5. Win notification (/aps/win)
```

### Full Example

```javascript
// 1. Get APS bids
const apsResponse = await fetch('/e/dtb/bid', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    pubId: '1234',
    slots: [{ slotID: 'header', slotName: 'header', sizes: [[728, 90]] }]
  })
});
const apsData = await apsResponse.json();

// 2. Pass targeting to GAM
const slot = apsData.contextual.slots[0];
googletag.pubads().setTargeting('amzniid', slot.amzniid);
googletag.pubads().setTargeting('amznbid', slot.amznbid);

// 3. When APS wins, report it
googletag.pubads().addEventListener('slotRenderEnded', (event) => {
  if (event.advertiserId === APS_ADVERTISER_ID) {
    // Decode price from amznbid (in mock, it's base64)
    const price = atob(slot.amznbid);
    fetch(`/aps/win?slot=${slot.slotID}&price=${price}`);
  }
});
```

## Use Cases

### Testing Win Reporting

Verify your win reporting logic fires correctly:

1. Make APS bid request
2. Simulate ad server win selection
3. Fire win notification
4. Check logs for confirmation

### Analytics Integration

Use win notifications to track:
- Win rates by slot
- Average winning prices
- Fill rate analysis

### Debugging

Compare win notifications against bid requests to identify:
- Missing win reports
- Price discrepancies
- Slot ID mismatches
