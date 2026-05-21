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
#   MOCKTIONEER_API_KEY     API key for batch sync auth (required)
#   MOCKTIONEER_PULL_TOKEN  Bearer token TS sends on pull sync calls (required)

command -v jq >/dev/null 2>&1 || {
  echo "jq is required to build the registration payload" >&2
  exit 1
}

TS_BASE_URL="${TS_BASE_URL:-https://cdintel.com}"
TS_BASE_URL="${TS_BASE_URL%/}"
TS_ADMIN_USER="${TS_ADMIN_USER:?Set TS_ADMIN_USER to the Basic Auth username}"
TS_ADMIN_PASS="${TS_ADMIN_PASS:?Set TS_ADMIN_PASS to the Basic Auth password}"
MOCKTIONEER_BASE_URL="${MOCKTIONEER_BASE_URL:-https://origin-mocktioneer.cdintel.com}"
MOCKTIONEER_BASE_URL="${MOCKTIONEER_BASE_URL%/}"
MOCKTIONEER_API_KEY="${MOCKTIONEER_API_KEY:?Set MOCKTIONEER_API_KEY to the batch sync API key}"
MOCKTIONEER_PULL_TOKEN="${MOCKTIONEER_PULL_TOKEN:?Set MOCKTIONEER_PULL_TOKEN to the pull sync bearer token}"

host_of() {
  local value="$1"
  value="${value#http://}"
  value="${value#https://}"
  value="${value%%/*}"
  value="${value##*@}"

  if [[ "${value}" == \[*\]* ]]; then
    value="${value#\[}"
    value="${value%%\]*}"
  else
    value="${value%%:*}"
  fi

  printf '%s\n' "${value}"
}

# Extract hostnames for trusted-server partner allowlists.
MOCKTIONEER_HOST="$(host_of "${MOCKTIONEER_BASE_URL}")"
RESOLVE_URL="${MOCKTIONEER_BASE_URL}/resolve"
RESOLVE_HOST="$(host_of "${RESOLVE_URL}")"

echo "Registering mocktioneer as EC partner at ${TS_BASE_URL}/_ts/admin/partners/register"
echo "  Mocktioneer host: ${MOCKTIONEER_HOST}"
echo "  Pull sync URL:    ${RESOLVE_URL}"
echo ""

jq -n \
  --arg mocktioneer_host "${MOCKTIONEER_HOST}" \
  --arg api_key "${MOCKTIONEER_API_KEY}" \
  --arg resolve_url "${RESOLVE_URL}" \
  --arg resolve_host "${RESOLVE_HOST}" \
  --arg pull_token "${MOCKTIONEER_PULL_TOKEN}" \
  '{
    id: "mocktioneer",
    name: "Mocktioneer Mock DSP",
    allowed_return_domains: [$mocktioneer_host],
    api_key: $api_key,
    bidstream_enabled: true,
    source_domain: "mocktioneer.dev",
    openrtb_atype: 3,
    sync_rate_limit: 100,
    batch_rate_limit: 60,
    pull_sync_enabled: true,
    pull_sync_url: $resolve_url,
    pull_sync_allowed_domains: [$resolve_host],
    pull_sync_ttl_sec: 86400,
    pull_sync_rate_limit: 10,
    ts_pull_token: $pull_token
  }' |
  curl -sS -w "\nHTTP %{http_code}\n" \
    -X POST "${TS_BASE_URL}/_ts/admin/partners/register" \
    -u "${TS_ADMIN_USER}:${TS_ADMIN_PASS}" \
    -H "Content-Type: application/json" \
    --data @-
