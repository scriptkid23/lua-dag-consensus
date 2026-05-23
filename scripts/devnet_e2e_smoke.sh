#!/usr/bin/env bash
# Poll node0 JSON-RPC until L3 macro finality is observable (spec §8 E2E).
#
# Prerequisites: four-node `docker compose up -d --build` with node0 RPC on
# host port 9200 (see docker-compose.yml). Requires curl + jq.
#
# Usage:
#   ./scripts/devnet_e2e_smoke.sh
#   RPC_URL=http://127.0.0.1:9200/ FINALIZE_TIMEOUT_SECS=240 ./scripts/devnet_e2e_smoke.sh

set -euo pipefail

RPC_URL="${RPC_URL:-http://127.0.0.1:9200/}"
FINALIZE_TIMEOUT_SECS="${FINALIZE_TIMEOUT_SECS:-300}"
POLL_INTERVAL_SECS="${POLL_INTERVAL_SECS:-5}"

rpc_post() {
  local method="$1"
  local params="${2:-[]}"
  curl -fsS -X POST "$RPC_URL" \
    -H 'content-type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"${method}\",\"params\":${params}}"
}

echo "devnet E2E: polling ${RPC_URL} for lua_getLatestFinalized (timeout ${FINALIZE_TIMEOUT_SECS}s)"

deadline=$(( $(date +%s) + FINALIZE_TIMEOUT_SECS ))
attempt=0

while :; do
  attempt=$((attempt + 1))
  if ! result=$(rpc_post "lua_getLatestFinalized" 2>&1); then
    echo "attempt ${attempt}: RPC unreachable (${result}); retrying in ${POLL_INTERVAL_SECS}s"
  else
    hash=$(echo "$result" | jq -r '.result.checkpoint_hash // empty')
    mode=$(echo "$result" | jq -r '.result.mode // empty')
    if [ -n "$hash" ] && [ "$hash" != "null" ]; then
      echo "consensus progressed: checkpoint_hash=${hash} mode=${mode}"
      mc=$(rpc_post "lua_getMacroCheckpointAt" '[1]')
      mc_hex=$(echo "$mc" | jq -r '.result.checkpoint_borsh_hex // empty')
      if [ -n "$mc_hex" ] && [ "$mc_hex" != "null" ]; then
        echo "macro checkpoint at height 1 present (${#mc_hex} hex chars)"
      else
        echo "warning: lua_getMacroCheckpointAt(1) returned null (finalized QC still OK)"
      fi
      exit 0
    fi
    echo "attempt ${attempt}: latest_finalized still null; retrying in ${POLL_INTERVAL_SECS}s"
  fi

  if [ "$(date +%s)" -gt "$deadline" ]; then
    echo "Timed out after ${FINALIZE_TIMEOUT_SECS}s waiting for macro finality"
    exit 1
  fi
  sleep "$POLL_INTERVAL_SECS"
done
