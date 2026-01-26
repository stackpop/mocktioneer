# Creatives & Assets

Mocktioneer serves creative assets that display the ad size and bid information for visual verification during testing.

## Creative HTML

### Endpoint

```
GET /static/creatives/{W}x{H}.html
```

Returns an HTML page that wraps the SVG creative and optionally includes tracking pixels.

### Parameters

| Parameter | Location | Type | Default | Description |
|-----------|----------|------|---------|-------------|
| `{W}x{H}` | Path | string | - | Size (e.g., `300x250`) |
| `pixel_html` | Query | boolean | `true` | Include HTML pixel |
| `pixel_js` | Query | boolean | `false` | Include JS pixel |

### Response

```html
<!DOCTYPE html>
<html>
<head>
  <style>
    body { margin: 0; overflow: hidden; }
    img { display: block; }
  </style>
</head>
<body>
  <a href="//localhost:8787/click?w=300&h=250" target="_blank">
    <img src="//localhost:8787/static/img/300x250.svg" 
         width="300" height="250" alt="300x250 creative">
  </a>
  <img src="//localhost:8787/pixel?pid=abc123" 
       data-static-pid="abc123" 
       width="1" height="1" style="display:none">
</body>
</html>
```

### Examples

```bash
# Default (with HTML pixel)
curl http://127.0.0.1:8787/static/creatives/300x250.html

# Without pixel
curl "http://127.0.0.1:8787/static/creatives/300x250.html?pixel_html=false"

# With both HTML and JS pixels
curl "http://127.0.0.1:8787/static/creatives/300x250.html?pixel_js=true"
```

## SVG Image

### Endpoint

```
GET /static/img/{W}x{H}.svg
```

Returns an SVG image displaying the size and optional bid amount.

### Parameters

| Parameter | Location | Type | Default | Description |
|-----------|----------|------|---------|-------------|
| `{W}x{H}` | Path | string | - | Size (e.g., `300x250`) |
| `bid` | Query | float | - | Bid amount to display |

### Response

The SVG displays:
- Ad size (e.g., "300x250")
- "mocktioneer" text
- Bid amount badge (if provided)

```xml
<svg xmlns="http://www.w3.org/2000/svg" width="300" height="250" viewBox="0 0 300 250">
  <rect width="100%" height="100%" fill="#f0f0f0"/>
  <text x="150" y="125" text-anchor="middle" font-size="24">300x250</text>
  <text x="150" y="150" text-anchor="middle" font-size="12">mocktioneer</text>
  <text x="280" y="20" text-anchor="end" font-size="10">$2.50</text>
</svg>
```

### Examples

```bash
# Without bid
curl http://127.0.0.1:8787/static/img/300x250.svg

# With bid amount
curl "http://127.0.0.1:8787/static/img/300x250.svg?bid=2.50"

# Different size
curl http://127.0.0.1:8787/static/img/728x90.svg
```

## Embedding Creatives

### In iframe (from auction response)

The `adm` field in auction responses contains ready-to-use iframe HTML:

```html
<iframe 
  src="//mocktioneer.edgecompute.app/static/creatives/300x250.html?crid=demo" 
  width="300" 
  height="250" 
  frameborder="0" 
  scrolling="no">
</iframe>
```

### Direct embed

```html
<!-- Full creative with tracking -->
<iframe 
  src="//mocktioneer.edgecompute.app/static/creatives/300x250.html" 
  width="300" 
  height="250" 
  frameborder="0">
</iframe>

<!-- SVG only (no tracking) -->
<img 
  src="//mocktioneer.edgecompute.app/static/img/300x250.svg?bid=2.50" 
  width="300" 
  height="250">
```

## Supported Sizes

| Size | Description |
|------|-------------|
| 300x250 | Medium Rectangle |
| 320x50 | Mobile Leaderboard |
| 728x90 | Leaderboard |
| 160x600 | Wide Skyscraper |
| 300x50 | Mobile Banner |
| 300x600 | Half Page |
| 970x250 | Billboard |
| 468x60 | Full Banner |
| 336x280 | Large Rectangle |
| 320x100 | Large Mobile Banner |

## Error Responses

### Non-Standard Size (404)

```bash
curl http://127.0.0.1:8787/static/creatives/999x999.html
# Returns 404 Not Found
```

### Invalid Format (422)

```bash
curl http://127.0.0.1:8787/static/img/invalid.svg
# Returns 422 Unprocessable Entity
```

### Negative Bid (422)

```bash
curl "http://127.0.0.1:8787/static/img/300x250.svg?bid=-1"
# Returns 422 Unprocessable Entity
```

## Using Example Script

```bash
# Fetch creative HTML
./examples/iframe_request.sh 300x250

# With specific parameters
./examples/iframe_request.sh 728x90 my-crid 3.50 true
```

## Content Types

| Endpoint | Content-Type |
|----------|--------------|
| `/static/creatives/*.html` | `text/html; charset=utf-8` |
| `/static/img/*.svg` | `image/svg+xml` |

## Caching

Creative assets are cacheable. Responses include appropriate headers for CDN caching. The `Host` header affects the generated URLs, so ensure consistent host values for cache efficiency.
