#!/usr/bin/env bash
set -euo pipefail

# Register mocktioneer as an EC sync partner in trusted-server.
#
# This is the setup step required before pixel sync, pull sync, or batch sync
# will work. Run it once against your trusted-server instance.
#
# Environment variables:
#   TS_BASE_URL           Trusted-server base URL (default: https://ts.publisher.com)
#   TS_ADMIN_TOKEN        Publisher admin bearer token for /admin/partners/register
#   MOCKTIONEER_BASE_URL  Mocktioneer base URL (default: http://localhost:8787)
#   MOCKTIONEER_API_KEY   API key for batch sync auth (default: mtk-demo-key-change-me)
#   MOCKTIONEER_PULL_TOKEN  Bearer token TS sends on pull sync calls (default: mtk-pull-token-change-me)

TS_BASE_URL="${TS_BASE_URL:-https://ts.publisher.com}"
TS_ADMIN_TOKEN="${TS_ADMIN_TOKEN:?Set TS_ADMIN_TOKEN to your publisher admin token}"
MOCKTIONEER_BASE_URL="${MOCKTIONEER_BASE_URL:-http://localhost:8787}"
MOCKTIONEER_API_KEY="${MOCKTIONEER_API_KEY:-mtk-demo-key-change-me}"
MOCKTIONEER_PULL_TOKEN="${MOCKTIONEER_PULL_TOKEN:-mtk-pull-token-change-me}"

# Extract hostname from mocktioneer URL for allowed_return_domains
MOCKTIONEER_HOST=$(echo "${MOCKTIONEER_BASE_URL}" | sed -E 's|https?://||' | sed -E 's|/.*||')

# Extract hostname from pull sync URL for pull_sync_allowed_domains
RESOLVE_URL="${MOCKTIONEER_BASE_URL}/resolve"
RESOLVE_HOST=$(echo "${RESOLVE_URL}" | sed -E 's|https?://||' | sed -E 's|/.*||' | sed -E 's|:.*||')

echo "Registering mocktioneer as EC partner at ${TS_BASE_URL}/admin/partners/register"
echo "  Mocktioneer host: ${MOCKTIONEER_HOST}"
echo "  Pull sync URL:    ${RESOLVE_URL}"
echo ""

curl -sS -w "\nHTTP %{http_code}\n" \
  -X POST "${TS_BASE_URL}/admin/partners/register" \
  -H "Authorization: Bearer ${TS_ADMIN_TOKEN}" \
  -H "Content-Type: application/json" \
  -d @- <<EOF
{
  "id": "mocktioneer",
  "name": "Mocktioneer Mock DSP",
  "allowed_return_domains": ["${MOCKTIONEER_HOST}"],
  "api_key": "${MOCKTIONEER_API_KEY}",
  "bidstream_enabled": true,
  "source_domain": "mocktioneer.dev",
  "openrtb_atype": 3,
  "sync_rate_limit": 100,
  "batch_rate_limit": 60,
  "pull_sync_enabled": true,
  "pull_sync_url": "${RESOLVE_URL}",
  "pull_sync_allowed_domains": ["${RESOLVE_HOST}"],
  "pull_sync_ttl_sec": 86400,
  "pull_sync_rate_limit": 10,
  "ts_pull_token": "${MOCKTIONEER_PULL_TOKEN}"
}
EOF
