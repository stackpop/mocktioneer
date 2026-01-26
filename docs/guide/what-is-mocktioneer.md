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

- **Deterministic responses** - Same input always produces the same output
- **Zero external dependencies** - Everything runs locally or at the edge
- **Full OpenRTB 2.x compliance** - Works with any standard OpenRTB client
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
- Test edge cases with controlled bid prices

### CI/CD

- Run automated tests without external dependencies
- Validate deployments with consistent mock data
- Performance testing with predictable latency

## Key Features

| Feature | Description |
|---------|-------------|
| Multi-platform | Runs on Fastly, Cloudflare, and native Axum |
| Manifest-driven | Single `edgezero.toml` configures everything |
| Price control | Override bid prices via request extensions |
| Standard sizes | Supports common IAB ad sizes |
| Cookie tracking | Optional pixel tracking with `mtkid` cookie |
| CORS enabled | Works with browser-based clients |

## How It Works

1. **Receive request** - Accept OpenRTB or APS bid requests
2. **Parse impressions** - Extract size and pricing information
3. **Generate bids** - Create deterministic bids based on input
4. **Return response** - Send OpenRTB-compliant response with creative URLs

The creative URLs point back to Mocktioneer's static asset endpoints, which render SVG images showing the ad size and bid price.

## Next Steps

- [Get started](./getting-started) with installation and local development
- Learn about [configuration](./configuration) options
- Explore the [API reference](/api/) for endpoint details
