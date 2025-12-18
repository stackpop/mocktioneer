#!/usr/bin/env bash
set -euo pipefail

# Start local with: (edgezero-cli serve --adapter fastly)
# Then run this script in another terminal. You can override the base URL by
# setting MOCKTIONEER_BASE_URL, e.g., MOCKTIONEER_BASE_URL=https://mocktioneer.edgecompute.app

BASE_URL="${MOCKTIONEER_BASE_URL:-http://127.0.0.1:7676}"
SIZE="${1:-300x250}"
CRID="${2:-demo}"
BID="${3:-2.5}"
PIXEL="${4:-true}"

REQUEST_URL="${BASE_URL}/static/creatives/${SIZE}.html?crid=${CRID}&bid=${BID}&pixel=${PIXEL}"

>&2 echo "Fetching creative iframe markup from ${REQUEST_URL}"
>&2 echo "(args: size=${SIZE}, crid=${CRID}, bid=${BID}, pixel=${PIXEL})"

curl -sS "${REQUEST_URL}"
