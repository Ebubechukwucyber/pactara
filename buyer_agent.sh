#!/bin/bash
# Pactara buyer helper: create a task with a restricted function-call access key.
# The contract remains the policy authority; this script only fails early.
#
# Required configuration:
#   PACTARA_CONTRACT        contract account (default: pactara-v10.testnet)
#   PACTARA_WORKER          worker account (default: pactara-v6.testnet)
#   PACTARA_RESTRICTED_KEY  path to the buyer access-key JSON
#   PACTARA_BUYER           signer account; must match account_id in the key file
#
# The restricted key must belong to PACTARA_BUYER. It should allow only
# create_task on PACTARA_CONTRACT. Inspect permissions with:
#   near account list-keys "$PACTARA_BUYER" network-config testnet now
#
# Add a function-call key (example, near-cli-rs):
#   near account add-key "$PACTARA_BUYER" grant function-call \
#     --contract-account-id "$PACTARA_CONTRACT" \
#     --method-names create_task \
#     network-config testnet sign-with-keychain send

set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
CONTRACT="${PACTARA_CONTRACT:-pactara-v10.testnet}"
WORKER="${PACTARA_WORKER:-pactara-v6.testnet}"
NETWORK="${NEAR_NETWORK:-testnet}"
PYTHON="${PYTHON:-python3}"
AMOUNT="${PACTARA_AMOUNT:-100000000000000000000000}"

if [[ -z "${PACTARA_RESTRICTED_KEY:-}" ]]; then
  echo "Set PACTARA_RESTRICTED_KEY to the buyer function-call key JSON." >&2
  echo "Do not point this at a full-access key or at another account's credentials." >&2
  exit 1
fi
RESTRICTED_KEY="$PACTARA_RESTRICTED_KEY"

if [[ ! -f "$RESTRICTED_KEY" ]]; then
  echo "Restricted key file not found: $RESTRICTED_KEY" >&2
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

KEY_META="$("$PYTHON" - "$RESTRICTED_KEY" <<'PY'
import json, sys
path = sys.argv[1]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle)
account = data.get("account_id") or data.get("accountId")
public = data.get("public_key") or data.get("publicKey")
if not account or not public:
    raise SystemExit("Key file must contain account_id and public_key")
print(account)
print(public)
PY
)"
KEY_ACCOUNT="$(printf '%s\n' "$KEY_META" | sed -n '1p')"
KEY_PUBLIC="$(printf '%s\n' "$KEY_META" | sed -n '2p')"
BUYER="${PACTARA_BUYER:-$KEY_ACCOUNT}"

if [[ "$BUYER" != "$KEY_ACCOUNT" ]]; then
  echo "Restricted key account_id ($KEY_ACCOUNT) does not match PACTARA_BUYER ($BUYER)." >&2
  exit 1
fi

echo "Contract : $CONTRACT"
echo "Buyer    : $BUYER"
echo "Worker   : $WORKER"
echo "Key file : $RESTRICTED_KEY"
echo "Key pub  : $KEY_PUBLIC"

echo "=== Confirm restricted key via NEAR RPC (permission object, not text scrape) ==="
"$PYTHON" "$ROOT/pactara_rpc.py" check-buyer-key "$BUYER" "$KEY_PUBLIC" "$CONTRACT"

echo "=== Preflight: contract views ==="
VERSION_OUT="$(near contract call-function as-read-only "$CONTRACT" get_code_version json-args '{}' network-config "$NETWORK" now)" || true
if ! printf '%s\n' "$VERSION_OUT" | grep -Fq 'pactara-2.2.0'; then
  echo "Contract $CONTRACT is not running pactara-2.2.0. Deploy this repo's wasm to a fresh account." >&2
  exit 1
fi
TREASURY_OUT="$(near contract call-function as-read-only "$CONTRACT" get_treasury json-args '{}' network-config "$NETWORK" now)" || {
  echo "Contract $CONTRACT is not accessible." >&2
  exit 1
}
printf '%s\n' "$TREASURY_OUT"

WORKER_OUT="$(near contract call-function as-read-only "$CONTRACT" is_authorized_worker json-args "{\"account\":\"$WORKER\"}" network-config "$NETWORK" now)" || true
if ! printf '%s\n' "$WORKER_OUT" | grep -Eq 'return value:[[:space:]]*true'; then
  echo "Worker $WORKER is not authorized on $CONTRACT." >&2
  exit 1
fi

BUYER_OUT="$(near contract call-function as-read-only "$CONTRACT" is_authorized_buyer json-args "{\"account\":\"$BUYER\"}" network-config "$NETWORK" now)" || true
if ! printf '%s\n' "$BUYER_OUT" | grep -Eq 'return value:[[:space:]]*true'; then
  echo "Buyer $BUYER is not authorized on $CONTRACT." >&2
  exit 1
fi

POLICY_OUT="$(near contract call-function as-read-only "$CONTRACT" get_policy json-args '{}' network-config "$NETWORK" now)" || {
  echo "get_policy is unavailable. Redeploy the current contract wasm." >&2
  exit 1
}
printf '%s\n' "$POLICY_OUT"

"$PYTHON" - "$AMOUNT" "$TREASURY_OUT" "$POLICY_OUT" <<'PY' || exit 1
import re, sys
amount = int(sys.argv[1])
treasury = sys.argv[2]
policy = sys.argv[3]

def nums(text):
    return [int(x) for x in re.findall(r'"?(\d{3,})"?', text)]

t = nums(treasury)
p = nums(policy)
if len(t) < 3:
    raise SystemExit("Could not parse treasury (total, available, reserved)")
available = t[1]
if len(p) < 3:
    raise SystemExit("Could not parse policy (max_per_task, daily_limit, daily_spent)")
max_task, daily_limit, daily_spent = p[0], p[1], p[2]
if amount > available:
    raise SystemExit(f"Insufficient available funds: need {amount}, have {available}")
if amount > max_task:
    raise SystemExit(f"Amount exceeds max_amount_per_task ({max_task})")
if daily_spent + amount > daily_limit:
    raise SystemExit(f"Amount exceeds remaining daily limit ({daily_limit - daily_spent})")
print("Preflight amounts ok")
PY

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
NOW_NS="$(( $(date +%s) * 1000000000 ))"
if [[ "$DEADLINE" -le "$NOW_NS" ]]; then
  echo "Deadline is not in the future." >&2
  exit 1
fi

echo "Deadline : $DEADLINE"
echo "=== Create task (restricted key, sign-as $BUYER) ==="
near contract call-function as-transaction "$CONTRACT" create_task \
  json-args "{\"worker\":\"$WORKER\",\"amount\":\"$AMOUNT\",\"criteria_hash\":$CRITERIA_HASH,\"input_hash\":$INPUT_HASH,\"verification_deadline\":$DEADLINE}" \
  prepaid-gas '50 Tgas' \
  attached-deposit '0 NEAR' \
  sign-as "$BUYER" \
  network-config "$NETWORK" \
  sign-with-access-key-file "$RESTRICTED_KEY" \
  send
