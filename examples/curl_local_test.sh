#!/usr/bin/env bash
set -euo pipefail

# Start local with: fastly compute serve (from this directory)
# Then run this script in another terminal.

REQ_FILE="$(dirname "$0")/request_banner.json"

echo "Posting OpenRTB request to http://127.0.0.1:7676/openrtb2/auction" >&2
curl -sS -X POST \
  -H 'Content-Type: application/json' \
  --data @"${REQ_FILE}" \
  http://127.0.0.1:7676/openrtb2/auction | jq .

