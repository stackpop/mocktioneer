# Integrations

Mocktioneer integrates with popular ad tech platforms for testing header bidding and programmatic advertising flows.

## Supported Integrations

| Platform                         | Type        | Description                  |
| -------------------------------- | ----------- | ---------------------------- |
| [Prebid.js](./prebidjs)          | Client-side | Browser-based header bidding |
| [Prebid Server](./prebid-server) | Server-side | Server-to-server bidding     |

## How Integration Works

Mocktioneer acts as a drop-in replacement for real bidders during development and testing:

```
┌─────────────────┐     ┌─────────────────┐
│   Prebid.js     │     │  Prebid Server  │
│   (Browser)     │     │    (Server)     │
└────────┬────────┘     └────────┬────────┘
         │                       │
         │    OpenRTB 2.x        │
         │    Bid Request        │
         ▼                       ▼
┌─────────────────────────────────────────┐
│              Mocktioneer                 │
│         (Edge or Local)                  │
└─────────────────────────────────────────┘
         │
         │    OpenRTB 2.x
         │    Bid Response
         ▼
┌─────────────────────────────────────────┐
│            Ad Server (GAM)               │
└─────────────────────────────────────────┘
```

## Benefits

### Deterministic Testing

- Same request always produces same response
- No flaky tests due to bidder variability
- Controlled bid prices for testing scenarios

### No External Dependencies

- No API keys or credentials needed
- Works offline
- Fast response times

### OpenRTB Banner Support

- OpenRTB 2.x banner requests and responses
- Compatible with standard OpenRTB banner clients
- Valid creative URLs

## Quick Start

### Prebid.js

```javascript
pbjs.setBidderConfig({
  bidders: ['mocktioneer'],
  config: {
    endpoint: 'http://localhost:8787/openrtb2/auction',
  },
})
```

### Prebid Server

```yaml
adapters:
  mocktioneer:
    enabled: true
    endpoint: http://localhost:8787/openrtb2/auction
```

## Deployment Options

### Local Development

Run Mocktioneer locally for fastest iteration:

```bash
cargo run -p mocktioneer-adapter-axum
```

### Shared Test Environment

Deploy to Fastly or Cloudflare for team-wide access:

- `https://mocktioneer.edgecompute.app` (Fastly)
- `https://mocktioneer.your-domain.workers.dev` (Cloudflare)

### CI/CD

Include Mocktioneer in your test pipeline (build and publish your own image first):

```yaml
# GitHub Actions example
services:
  mocktioneer:
    image: mocktioneer:latest # replace with your published image
    ports:
      - 8787:8787
```

## Common Use Cases

### Testing Bid Adapters

Verify your Prebid adapter handles responses correctly:

1. Configure adapter to use Mocktioneer
2. Run test page
3. Inspect bid responses in Prebid debug

### Validating Creative Rendering

Test creative rendering pipeline:

1. Get bid response with creative URL
2. Load creative in iframe
3. Verify SVG displays correctly

### Price Testing

Test price handling and floor logic:

```javascript
// Override bid price
params: {
  bid: 5.0 // Force $5 CPM
}
```

### Error Handling

Test error scenarios:

- Empty APS responses when all sizes are non-standard (OpenRTB coerces to 300x250)
- Malformed requests
- Timeout simulation (not built-in, use network tools)
