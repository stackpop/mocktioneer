# Pull Sync (Resolve)

The `/resolve` endpoint provides server-to-server identity resolution for the [Edge Cookie (EC)](../integrations/trusted-server) protocol. Trusted-server calls this endpoint to look up a buyer UID for a given EC identifier and client IP address.

## Endpoint

```
GET /resolve?ec_id={64-hex}.{6-alnum}&ip={ip}
```

Returns a deterministic buyer UID derived from the EC identifier and IP combination.

## Parameters

| Parameter | Location | Type   | Required | Description                                       |
| --------- | -------- | ------ | -------- | ------------------------------------------------- |
| `ec_id`   | Query    | string | Yes      | Full EC identifier in `{64-hex}.{6-alnum}` format |
| `ip`      | Query    | string | Yes      | Client IP address (1-45 characters)               |

## Authentication

When `MOCKTIONEER_PULL_TOKEN` is set, the endpoint requires a Bearer token in the `Authorization` header. The token is compared using constant-time comparison (SHA-256 digest) to prevent timing attacks.

```bash
curl "http://127.0.0.1:8787/resolve?ec_id=a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2.AbC123&ip=1.2.3.4" \
  -H "Authorization: Bearer mtk-pull-token-change-me"
```

When `MOCKTIONEER_PULL_TOKEN` is not set, authentication is disabled and any request is accepted.

::: warning WASM Note
On Cloudflare Workers, `std::env::var` returns `Err`, so authentication is effectively disabled in that environment. Use Cloudflare's built-in access controls (e.g., Service Auth tokens) instead.
:::

## Response Format

```json
{
  "uid": "mtk-a1b2c3d4e5f6"
}
```

| Field | Type   | Description                                     |
| ----- | ------ | ----------------------------------------------- |
| `uid` | string | Deterministic UID: `mtk-` prefix + 12 hex chars |

::: tip Deterministic UIDs
The 64-hex hash prefix is extracted from the `ec_id`, then hashed with the IP: `SHA-256(ec_hash || ip)` truncated to 12 hex characters, prefixed with `mtk-`. The same `(ec_id, ip)` pair always produces the same UID.
:::

## Examples

```bash
# Basic resolve request
curl "http://127.0.0.1:8787/resolve?ec_id=a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2.AbC123&ip=192.168.1.1" | jq .

# With authentication
curl "http://127.0.0.1:8787/resolve?ec_id=a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2.AbC123&ip=192.168.1.1" \
  -H "Authorization: Bearer mtk-pull-token-change-me" | jq .
```

```json
{
  "uid": "mtk-a1b2c3d4e5f6"
}
```

### Different IPs produce different UIDs

```bash
# Same ec_id, different IPs
curl "http://127.0.0.1:8787/resolve?ec_id=a1b2c3d4...64hex...AbC123&ip=1.2.3.4" | jq .uid
# "mtk-abc123def456"

curl "http://127.0.0.1:8787/resolve?ec_id=a1b2c3d4...64hex...AbC123&ip=5.6.7.8" | jq .uid
# "mtk-789012345678"  (different)
```

## Error Responses

### Missing or invalid ec_id (400)

The `ec_id` must be in `{64-hex}.{6-alnum}` format:

```bash
curl "http://127.0.0.1:8787/resolve?ec_id=tooshort&ip=1.2.3.4"
# Returns 400 Bad Request
```

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "ec_id: must be in {64-hex}.{6-alnum} format"
  }
}
```

### Non-hex characters in ec_id (400)

```bash
curl "http://127.0.0.1:8787/resolve?ec_id=zzzz...64chars....AbC123&ip=1.2.3.4"
# Returns 400 Bad Request
```

### Missing ip (400)

```bash
curl "http://127.0.0.1:8787/resolve?ec_id=a1b2...64hex.AbC123"
# Returns 400 Bad Request
```

### Unauthorized (401)

When `MOCKTIONEER_PULL_TOKEN` is set and the token is missing or incorrect:

```bash
curl "http://127.0.0.1:8787/resolve?ec_id=a1b2...64hex.AbC123&ip=1.2.3.4" \
  -H "Authorization: Bearer wrong-token"
# Returns 401 Unauthorized
```

## Environment Variables

| Variable                 | Description                                                                            | Default |
| ------------------------ | -------------------------------------------------------------------------------------- | ------- |
| `MOCKTIONEER_PULL_TOKEN` | Bearer token required for `/resolve` requests. When unset, authentication is disabled. | Unset   |

## Next Steps

- [Pixel Sync](./sync) — browser-based redirect sync flow
- [Trusted Server Integration](../integrations/trusted-server) — full setup guide
