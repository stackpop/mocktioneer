#!/usr/bin/env bash
set -euo pipefail

# Register mocktioneer as an EC sync partner in trusted-server.
#
# This is the setup step required before pixel sync, pull sync, or batch sync
# will work. Run it once against your trusted-server instance.
#
# Environment variables:
#   TS_BASE_URL             Trusted-server base URL (default: https://cdintel.com)
#   TS_ADMIN_USER           Basic Auth username for /_ts/admin/* routes
#   TS_ADMIN_PASS           Basic Auth password for /_ts/admin/* routes
#   MOCKTIONEER_BASE_URL    Mocktioneer base URL (default: https://origin-mocktioneer.cdintel.com)
#   MOCKTIONEER_API_KEY     API key for batch sync auth (default: mtk-demo-key-change-me)
#   MOCKTIONEER_PULL_TOKEN  Bearer token TS sends on pull sync calls (default: mtk-pull-token-change-me)

TS_BASE_URL="${TS_BASE_URL:-https://cdintel.com}"
TS_ADMIN_USER="${TS_ADMIN_USER:?Set TS_ADMIN_USER to the Basic Auth username}"
TS_ADMIN_PASS="${TS_ADMIN_PASS:?Set TS_ADMIN_PASS to the Basic Auth password}"
MOCKTIONEER_BASE_URL="${MOCKTIONEER_BASE_URL:-https://origin-mocktioneer.cdintel.com}"
MOCKTIONEER_API_KEY="${MOCKTIONEER_API_KEY:-mtk-demo-key-change-me}"
MOCKTIONEER_PULL_TOKEN="${MOCKTIONEER_PULL_TOKEN:-mtk-pull-token-change-me}"

# Extract hostname from mocktioneer URL for allowed_return_domains
MOCKTIONEER_HOST=$(echo "${MOCKTIONEER_BASE_URL}" | sed -E 's|https?://||' | sed -E 's|/.*||')

# Extract hostname from pull sync URL for pull_sync_allowed_domains
RESOLVE_URL="${MOCKTIONEER_BASE_URL}/resolve"
RESOLVE_HOST=$(echo "${RESOLVE_URL}" | sed -E 's|https?://||' | sed -E 's|/.*||' | sed -E 's|:.*||')

echo "Registering mocktioneer as EC partner at ${TS_BASE_URL}/_ts/admin/partners/register"
echo "  Mocktioneer host: ${MOCKTIONEER_HOST}"
echo "  Pull sync URL:    ${RESOLVE_URL}"
echo ""

curl -sS -w "\nHTTP %{http_code}\n" \
  -X POST "${TS_BASE_URL}/_ts/admin/partners/register" \
  -u "${TS_ADMIN_USER}:${TS_ADMIN_PASS}" \
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
