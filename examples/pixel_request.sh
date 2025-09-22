#!/usr/bin/env bash
set -euo pipefail

# Start local with: (fastly compute serve -C crates/mocktioneer-adapter-fastly)
# Then run this script in another terminal. Set MOCKTIONEER_BASE_URL to override
# the default http://127.0.0.1:7676 endpoint.
# Pass an optional format argument: base64 (default), raw, or hexdump.

BASE_URL="${MOCKTIONEER_BASE_URL:-http://127.0.0.1:7676}"
OUTPUT_FORMAT="${1:-base64}"
PIXEL_URL="${BASE_URL}/pixel"

>&2 echo "Requesting tracking pixel from ${PIXEL_URL}"

fetch_pixel() {
  curl -sS -D >(sed 's/^/[header] /' >&2) "${PIXEL_URL}"
}

case "${OUTPUT_FORMAT}" in
  base64)
    >&2 echo "Printing base64-encoded response body to stdout"
    fetch_pixel | base64
    ;;
  raw)
    >&2 echo "Streaming raw response body to stdout"
    fetch_pixel
    ;;
  hexdump)
    >&2 echo "Printing hex dump of response body to stdout"
    fetch_pixel | hexdump -C
    ;;
  *)
    >&2 echo "Unknown output format: ${OUTPUT_FORMAT} (use base64, raw, or hexdump)"
    exit 1
    ;;
esac
