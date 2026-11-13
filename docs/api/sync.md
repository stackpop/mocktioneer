# Edge Cookie Sync

Mocktioneer supports [Edge Cookie (EC)](../integrations/trusted-server) pixel sync — a browser-based redirect flow that associates Mocktioneer's `mtkid` cookie with the publisher's Edge Cookie via a trusted-server instance.

Two endpoints make up the pixel sync flow: `/sync/start` initiates the redirect chain and `/sync/done` receives the callback.

## Sync Flow

```
Browser                  Mocktioneer               Trusted Server
   │                         │                           │
   │  GET /sync/start        │                           │
   │  ?ts_domain=ts.pub.com  │                           │
   │────────────────────────>│                           │
   │                         │                           │
   │  302 → ts.pub.com/sync  │                           │
   │  Set-Cookie: mtkid=...  │                           │
   │<────────────────────────│                           │
   │                         │                           │
   │  GET /sync?partner=mocktioneer&uid=...&return=...   │
   │────────────────────────────────────────────────────>│
   │                         │                           │
   │  302 → mocktioneer/sync/done?ts_synced=1            │
   │<────────────────────────────────────────────────────│
   │                         │                           │
   │  GET /sync/done         │                           │
   │  ?ts_synced=1           │                           │
   │────────────────────────>│                           │
   │                         │                           │
   │  200 (1x1 GIF)          │                           │
   │<────────────────────────│                           │
```

## Start Sync {#sync-start}

### Endpoint

```
GET /sync/start?ts_domain={hostname}
```

Initiates the pixel sync by redirecting the browser to the trusted-server's `/sync` endpoint, passing Mocktioneer's `mtkid` cookie value.

### Parameters

| Parameter   | Location | Type   | Required | Description                                        |
| ----------- | -------- | ------ | -------- | -------------------------------------------------- |
| `ts_domain` | Query    | string | Yes      | Trusted-server hostname (e.g., `ts.publisher.com`) |

### Behavior

1. Validates `ts_domain` as a clean hostname (no paths, ports, auth, or query strings)
2. If `MOCKTIONEER_TS_DOMAINS` is set, checks `ts_domain` against the allowlist
3. Reads existing `mtkid` cookie or creates a new deterministic, host-scoped mock/test one
4. Redirects to `https://{ts_domain}/sync?partner=mocktioneer&uid={mtkid}&return={self}/sync/done`

### Response

Returns `302 Found` with a `Location` header pointing to the trusted-server.

```
HTTP/1.1 302 Found
Location: https://ts.publisher.com/sync?partner=mocktioneer&uid=abc123...&return=https%3A%2F%2Fmocktioneer.example.com%2Fsync%2Fdone
Cache-Control: no-store, no-cache, must-revalidate, max-age=0
Set-Cookie: mtkid=abc123...; Path=/; Max-Age=31536000; SameSite=None; Secure; HttpOnly
```

The `Set-Cookie` header is only present when creating a new cookie.

### Cookie Details

| Property | Value                                                 |
| -------- | ----------------------------------------------------- |
| Name     | `mtkid`                                               |
| Value    | Host-scoped deterministic SHA-256 hash (32 hex chars) |
| Path     | `/`                                                   |
| Max-Age  | 31536000 (1 year)                                     |
| SameSite | None                                                  |
| Secure   | Yes                                                   |
| HttpOnly | Yes                                                   |

::: tip Deterministic Host-Scoped IDs
When no existing cookie is present, the generated `mtkid` value is derived from `SHA-256("mtkid:" || host)` and truncated to 32 hex characters. The same Mocktioneer host always produces the same generated `mtkid`, so this is intentionally mock/test-oriented and not a per-visitor production identifier.
:::

::: warning Configure an Allowlist in Production
When `MOCKTIONEER_TS_DOMAINS` is unset, `/sync/start` accepts any syntactically valid hostname. This is convenient for local demos, but production deployments should set an explicit allowlist.
:::

### Examples

```bash
# Initiate sync with trusted-server
curl -v "http://127.0.0.1:8787/sync/start?ts_domain=ts.publisher.com"
# Returns 302 redirect to ts.publisher.com/sync?partner=mocktioneer&uid=...

# With existing mtkid cookie
curl -v "http://127.0.0.1:8787/sync/start?ts_domain=ts.publisher.com" \
  -H "Cookie: mtkid=existing-value"
```

### Error Responses

| Failure                                         | Status | Body                                 |
| ----------------------------------------------- | ------ | ------------------------------------ |
| Missing `ts_domain` or malformed query          | 400    | Framework JSON error (`bad_request`) |
| Empty `ts_domain` or more than 253 characters   | 422    | Framework JSON error (`validation`)  |
| Invalid hostname syntax after length validation | 400    | Empty (`Content-Length: 0`)          |
| Domain not in the configured allowlist          | 403    | Empty (`Content-Length: 0`)          |

Hostname checks reject `/`, `@`, `:`, `?`, `#`, whitespace, empty labels,
labels over 63 characters, underscores, and labels starting or ending with `-`.
The redirect has an empty body with `Content-Length: 0`; it does not perform a
server-side fetch of the trusted-server URL.

```bash
curl "http://127.0.0.1:8787/sync/start?ts_domain=evil.com/redirect"
# Returns an empty 400 Bad Request

curl "http://127.0.0.1:8787/sync/start?ts_domain="
# Returns 422 Unprocessable Entity
```

---

## Sync Done {#sync-done}

### Endpoint

```
GET /sync/done?ts_synced={0|1}
```

Receives the callback from trusted-server after the sync completes. Returns a 1x1 transparent GIF for a valid success or failure callback.

### Parameters

| Parameter   | Location | Type   | Required | Description                                          |
| ----------- | -------- | ------ | -------- | ---------------------------------------------------- |
| `ts_synced` | Query    | string | Yes      | `"1"` for success, `"0"` for failure                 |
| `ts_reason` | Query    | string | No       | Failure reason, max 256 chars (e.g., `"no_consent"`) |

### Response

Valid callbacks return a 1x1 transparent GIF:

```
HTTP/1.1 200 OK
Content-Type: image/gif
Content-Length: 43
Cache-Control: no-store, no-cache, must-revalidate, max-age=0
```

Missing or malformed query fields return 400 (`bad_request`). A `ts_synced`
value other than `0` or `1`, or a `ts_reason` longer than 256 characters, returns
422 (`validation`).

### Examples

```bash
# Successful sync callback
curl -v "http://127.0.0.1:8787/sync/done?ts_synced=1"

# Failed sync callback (e.g., no consent)
curl -v "http://127.0.0.1:8787/sync/done?ts_synced=0&ts_reason=no_consent"
```

---

## Environment Variables

| Variable                 | Description                                                                                                             | Default                                                        |
| ------------------------ | ----------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| `MOCKTIONEER_TS_DOMAINS` | Comma-separated allowlist of trusted-server hostnames. When set, `/sync/start` rejects any `ts_domain` not in the list. | Unset (all syntactically valid domains allowed; demo/dev mode) |

```bash
# Allow only specific trusted-server instances
export MOCKTIONEER_TS_DOMAINS="ts.publisher.com,ts.staging.publisher.com"
```

::: warning WASM Note
On Cloudflare Workers, this endpoint currently reads `MOCKTIONEER_TS_DOMAINS` with `std::env::var`, which does not see `wrangler.toml` bindings. Use platform controls or adapter-level configuration for production Cloudflare deployments.
:::

## Next Steps

- [Pull Sync (Resolve)](./resolve) — server-to-server identity resolution
- [Trusted Server Integration](../integrations/trusted-server) — full setup guide
