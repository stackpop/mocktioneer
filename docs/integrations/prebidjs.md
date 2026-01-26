# Prebid.js Integration

Mocktioneer provides a Prebid.js bid adapter for client-side header bidding integration.

## Adapter Installation

The Mocktioneer adapter is available in the Prebid.js modules:

```
Prebid.js/modules/mocktioneerBidAdapter.js
```

### Building Prebid with Mocktioneer

```bash
gulp build --modules=mocktioneerBidAdapter
```

Or include in your modules list:

```javascript
// modules.json
[
  "mocktioneerBidAdapter",
  // ... other adapters
]
```

## Configuration

### Basic Setup

```javascript
var adUnits = [
  {
    code: 'div-banner-1',
    mediaTypes: {
      banner: {
        sizes: [[300, 250], [320, 50]]
      }
    },
    bids: [
      {
        bidder: 'mocktioneer',
        params: {
          // Optional: custom endpoint
          endpoint: 'https://mocktioneer.edgecompute.app/openrtb2/auction'
        }
      }
    ]
  }
];
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

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `endpoint` | string | No | Custom auction endpoint URL |
| `bid` | float | No | Override bid price (CPM) |

### Price Override

Force a specific bid price for testing:

```javascript
bids: [
  {
    bidder: 'mocktioneer',
    params: {
      bid: 5.00  // Force $5.00 CPM
    }
  }
]
```

The price override is passed via `imp[].ext.mocktioneer.bid` in the OpenRTB request.

## Example Page

```html
<!DOCTYPE html>
<html>
<head>
  <script src="prebid.js"></script>
  <script>
    var PREBID_TIMEOUT = 1000;

    var adUnits = [{
      code: 'div-banner',
      mediaTypes: {
        banner: { sizes: [[300, 250]] }
      },
      bids: [{
        bidder: 'mocktioneer',
        params: {
          endpoint: 'http://localhost:8787/openrtb2/auction',
          bid: 2.50
        }
      }]
    }];

    var pbjs = pbjs || {};
    pbjs.que = pbjs.que || [];

    pbjs.que.push(function() {
      pbjs.addAdUnits(adUnits);
      pbjs.requestBids({
        bidsBackHandler: function(bids) {
          console.log('Bids received:', bids);
          // Render winning bid
          var winner = pbjs.getHighestCpmBids('div-banner')[0];
          if (winner) {
            pbjs.renderAd(document.getElementById('div-banner'), winner.adId);
          }
        },
        timeout: PREBID_TIMEOUT
      });
    });
  </script>
</head>
<body>
  <div id="div-banner" style="width:300px;height:250px;"></div>
</body>
</html>
```

## Debugging

### Enable Prebid Debug

```javascript
pbjs.setConfig({
  debug: true
});
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
      banner: { sizes: [[728, 90], [970, 250]] }
    },
    bids: [{
      bidder: 'mocktioneer',
      params: { bid: 3.00 }
    }]
  },
  {
    code: 'sidebar',
    mediaTypes: {
      banner: { sizes: [[300, 250], [300, 600]] }
    },
    bids: [{
      bidder: 'mocktioneer',
      params: { bid: 2.00 }
    }]
  },
  {
    code: 'mobile-banner',
    mediaTypes: {
      banner: { sizes: [[320, 50], [320, 100]] }
    },
    bids: [{
      bidder: 'mocktioneer',
      params: { bid: 1.50 }
    }]
  }
];
```

## Testing Scenarios

### No Bid Response

Request a non-standard size to get no bid:

```javascript
mediaTypes: {
  banner: { sizes: [[999, 999]] }  // Non-standard, will be coerced to 300x250
}
```

### High CPM Testing

Test price floor logic:

```javascript
params: {
  bid: 100.00  // $100 CPM
}
```

### Multiple Bidders

Compare Mocktioneer with other bidders:

```javascript
bids: [
  {
    bidder: 'mocktioneer',
    params: { bid: 2.00 }
  },
  {
    bidder: 'appnexus',
    params: { placementId: '12345' }
  }
]
```

## GAM Integration

Send Mocktioneer bids to Google Ad Manager:

```javascript
pbjs.que.push(function() {
  pbjs.setConfig({
    priceGranularity: 'dense'
  });
  
  pbjs.addAdUnits(adUnits);
  
  pbjs.requestBids({
    bidsBackHandler: function() {
      pbjs.setTargetingForGPTAsync();
      googletag.pubads().refresh();
    }
  });
});
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
  bidderTimeout: 3000  // Increase timeout
});
```

For local development, ensure Mocktioneer is running before requesting bids.
