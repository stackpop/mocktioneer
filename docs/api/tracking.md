# Tracking Endpoints

Mocktioneer provides pixel and click tracking endpoints for testing impression and click tracking flows.

## Pixel Endpoint

### Endpoint

```
GET /pixel?pid={id}
```

Returns a 1x1 transparent GIF and optionally sets a tracking cookie.

### Parameters

| Parameter | Location | Type | Required | Description |
|-----------|----------|------|----------|-------------|
| `pid` | Query | string | Yes | Pixel ID (1-128 chars) |

### Behavior

1. If no `mtkid` cookie exists, sets one with a UUIDv7 value
2. Returns a 1x1 transparent GIF
3. Sets cache-control headers to prevent caching

### Response Headers

```
Content-Type: image/gif
Content-Length: 43
Cache-Control: no-store, no-cache, must-revalidate, max-age=0
Pragma: no-cache
Set-Cookie: mtkid=019abc123...; Path=/; Max-Age=31536000; SameSite=None; Secure; HttpOnly
```

The `Set-Cookie` header is only present when creating a new cookie.

### Cookie Details

| Property | Value |
|----------|-------|
| Name | `mtkid` |
| Value | UUIDv7 |
| Path | `/` |
| Max-Age | 31536000 (1 year) |
| SameSite | None |
| Secure | Yes |
| HttpOnly | Yes |

### Examples

```bash
# Basic pixel request
curl -v "http://127.0.0.1:8787/pixel?pid=test123"

# With existing cookie (no new cookie set)
curl -v "http://127.0.0.1:8787/pixel?pid=test123" \
  -H "Cookie: mtkid=existing-value"

# Save response to file
curl -o pixel.gif "http://127.0.0.1:8787/pixel?pid=test123"
```

### Using Example Script

```bash
# Default output (base64)
./examples/pixel_request.sh

# Raw binary output
./examples/pixel_request.sh raw

# Hexdump output
./examples/pixel_request.sh hexdump

# Custom pixel ID
MOCKTIONEER_PIXEL_ID=my-custom-id ./examples/pixel_request.sh
```

### Error Responses

#### Missing pid (400)

```bash
curl http://127.0.0.1:8787/pixel
# Returns 400 Bad Request
```

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "pid: missing required field"
  }
}
```

#### pid Too Long (422)

```bash
curl "http://127.0.0.1:8787/pixel?pid=$(python -c 'print("x"*200)')"
# Returns 422 Unprocessable Entity
```

---

## Click Endpoint

### Endpoint

```
GET /click
```

Returns an HTML page that echoes the creative metadata. Used as a click landing page for testing.

### Parameters

| Parameter | Location | Type | Required | Description |
|-----------|----------|------|----------|-------------|
| `crid` | Query | string | No | Creative ID (max 128 chars) |
| `w` | Query | integer | No | Width (min 1) |
| `h` | Query | integer | No | Height (min 1) |
| `*` | Query | any | No | Additional parameters are echoed |

### Response

Returns an HTML page displaying:
- Creative ID
- Dimensions
- Any additional query parameters

```html
<!DOCTYPE html>
<html>
<head>
  <title>Click Landing Page</title>
</head>
<body>
  <h1>Mocktioneer Click</h1>
  <dl>
    <dt>Creative ID</dt>
    <dd>demo-creative</dd>
    <dt>Size</dt>
    <dd>300x250</dd>
  </dl>
  <h2>Additional Parameters</h2>
  <dl>
    <dt>campaign</dt>
    <dd>summer-sale</dd>
  </dl>
</body>
</html>
```

### Examples

```bash
# Basic click
curl "http://127.0.0.1:8787/click?crid=demo&w=300&h=250"

# With additional parameters
curl "http://127.0.0.1:8787/click?crid=demo&w=300&h=250&campaign=test&source=email"

# No parameters (empty values)
curl http://127.0.0.1:8787/click
```

### Click URL in Creatives

The creative HTML wraps the image in a link to the click endpoint:

```html
<a href="//localhost:8787/click?w=300&h=250&crid=demo" target="_blank">
  <img src="//localhost:8787/static/img/300x250.svg" ...>
</a>
```

---

## Testing Tracking Flows

### Full Impression Flow

1. Receive auction response with creative URL
2. Load creative HTML (includes pixel)
3. Pixel fires automatically
4. Cookie is set (if not present)

```bash
# Simulate flow
# 1. Get auction response
BID=$(curl -s -X POST http://127.0.0.1:8787/openrtb2/auction \
  -H 'Content-Type: application/json' \
  -d '{"id":"test","imp":[{"id":"1","banner":{"w":300,"h":250}}]}')

# 2. Extract creative URL
CREATIVE_URL=$(echo $BID | jq -r '.seatbid[0].bid[0].adm' | grep -oP 'src="\K[^"]+')

# 3. Fetch creative (triggers pixel)
curl -v "$CREATIVE_URL"
```

### Click Flow

1. User clicks creative
2. Click endpoint is hit
3. Landing page displays metadata

```bash
# Simulate click
curl "http://127.0.0.1:8787/click?crid=test-creative&w=300&h=250"
```

### Verifying Cookie Behavior

```bash
# First request - sets cookie
curl -v "http://127.0.0.1:8787/pixel?pid=test1" 2>&1 | grep -i set-cookie

# Second request with cookie - no new cookie
curl -v "http://127.0.0.1:8787/pixel?pid=test2" \
  -H "Cookie: mtkid=abc123" 2>&1 | grep -i set-cookie
# (no output - cookie not reset)
```
