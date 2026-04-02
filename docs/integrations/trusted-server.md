# Trusted Server (Edge Cookie)

Mocktioneer integrates with [trusted-server](https://github.com/ABTechLab/trusted-server) to support the Edge Cookie (EC) identity protocol. This enables testing of the full EC pipeline — pixel sync, pull sync, and bidstream identity decoration — without a production DSP.

## What is Edge Cookie?

Edge Cookie is a publisher-side identity mechanism managed by trusted-server. It works by:

1. Setting a first-party cookie (the "Edge Cookie") on the publisher's domain
2. Syncing partner IDs (like Mocktioneer's `mtkid`) with the EC via pixel sync or pull sync
3. Decorating OpenRTB bid requests with the synced identity data (`user.id`, `user.eids`, `user.buyeruid`)

Mocktioneer acts as a mock DSP partner, implementing all three integration points so you can test the EC pipeline end-to-end.

## Architecture

```
Publisher Page            Trusted Server              Mocktioneer
      │                        │                          │
      │   1. Page load sets    │                          │
      │      Edge Cookie       │                          │
      │<──────────────────────>│                          │
      │                        │                          │
      │   2. Pixel sync        │                          │
      │      redirect chain    │   /sync/start            │
      │───────────────────────────────────────────────────>│
      │                        │   302 → TS /sync         │
      │<───────────────────────────────────────────────────│
      │                        │                          │
      │   3. TS stores mapping │                          │
      │──────────────────────>│                           │
      │                        │   4. Pull sync           │
      │                        │      /resolve            │
      │                        │─────────────────────────>│
      │                        │   { "uid": "mtk-..." }   │
      │                        │<─────────────────────────│
      │                        │                          │
      │   5. Bid request with  │                          │
      │      user.eids         │   OpenRTB auction        │
      │──────────────────────>│──────────────────────────>│
      │                        │   Bid response           │
      │                        │<─────────────────────────│
```

## Prerequisites

- A running trusted-server instance
- Mocktioneer deployed (local or edge)
- Admin credentials for the trusted-server `/_ts/admin/*` API

## Setup

### 1. Register Mocktioneer as a Partner {#partner-registration}

Before any sync or identity decoration works, Mocktioneer must be registered as an EC partner with your trusted-server instance. Use the included registration script:

```bash
export TS_BASE_URL="https://ts.publisher.com"
export TS_ADMIN_USER="admin"
export TS_ADMIN_PASS="your-password"
export MOCKTIONEER_BASE_URL="https://mocktioneer.example.com"

./examples/register_partner.sh
```

This registers Mocktioneer with the following capabilities:

| Capability           | Value                                     |
| -------------------- | ----------------------------------------- |
| Partner ID           | `mocktioneer`                             |
| Source domain        | `mocktioneer.dev`                         |
| OpenRTB atype        | `3` (partner-defined)                     |
| Pixel sync           | Enabled (via `/sync/start` redirect flow) |
| Pull sync            | Enabled (via `/resolve` endpoint)         |
| Bidstream decoration | Enabled (`user.eids`, `user.buyeruid`)    |

::: details Full registration payload

```json
{
  "id": "mocktioneer",
  "name": "Mocktioneer Mock DSP",
  "allowed_return_domains": ["mocktioneer.example.com"],
  "api_key": "mtk-demo-key-change-me",
  "bidstream_enabled": true,
  "source_domain": "mocktioneer.dev",
  "openrtb_atype": 3,
  "sync_rate_limit": 100,
  "batch_rate_limit": 60,
  "pull_sync_enabled": true,
  "pull_sync_url": "https://mocktioneer.example.com/resolve",
  "pull_sync_allowed_domains": ["mocktioneer.example.com"],
  "pull_sync_ttl_sec": 86400,
  "pull_sync_rate_limit": 10,
  "ts_pull_token": "mtk-pull-token-change-me"
}
```

:::

#### Registration Environment Variables

| Variable                 | Description                                    | Default                                  |
| ------------------------ | ---------------------------------------------- | ---------------------------------------- |
| `TS_BASE_URL`            | Trusted-server base URL                        | `https://cdintel.com`                    |
| `TS_ADMIN_USER`          | Basic Auth username for admin API              | **Required**                             |
| `TS_ADMIN_PASS`          | Basic Auth password for admin API              | **Required**                             |
| `MOCKTIONEER_BASE_URL`   | Mocktioneer's public base URL                  | `https://origin-mocktioneer.cdintel.com` |
| `MOCKTIONEER_API_KEY`    | API key for batch sync authentication          | `mtk-demo-key-change-me`                 |
| `MOCKTIONEER_PULL_TOKEN` | Bearer token trusted-server sends on pull sync | `mtk-pull-token-change-me`               |

::: warning Change Default Tokens
The default `MOCKTIONEER_API_KEY` and `MOCKTIONEER_PULL_TOKEN` values are placeholders. Set real values in production.
:::

### 2. Configure Mocktioneer Environment

Set these environment variables on your Mocktioneer deployment:

```bash
# Optional: restrict which trusted-server domains can initiate sync
export MOCKTIONEER_TS_DOMAINS="ts.publisher.com,ts.staging.publisher.com"

# Optional: require authentication on /resolve
export MOCKTIONEER_PULL_TOKEN="mtk-pull-token-change-me"
```

| Variable                 | Description                                                             | Default               |
| ------------------------ | ----------------------------------------------------------------------- | --------------------- |
| `MOCKTIONEER_TS_DOMAINS` | Comma-separated allowlist of trusted-server hostnames for `/sync/start` | Unset (all allowed)   |
| `MOCKTIONEER_PULL_TOKEN` | Bearer token for `/resolve` authentication                              | Unset (auth disabled) |

## Sync Methods

### Pixel Sync (Browser-Based)

Pixel sync uses a browser redirect chain to associate Mocktioneer's `mtkid` cookie with the publisher's Edge Cookie. This is the primary sync method for browser-based environments.

**Flow:**

1. Publisher page loads a sync pixel pointing to `/sync/start?ts_domain=ts.publisher.com`
2. Mocktioneer sets (or reads) the `mtkid` cookie and redirects to trusted-server
3. Trusted-server stores the `mtkid` → EC mapping and redirects back to `/sync/done`
4. Mocktioneer returns a 1x1 transparent GIF

```html
<!-- Add to publisher page to trigger sync -->
<img
  src="https://mocktioneer.example.com/sync/start?ts_domain=ts.publisher.com"
  width="1"
  height="1"
  style="display:none"
/>
```

See the [Sync API reference](/api/sync) for full endpoint details.

### Pull Sync (Server-to-Server)

Pull sync allows trusted-server to resolve a buyer UID on demand by calling Mocktioneer's `/resolve` endpoint directly. This is used when the browser-based sync hasn't happened yet or as a fallback.

**Flow:**

1. Trusted-server receives a bid request with an EC hash
2. Trusted-server calls `GET /resolve?ec_hash={hash}&ip={client_ip}` on Mocktioneer
3. Mocktioneer returns a deterministic UID (`mtk-{12 hex chars}`)
4. Trusted-server includes this UID in the OpenRTB bid request's `user.eids`

See the [Resolve API reference](/api/resolve) for full endpoint details.

## Bidstream Identity Decoration

After sync, trusted-server decorates OpenRTB bid requests with EC identity data. Mocktioneer parses these fields and reflects them in creative metadata for visual debugging.

### OpenRTB Fields Used

| Field           | Description                                        | Example                            |
| --------------- | -------------------------------------------------- | ---------------------------------- |
| `user.id`       | Full EC value (`{64-hex}.{6-alnum}`)               | `a1b2c3...d4e5.AbC123`             |
| `user.buyeruid` | Mocktioneer's synced UID                           | `mtk-a1b2c3d4e5f6`                 |
| `user.consent`  | TCF consent string                                 | `CPx...`                           |
| `user.eids`     | Extended Identifiers (OpenRTB 2.6)                 | See below                          |
| `user.ext.eids` | Extended Identifiers (Prebid Server / OpenRTB 2.5) | Fallback when `user.eids` is empty |

### EID Format

Trusted-server adds Mocktioneer's UID to the `user.eids` array:

```json
{
  "user": {
    "id": "a1b2c3d4e5f6...64hex...chars.AbC123",
    "buyeruid": "mtk-a1b2c3d4e5f6",
    "eids": [
      {
        "source": "mocktioneer.dev",
        "uids": [
          {
            "id": "mtk-a1b2c3d4e5f6",
            "atype": 3
          }
        ]
      }
    ]
  }
}
```

### EC Info in Creative Metadata

When EC data is present in a bid request, Mocktioneer includes it in the creative's HTML comment metadata alongside the existing signature and request/response data:

```json
{
  "edge_cookie": {
    "ec_value": "a1b2c3...64hex.AbC123",
    "ec_hash": "a1b2c3...64hex",
    "buyer_uid": "mtk-a1b2c3d4e5f6",
    "consent": "CPx...",
    "eids_count": 1,
    "mocktioneer_matched": true
  }
}
```

This allows you to inspect the rendered creative and verify which EC fields were received by the bidder.

## Testing the Full Flow

### 1. Register and start Mocktioneer

```bash
# Register with trusted-server (one-time)
export TS_BASE_URL="https://ts.publisher.com"
export TS_ADMIN_USER="admin"
export TS_ADMIN_PASS="password"
./examples/register_partner.sh

# Start Mocktioneer locally
cargo run -p mocktioneer-adapter-axum
```

### 2. Test pixel sync

```bash
# Initiate sync (follow redirects with -L)
curl -v -L "http://127.0.0.1:8787/sync/start?ts_domain=ts.publisher.com"
```

### 3. Test pull sync

```bash
# Resolve a UID from an EC hash
curl "http://127.0.0.1:8787/resolve?ec_hash=a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2&ip=192.168.1.1" | jq .
```

### 4. Test bidstream decoration

Send an OpenRTB request with EC identity fields and inspect the creative metadata:

```bash
curl -s -X POST http://127.0.0.1:8787/openrtb2/auction \
  -H 'Content-Type: application/json' \
  -d '{
    "id": "test-ec",
    "imp": [{"id": "1", "banner": {"w": 300, "h": 250}}],
    "user": {
      "id": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2.AbC123",
      "buyeruid": "mtk-a1b2c3d4e5f6",
      "eids": [{
        "source": "mocktioneer.dev",
        "uids": [{"id": "mtk-a1b2c3d4e5f6", "atype": 3}]
      }]
    }
  }' | jq .
```

## Security Considerations

- **Open-redirect protection**: `/sync/start` validates `ts_domain` as a clean hostname, rejecting paths, ports, auth strings, and query parameters
- **Domain allowlist**: Set `MOCKTIONEER_TS_DOMAINS` to restrict which trusted-server instances can initiate sync
- **Constant-time auth**: `/resolve` uses SHA-256 digest comparison to prevent timing attacks on the Bearer token
- **Input sanitization**: User-supplied values are sanitized before logging (control characters stripped, length truncated)
- **Deterministic IDs**: No randomness — all generated IDs use SHA-256 hashing for reproducibility
