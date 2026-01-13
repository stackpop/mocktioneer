#!/usr/bin/env bash
set -euo pipefail

MOCKTIONEER_BASE_URL="${MOCKTIONEER_BASE_URL:-http://127.0.0.1:7676}"
PAYLOAD_FILE="${1:-}"

if [ -n "$PAYLOAD_FILE" ] && [ -f "$PAYLOAD_FILE" ]; then
  PAYLOAD=$(cat "$PAYLOAD_FILE")
else
  PAYLOAD='{
    "pubId": "1234",
    "slots": [
      {
        "slotID": "header-banner",
        "slotName": "header-banner",
        "sizes": [[728, 90], [970, 250]]
      },
      {
        "slotID": "sidebar",
        "slotName": "sidebar",
        "sizes": [[300, 250]]
      }
    ],
    "pageUrl": "https://example.com/article",
    "ua": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
    "timeout": 800
  }'
fi

echo "→ Posting APS bid request to ${MOCKTIONEER_BASE_URL}/e/dtb/bid"
echo ""

curl -sS -X POST \
  -H 'Content-Type: application/json' \
  --data "$PAYLOAD" \
  "${MOCKTIONEER_BASE_URL}/e/dtb/bid" | jq .

echo ""
echo "✓ APS bid request complete"
