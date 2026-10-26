# What is Mocktioneer?

Mocktioneer is a deterministic OpenRTB banner bidder designed for edge platforms. It helps test client integrations (Prebid.js, Prebid Server, custom SDKs) without depending on third-party bidders or origin backends.

## The Problem

When developing and testing ad tech integrations, you face several challenges:

- **Third-party dependencies** - Real bidders may be slow, rate-limited, or unavailable
- **Non-deterministic responses** - Real auctions return different results each time, making tests flaky
- **Complex setup** - Running a real bidder requires infrastructure, credentials, and configuration
- **Cost** - Real bid requests may incur costs or use up testing quotas

## The Solution

Mocktioneer provides:

- **Deterministic bidding** - Fixed input and configuration produce the same prices and bid selection; operational deadlines, key-cache expiry and signature freshness use clocks
- **Embedded creatives** - Assets render locally or at the edge; bid routes use pushed config, and signed requests may fetch trusted-server keys
- **OpenRTB 2.x banner support** - Works with standard OpenRTB banner clients
- **APS TAM compatibility** - Test Amazon Publisher Services integrations
- **Predictable creatives** - SVG banners with size and bid information for visual verification

## Use Cases

### QA and Testing

- Verify Prebid.js adapter behavior
- Test Prebid Server bidder configurations
- Validate creative rendering pipelines
- Check win notification handling

### Development

- Build ad tech features without waiting for real bidders
- Debug integration issues with predictable responses
- Test edge cases with fixed, deterministic bid pricing

### CI/CD

- Run automated tests without external dependencies
- Validate deployments with consistent mock data
- Performance testing with predictable latency

## Key Features

| Feature               | Description                                                                                    |
| --------------------- | ---------------------------------------------------------------------------------------------- |
| Multi-platform        | Runs on Fastly, Cloudflare, Spin 4.1+, and native Axum                                         |
| Manifest-driven       | Single `edgezero.toml` configures everything                                                   |
| Deterministic pricing | Returns a configurable CPM for generated bids (`bid_cpm` in mocktioneer.toml; default `$0.20`) |
| Standard sizes        | Supports common IAB ad sizes                                                                   |
| Cookie tracking       | Optional pixel tracking with `mtkid` cookie                                                    |
| CORS enabled          | Works with browser-based clients                                                               |

## How It Works

1. **Receive request** - Accept OpenRTB or APS bid requests
2. **Parse impressions** - Extract size and pricing information
3. **Generate bids** - Create deterministic bids based on input
4. **Return response** - Send OpenRTB-compliant response with creative URLs

The creative URLs point back to Mocktioneer's static asset endpoints, which render SVG images showing the ad size and bid price.

Auction and APS bodies are capped at 1 MiB; mediation keeps the 8 MiB default.
All three POST routes have a 10-second body-read budget. Config reads and some
platform deadlines are BestEffort; see the [API limits](/api/#request-limits-and-deadlines)
and [adapter capabilities](./adapters/#capability-support) before relying on them.

## Next Steps

- [Get started](./getting-started) with installation and local development
- Learn about [configuration](./configuration) options
- Explore the [API reference](/api/) for endpoint details
