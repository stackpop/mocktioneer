#!/usr/bin/env bash
set -euo pipefail

# Start local with: (anyedge-cli serve --adapter fastly)
# Then run this script in another terminal. Override the base URL by setting
# MOCKTIONEER_BASE_URL, e.g., MOCKTIONEER_BASE_URL=https://mocktioneer.edgecompute.app
# Pass a custom payload file as the first argument or a custom endpoint path
# (relative to the base URL) as the second argument if needed.

BASE_URL="${MOCKTIONEER_BASE_URL:-http://127.0.0.1:7676}"
SCRIPT_DIR="$(cd -- "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REQUEST_PAYLOAD="${1:-${SCRIPT_DIR}/openrtb_request.json}"
ENDPOINT_PATH="${2:-/openrtb2/auction}"
REQUEST_URL="${BASE_URL}${ENDPOINT_PATH}"

if [[ ! -f "${REQUEST_PAYLOAD}" ]]; then
  >&2 echo "Request payload not found: ${REQUEST_PAYLOAD}"
  exit 1
fi

>&2 echo "Posting OpenRTB request from ${REQUEST_PAYLOAD} to ${REQUEST_URL}"

curl -sS -X POST \
  -H 'Content-Type: application/json' \
  --data @"${REQUEST_PAYLOAD}" \
  "${REQUEST_URL}" | jq .
