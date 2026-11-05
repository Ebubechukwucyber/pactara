#!/bin/bash
set -e
CONTRACT="pactara-v6.testnet"
ACCOUNT="pactara-v6.testnet"

echo "=== Pactara Demo ==="
echo "Contract: $CONTRACT"
echo

echo "1. Treasury status"
near contract call-function as-read-only "$CONTRACT" get_treasury json-args '{}' network-config testnet now
echo

echo "2. Daily status"
near contract call-function as-read-only "$CONTRACT" get_daily_status json-args '{}' network-config testnet now
echo

# Create criteria and artifact files
mkdir -p /tmp/pactara-demo
cat > /tmp/pactara-demo/criteria.json << 'CRITERIA'
{
  "schema": {
    "result": {"type": "string", "required": true, "minLength": 1},
    "score": {"type": "number", "required": true, "min": 0, "max": 100}
  },
  "input": {"query": "demo"}
}
CRITERIA

cat > /tmp/pactara-demo/artifact.json << 'ARTIFACT'
{
  "result": "completed",
  "score": 95
}
ARTIFACT

# Compute hashes as byte arrays
CRITERIA_HASH=$(python3 -c "
import json, hashlib
c = json.load(open('/tmp/pactara-demo/criteria.json'))
h = hashlib.sha256(json.dumps(c, sort_keys=True).encode()).hexdigest()
print(list(bytes.fromhex(h)))
")

INPUT_HASH=$(python3 -c "
import json, hashlib
c = json.load(open('/tmp/pactara-demo/criteria.json'))
h = hashlib.sha256(json.dumps(c.get('input', {}), sort_keys=True).encode()).hexdigest()
print(list(bytes.fromhex(h)))
")

echo "3. Create task"
near contract call-function as-transaction "$CONTRACT" create_task \
  json-args "{\"worker\":\"$ACCOUNT\",\"amount\":\"100000000000000000000000\",\"criteria_hash\":$CRITERIA_HASH,\"input_hash\":$INPUT_HASH,\"verification_deadline\":2000000000000000000}" \
  prepaid-gas "50.0 Tgas" attached-deposit "0 NEAR" \
  sign-as "$ACCOUNT" network-config testnet sign-with-keychain send

echo
echo "4. Submit artifact (as worker)"
ARTIFACT_HASH=$(python3 -c "
import json, hashlib
a = json.load(open('/tmp/pactara-demo/artifact.json'))
h = hashlib.sha256(json.dumps(a, sort_keys=True).encode()).hexdigest()
print(list(bytes.fromhex(h)))
")

near contract call-function as-transaction "$CONTRACT" submit_artifact \
  json-args "{\"task_id\":1,\"artifact_hash\":$ARTIFACT_HASH}" \
  prepaid-gas "30.0 Tgas" attached-deposit "0 NEAR" \
  sign-as "$ACCOUNT" network-config testnet sign-with-keychain send

echo
echo "5. Run verifier"
echo "python3 verifier.py <PRIVATE_KEY> $CONTRACT 1 /tmp/pactara-demo/criteria.json /tmp/pactara-demo/artifact.json $ACCOUNT"
echo
echo "Done. Run the verifier command above with your private key."
