#!/bin/bash
# Pactara buyer helper: create a task with a restricted access key.
# For a full create → submit → verify → settle flow, use ./demo.sh.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
CONTRACT="${PACTARA_CONTRACT:-pactara-v9.testnet}"
WORKER="${PACTARA_WORKER:-pactara-v6.testnet}"
RESTRICTED_KEY="${PACTARA_RESTRICTED_KEY:-$HOME/.near-credentials/testnet/pactara-v6.testnet/ed25519_FcgWnB8ffKw41KWYHPQoWjkG3pjYvL6b8W51Wq2tjzdh.json}"
NETWORK="${NEAR_NETWORK:-testnet}"
PYTHON="${PYTHON:-python3}"

WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/pactara-buyer.XXXXXX")"
trap 'rm -rf "$WORKDIR"' EXIT

cat > "$WORKDIR/criteria.json" <<'JSON'
{"schema":{"summary":{"type":"string","minLength":8,"required":true}}}
JSON
cat > "$WORKDIR/input.json" <<'JSON'
{"prompt":"Summarize Pactara in one sentence."}
JSON

CRITERIA_HASH="$("$PYTHON" "$ROOT/verifier.py" hash-file "$WORKDIR/criteria.json" --format near)"
INPUT_HASH="$("$PYTHON" "$ROOT/verifier.py" hash-file "$WORKDIR/input.json" --format near)"
DEADLINE="$(( $(date +%s) * 1000000000 + 3600 * 1000000000 ))"

echo "Contract : $CONTRACT"
echo "Worker   : $WORKER"
echo "Deadline : $DEADLINE"

near contract call-function as-read-only "$CONTRACT" get_treasury \
  json-args '{}' network-config "$NETWORK" now

near contract call-function as-transaction "$CONTRACT" create_task \
  json-args "{\"worker\":\"$WORKER\",\"amount\":\"100000000000000000000000\",\"criteria_hash\":$CRITERIA_HASH,\"input_hash\":$INPUT_HASH,\"verification_deadline\":$DEADLINE}" \
  prepaid-gas '50 Tgas' \
  attached-deposit '0 NEAR' \
  sign-as "$CONTRACT" \
  network-config "$NETWORK" \
  sign-with-access-key-file "$RESTRICTED_KEY" \
  send
