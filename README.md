# Mocktioneer

Deterministic OpenRTB banner bidder for edge platforms. Test client integrations (Prebid.js, Prebid Server, custom SDKs) without depending on third-party bidders or origin backends.

## Features

- **Multi-platform** - Runs on Fastly Compute, Cloudflare Workers, and native Axum from a single codebase
- **Deterministic** - Same input always produces the same output
- **OpenRTB 2.x & APS TAM** - Full banner support with predictable pricing
- **Zero dependencies** - All routes render from embedded assets

## Quick Start

```bash
# Clone and run locally
git clone https://github.com/stackpop/mocktioneer.git
cd mocktioneer
cargo run -p mocktioneer-adapter-axum

# Test the auction endpoint
curl -X POST http://127.0.0.1:8787/openrtb2/auction \
  -H 'Content-Type: application/json' \
  -d '{"id":"test","imp":[{"id":"1","banner":{"w":300,"h":250}}]}'
```

## Documentation

Full documentation is available at **[stackpop.github.io/mocktioneer](https://stackpop.github.io/mocktioneer/)**

- [Getting Started](https://stackpop.github.io/mocktioneer/guide/getting-started) - Installation and setup
- [API Reference](https://stackpop.github.io/mocktioneer/api/) - Endpoint documentation
- [Configuration](https://stackpop.github.io/mocktioneer/guide/configuration) - `edgezero.toml` options
- [Integrations](https://stackpop.github.io/mocktioneer/integrations/) - Prebid.js and Prebid Server

## Endpoints

| Path | Description |
|------|-------------|
| `POST /openrtb2/auction` | OpenRTB 2.x bid request |
| `POST /e/dtb/bid` | APS TAM bid request |
| `GET /static/creatives/{size}.html` | Creative wrapper |
| `GET /_/sizes` | Supported sizes with pricing |

See the [full API reference](https://stackpop.github.io/mocktioneer/api/) for all endpoints.

## Development

```bash
cargo test                              # Run tests
cargo run -p mocktioneer-adapter-axum   # Local server on :8787
```

## License

Apache License 2.0. See [LICENSE](LICENSE) for details.
