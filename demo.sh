#!/bin/bash
# End-to-end Pactara demo on NEAR testnet.
# Usage:
#   ./demo.sh pass   # valid artifact → Settled
#   ./demo.sh fail   # invalid artifact → Refunded
#   ./demo.sh deny   # policy violations must be rejected on-chain
#
# PACTARA_CONTRACT  PACTARA_WORKER  PACTARA_SIGNER  NEAR_NETWORK

set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
CONTRACT="${PACTARA_CONTRACT:-pactara-v9.testnet}"
WORKER="${PACTARA_WORKER:-pactara-v6.testnet}"
SIGNER="${PACTARA_SIGNER:-$CONTRACT}"
MODE="${1:-pass}"
NETWORK="${NEAR_NETWORK:-testnet}"
PYTHON="${PYTHON:-python3}"
AMOUNT="100000000000000000000000"

if [[ "$MODE" != "pass" && "$MODE" != "fail" && "$MODE" != "deny" ]]; then
  echo "Usage: $0 [pass|fail|deny]" >&2
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

expect_reject() {
  local label="$1"
  shift
  echo "=== Expect reject: $label ==="
  if "$@"; then
    echo "FAIL: $label was accepted on-chain" >&2
    exit 1
  fi
  echo "Rejected on-chain: $label"
}

print_treasury() {
  echo "Treasury ($1):"
  near_view get_treasury '{}'
}

require_state() {
  local task_id="$1"
  local wanted="$2"
  local out
  out="$(near_view get_task_state "{\"task_id\":$task_id}")"
  printf '%s\n' "$out"
  if ! printf '%s\n' "$out" | grep -Eq "$wanted"; then
    echo "On-chain state was not $wanted" >&2
    exit 1
  fi
}

echo "Contract : $CONTRACT"
echo "Worker   : $WORKER"
echo "Signer   : $SIGNER"
echo "Mode     : $MODE"

VERSION_OUT="$(near_view get_code_version '{}' || true)"
if ! printf '%s\n' "$VERSION_OUT" | grep -Fq 'pactara-2.0.0'; then
  echo "Contract $CONTRACT is not running pactara-2.0.0 (get_code_version missing or mismatched)." >&2
  echo "This repo's source is not the bytecode on that account. Deploy to a fresh account." >&2
  printf '%s\n' "$VERSION_OUT" >&2
  exit 1
fi

if [[ "$MODE" == "deny" ]]; then
  echo "=== Scenario C: unauthorized spending must fail on-chain ==="
  near_view get_treasury '{}' >/dev/null
  expect_reject "unauthorized worker" \
    near_call create_task "{\"worker\":\"not-a-worker.testnet\",\"amount\":\"$AMOUNT\",\"criteria_hash\":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],\"input_hash\":[2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2],\"verification_deadline\":$(( $(date +%s) * 1000000000 + 3600000000000 ))}" "$SIGNER" "50.0 Tgas"
  expect_reject "amount above per-task limit" \
    near_call create_task "{\"worker\":\"$WORKER\",\"amount\":\"999000000000000000000000000\",\"criteria_hash\":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],\"input_hash\":[2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2],\"verification_deadline\":$(( $(date +%s) * 1000000000 + 3600000000000 ))}" "$SIGNER" "50.0 Tgas"
  expect_reject "worker cannot set_policy" \
    near_call set_policy '{"max_amount_per_task":"1","daily_limit":"1"}' "$WORKER" "30.0 Tgas"
  expect_reject "worker cannot withdraw" \
    near_call withdraw '{"amount":"1"}' "$WORKER" "30.0 Tgas"
  echo "Scenario C complete: contract rejected the unauthorized actions."
  exit 0
fi

WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/pactara-demo.XXXXXX")"
cleanup() { rm -rf "$WORKDIR"; }
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

echo "Verifier : $PUBKEY"
echo "Deadline : $DEADLINE"
print_treasury "before"

WORKER_AUTH="$(view_bool "$(near_view is_authorized_worker "{\"account\":\"$WORKER\"}")")"
if [[ "$WORKER_AUTH" != "true" ]]; then
  echo "Worker $WORKER is not authorized; adding it."
  near_call add_authorized_worker "{\"worker\":\"$WORKER\"}" "$SIGNER"
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
CREATE_OUT="$(near_call create_task "{\"worker\":\"$WORKER\",\"amount\":\"$AMOUNT\",\"criteria_hash\":$CRITERIA_HASH,\"input_hash\":$INPUT_HASH,\"verification_deadline\":$DEADLINE}" "$SIGNER" "50.0 Tgas")"
printf '%s\n' "$CREATE_OUT"
TASK_ID="$(extract_task_id "$CREATE_OUT")"
if [[ -z "${TASK_ID:-}" ]]; then
  echo "Could not parse task id from create_task output" >&2
  exit 1
fi
echo "Using task id $TASK_ID"
require_state "$TASK_ID" "Funded"

echo "=== Worker submits artifact ==="
near_call submit_artifact "{\"task_id\":$TASK_ID,\"artifact_hash\":$ARTIFACT_HASH}" "$WORKER" "40.0 Tgas"
require_state "$TASK_ID" "Submitted"
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

echo "=== Final on-chain state ==="
near_view get_task "{\"task_id\":$TASK_ID}"
print_treasury "after settlement"
if [[ "$MODE" == "pass" ]]; then
  require_state "$TASK_ID" "Settled"
  echo "Scenario A complete: task $TASK_ID is Settled."
else
  require_state "$TASK_ID" "Refunded"
  echo "Scenario B complete: task $TASK_ID is Refunded."
fi
