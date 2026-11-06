#!/bin/bash
# End-to-end Pactara demo on NEAR testnet.
# Usage:
#   ./demo.sh           # PASS path: create, submit, verify, settle
#   ./demo.sh fail      # FAIL path: invalid artifact is refunded
#
# Override accounts with PACTARA_CONTRACT, PACTARA_WORKER, PACTARA_SIGNER.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
CONTRACT="${PACTARA_CONTRACT:-pactara-v9.testnet}"
WORKER="${PACTARA_WORKER:-pactara-v6.testnet}"
SIGNER="${PACTARA_SIGNER:-$CONTRACT}"
MODE="${1:-pass}"
NETWORK="${NEAR_NETWORK:-testnet}"
PYTHON="${PYTHON:-python3}"

if [[ "$MODE" != "pass" && "$MODE" != "fail" ]]; then
  echo "Usage: $0 [pass|fail]" >&2
  exit 1
fi

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Missing dependency: $1" >&2
    exit 1
  }
}

need near
need "$PYTHON"
need grep

"$PYTHON" -c "import nacl, base58" 2>/dev/null || {
  echo "Install Python deps: pip install pynacl base58" >&2
  exit 1
}

near_call() {
  local method="$1"
  local args="$2"
  local signer="$3"
  local gas="${4:-50.0 Tgas}"
  local deposit="${5:-0 NEAR}"
  near contract call-function as-transaction "$CONTRACT" "$method" \
    json-args "$args" prepaid-gas "$gas" attached-deposit "$deposit" \
    sign-as "$signer" network-config "$NETWORK" sign-with-keychain send
}

near_view() {
  local method="$1"
  local args="$2"
  near contract call-function as-read-only "$CONTRACT" "$method" \
    json-args "$args" network-config "$NETWORK" now
}

extract_task_id() {
  local output="$1"
  local from_log
  from_log="$(printf '%s\n' "$output" | grep -Eo 'Task [0-9]+ created' | tail -n1 | awk '{print $2}' || true)"
  if [[ -n "${from_log:-}" ]]; then
    echo "$from_log"
    return
  fi
  printf '%s\n' "$output" | grep -Eo 'Function execution return value:[[:space:]]*[0-9]+' | tail -n1 | awk '{print $NF}'
}

view_bool() {
  local output="$1"
  if printf '%s\n' "$output" | grep -Eq 'return value:[[:space:]]*true'; then
    echo true
  else
    echo false
  fi
}

print_treasury() {
  echo "Treasury ($1):"
  near_view get_treasury '{}'
}

WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/pactara-demo.XXXXXX")"
cleanup() {
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

chmod 700 "$WORKDIR"
KEY_FILE="$WORKDIR/verifier.sk"
CRITERIA="$WORKDIR/criteria.json"
INPUT="$WORKDIR/input.json"
ARTIFACT="$WORKDIR/artifact.json"

"$PYTHON" - <<PY
from nacl.signing import SigningKey
sk = SigningKey.generate()
open(r"$KEY_FILE", "w", encoding="utf-8").write(sk.encode().hex())
PY
chmod 600 "$KEY_FILE"
export PACTARA_VERIFIER_KEY_FILE="$KEY_FILE"
PUBKEY="$("$PYTHON" "$ROOT/verifier.py" pubkey)"

cat > "$CRITERIA" <<'JSON'
{"schema":{"summary":{"type":"string","minLength":8,"required":true},"score":{"type":"integer","min":1,"max":100,"required":true},"ok":{"type":"boolean","required":true}}}
JSON

cat > "$INPUT" <<'JSON'
{"prompt":"Summarize Pactara in one sentence.","max_words":30}
JSON

if [[ "$MODE" == "pass" ]]; then
  cat > "$ARTIFACT" <<'JSON'
{"summary":"Pactara reserves funds and pays only after a registered verifier signs.","score":92,"ok":true}
JSON
else
  cat > "$ARTIFACT" <<'JSON'
{"summary":"no","score":true,"ok":1}
JSON
fi

CRITERIA_HASH="$("$PYTHON" "$ROOT/verifier.py" hash-file "$CRITERIA" --format near)"
INPUT_HASH="$("$PYTHON" "$ROOT/verifier.py" hash-file "$INPUT" --format near)"
ARTIFACT_HASH="$("$PYTHON" "$ROOT/verifier.py" hash-file "$ARTIFACT" --format near)"
DEADLINE="$(( $(date +%s) * 1000000000 + 3600 * 1000000000 ))"

echo "Contract : $CONTRACT"
echo "Worker   : $WORKER"
echo "Signer   : $SIGNER"
echo "Mode     : $MODE"
echo "Verifier : $PUBKEY"
echo "Deadline : $DEADLINE"
echo "Workdir  : $WORKDIR"

print_treasury "before"

echo "=== Authorize worker if needed ==="
WORKER_AUTH="$(view_bool "$(near_view is_authorized_worker "{\"account\":\"$WORKER\"}")")"
if [[ "$WORKER_AUTH" != "true" ]]; then
  echo "Worker $WORKER is not authorized; adding it."
  near_call add_authorized_worker "{\"worker\":\"$WORKER\"}" "$SIGNER"
else
  echo "Worker $WORKER is authorized."
fi

BUYER_AUTH="$(view_bool "$(near_view is_authorized_buyer "{\"account\":\"$SIGNER\"}")")"
if [[ "$BUYER_AUTH" != "true" ]]; then
  echo "Signer $SIGNER is not an authorized buyer; adding it."
  near_call add_authorized_buyer "{\"buyer\":\"$SIGNER\"}" "$SIGNER"
fi

echo "=== Register ephemeral verifier key ==="
near_call register_verifier "{\"public_key\":\"$PUBKEY\"}" "$SIGNER"

echo "=== Deposit 1 NEAR ==="
near_call deposit '{}' "$SIGNER" "30.0 Tgas" "1 NEAR"

print_treasury "after deposit"

echo "=== Create task ==="
CREATE_OUT="$(near_call create_task "{\"worker\":\"$WORKER\",\"amount\":\"100000000000000000000000\",\"criteria_hash\":$CRITERIA_HASH,\"input_hash\":$INPUT_HASH,\"verification_deadline\":$DEADLINE}" "$SIGNER" "50.0 Tgas")"
printf '%s\n' "$CREATE_OUT"
TASK_ID="$(extract_task_id "$CREATE_OUT")"
if [[ -z "${TASK_ID:-}" ]]; then
  echo "Could not parse task id from create_task output" >&2
  exit 1
fi
echo "Using task id $TASK_ID"

echo "=== Worker submits artifact ==="
near_call submit_artifact "{\"task_id\":$TASK_ID,\"artifact_hash\":$ARTIFACT_HASH}" "$WORKER" "40.0 Tgas"

echo "=== On-chain task before verification ==="
near_view get_task "{\"task_id\":$TASK_ID}"

echo "=== Verifier evaluates and submits signed result ==="
"$PYTHON" "$ROOT/verifier.py" submit \
  --contract "$CONTRACT" \
  --task-id "$TASK_ID" \
  --criteria "$CRITERIA" \
  --input "$INPUT" \
  --artifact "$ARTIFACT" \
  --account "$SIGNER" \
  --expect-worker "$WORKER"

echo "=== Final task state ==="
near_view get_task_state "{\"task_id\":$TASK_ID}"
near_view get_task "{\"task_id\":$TASK_ID}"
print_treasury "after settlement"

if [[ "$MODE" == "pass" ]]; then
  echo "Expected state: Settled (PASS -> PayoutPending -> callback)"
else
  echo "Expected state: Refunded (FAIL verification restores reserved funds)"
fi
