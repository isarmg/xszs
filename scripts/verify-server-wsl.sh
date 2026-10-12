#!/usr/bin/env bash
set -euo pipefail

base_url="${XSZS_VERIFY_URL:-http://127.0.0.1:8080}"

health_status="$(curl --silent --output /dev/null --write-out '%{http_code}' \
  "$base_url/healthz")"
admin_status="$(curl --silent --output /dev/null --write-out '%{http_code}' \
  "$base_url/admin")"
readiness="$(curl --silent --show-error --fail "$base_url/readyz")"

printf 'health=%s admin_page=%s\n' "$health_status" "$admin_status"
test "$health_status" = "204"
test "$readiness" = '{"ready":true}'
test "$admin_status" = "200"
