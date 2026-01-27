# Prebid.js Integration

Mocktioneer works with Prebid.js for client-side header bidding integration.

## Adapter Options

### Option 1: Use Generic OpenRTB Adapter (Recommended)

Prebid.js includes a generic OpenRTB adapter that works with any OpenRTB-compliant endpoint. No custom adapter required:

```javascript
var adUnits = [
  {
    code: 'div-banner-1',
    mediaTypes: {
      banner: { sizes: [[300, 250]] },
    },
    bids: [
      {
        bidder: 'genericOrtb',
        params: {
          endpoint: 'http://localhost:8787/openrtb2/auction',
        },
      },
    ],
  },
]
```

### Option 2: Custom Mocktioneer Adapter

For a dedicated adapter with Mocktioneer-specific features (like price override), create a custom adapter:

```
your-prebid-fork/modules/mocktioneerBidAdapter.js
```

Build Prebid with your custom adapter:

```bash
gulp build --modules=mocktioneerBidAdapter
```

## Configuration

### Basic Setup

```javascript
var adUnits = [
  {
    code: 'div-banner-1',
    mediaTypes: {
      banner: {
        sizes: [
          [300, 250],
          [320, 50],
        ],
      },
    },
    bids: [
      {
        bidder: 'mocktioneer',
        params: {
          // Optional: custom endpoint
          endpoint: 'https://mocktioneer.edgecompute.app/openrtb2/auction',
        },
      },
    ],
  },
]
```

### Default Endpoint

If no endpoint is specified, the adapter uses:

```
https://mocktioneer.edgecompute.app/openrtb2/auction
```

### Local Development

Point to your local Mocktioneer instance:

```javascript
params: {
  endpoint: 'http://localhost:8787/openrtb2/auction'
}
```

## Parameters

| Parameter  | Type   | Required | Description                 |
| ---------- | ------ | -------- | --------------------------- |
| `endpoint` | string | No       | Custom auction endpoint URL |
| `bid`      | float  | No       | Override bid price (CPM)    |

### Price Override

Force a specific bid price for testing:

```javascript
bids: [
  {
    bidder: 'mocktioneer',
    params: {
      bid: 5.0, // Force $5.00 CPM
    },
  },
]
```

The price override is passed via `imp[].ext.mocktioneer.bid` in the OpenRTB request.

## Example Page

::: details Full HTML Example (click to expand)

```html
<!DOCTYPE html>
<html>
  <head>
    <script src="prebid.js"></script>
    <script>
      var adUnits = [
        {
          code: 'div-banner',
          mediaTypes: { banner: { sizes: [[300, 250]] } },
          bids: [
            {
              bidder: 'mocktioneer',
              params: { endpoint: 'http://localhost:8787/openrtb2/auction' },
            },
          ],
        },
      ]

      var pbjs = pbjs || {}
      pbjs.que = pbjs.que || []
      pbjs.que.push(function () {
        pbjs.addAdUnits(adUnits)
        pbjs.requestBids({
          bidsBackHandler: function () {
            var winner = pbjs.getHighestCpmBids('div-banner')[0]
            if (winner)
              pbjs.renderAd(document.getElementById('div-banner'), winner.adId)
          },
          timeout: 1000,
        })
      })
    </script>
  </head>
  <body>
    <div id="div-banner" style="width:300px;height:250px;"></div>
  </body>
</html>
```

:::

## Debugging

### Enable Prebid Debug

```javascript
pbjs.setConfig({
  debug: true,
})
```

### Check Bid Responses

Open browser console and look for:

```
Prebid Debug: Bids Received for mocktioneer
```

### Inspect Network Requests

In DevTools Network tab, filter for `openrtb2/auction`:

- Request payload shows OpenRTB bid request
- Response shows bid with creative URL

## Multiple Ad Units

```javascript
var adUnits = [
  {
    code: 'header-banner',
    mediaTypes: {
      banner: {
        sizes: [
          [728, 90],
          [970, 250],
        ],
      },
    },
    bids: [
      {
        bidder: 'mocktioneer',
        params: { bid: 3.0 },
      },
    ],
  },
  {
    code: 'sidebar',
    mediaTypes: {
      banner: {
        sizes: [
          [300, 250],
          [300, 600],
        ],
      },
    },
    bids: [
      {
        bidder: 'mocktioneer',
        params: { bid: 2.0 },
      },
    ],
  },
  {
    code: 'mobile-banner',
    mediaTypes: {
      banner: {
        sizes: [
          [320, 50],
          [320, 100],
        ],
      },
    },
    bids: [
      {
        bidder: 'mocktioneer',
        params: { bid: 1.5 },
      },
    ],
  },
]
```

## Testing Scenarios

### No Bid Response

Mocktioneer returns a bid for valid banner impressions (non-standard sizes are coerced to 300x250). If you need a no-bid path, filter it on the client side or use the mediation endpoint with a `price_floor` above the bids you send.

### High CPM Testing

Test price floor logic:

```javascript
params: {
  bid: 100.0 // $100 CPM
}
```

### Multiple Bidders

Compare Mocktioneer with other bidders:

```javascript
bids: [
  {
    bidder: 'mocktioneer',
    params: { bid: 2.0 },
  },
  {
    bidder: 'appnexus',
    params: { placementId: '12345' },
  },
]
```

## GAM Integration

Send Mocktioneer bids to Google Ad Manager:

```javascript
pbjs.que.push(function () {
  pbjs.setConfig({
    priceGranularity: 'dense',
  })

  pbjs.addAdUnits(adUnits)

  pbjs.requestBids({
    bidsBackHandler: function () {
      pbjs.setTargetingForGPTAsync()
      googletag.pubads().refresh()
    },
  })
})
```

## Troubleshooting

### No Bids Returned

1. Check endpoint is accessible
2. Verify ad unit sizes are standard
3. Enable Prebid debug mode
4. Check browser console for errors

### Creative Not Rendering

1. Verify iframe src URL is correct
2. Check for CORS issues (shouldn't occur with Mocktioneer)
3. Ensure creative size matches ad unit

### Timeout Issues

```javascript
pbjs.setConfig({
  bidderTimeout: 3000, // Increase timeout
})
```

For local development, ensure Mocktioneer is running before requesting bids.
