#!/bin/bash
set -e
CONTRACT="pactara-v6.testnet"
BUYER="$CONTRACT"
WORKER="$CONTRACT"
SIGNER="$CONTRACT"

echo "=== Pactara Demo ==="
echo "Contract: $CONTRACT"

echo ""
echo "1. Treasury status"
near contract call-function as-read-only $CONTRACT get_treasury json-args '{}' network-config testnet now

echo ""
echo "2. Daily status"
near contract call-function as-read-only $CONTRACT get_daily_status json-args '{}' network-config testnet now

# Create criteria and artifact files
cat > /tmp/criteria.json << 'EOF'
{
  "schema": {
    "result": {"type": "string", "required": true, "minLength": 1},
    "score": {"type": "number", "required": true, "min": 0, "max": 100}
  },
  "input": {"query": "demo"}
}
EOF

cat > /tmp/artifact.json << 'EOF'
{
  "result": "completed",
  "score": 95
}
EOF

CRITERIA_HASH=$(python3 -c "import json,hashlib; print(hashlib.sha256(json.dumps(json.load(open('/tmp/criteria.json')),sort_keys=True,separators=(',',':')).encode()).hexdigest())")
INPUT_HASH=$(python3 -c "import json,hashlib; d=json.load(open('/tmp/criteria.json')); print(hashlib.sha256(json.dumps(d.get('input',{}),sort_keys=True,separators=(',',':')).encode()).hexdigest())")
ARTIFACT_HASH=$(python3 -c "import json,hashlib; print(hashlib.sha256(json.dumps(json.load(open('/tmp/artifact.json')),sort_keys=True,separators=(',',':')).encode()).hexdigest())")

echo ""
echo "3. Create task"
near contract call-function as-transaction $CONTRACT create_task \
  json-args '{"worker":"'"$WORKER"'","amount":"100000000000000000000000","criteria_hash":"'"$CRITERIA_HASH"'","input_hash":"'"$INPUT_HASH"'","verification_deadline":"9999999999999999999"}' \
  prepaid-gas '50 Tgas' attached-deposit '0 NEAR' sign-as $BUYER network-config testnet sign-with-keychain send

echo ""
echo "4. Submit artifact"
near contract call-function as-transaction $CONTRACT submit_artifact \
  json-args '{"task_id":1,"artifact_hash":"'"$ARTIFACT_HASH"'"}' \
  prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as $WORKER network-config testnet sign-with-keychain send

echo ""
echo "5. Verifier signs and submits"
echo "Note: Run verifier.py with the verifier private key to complete settlement"
echo "python3 verifier.py <sk_hex> $CONTRACT 1 /tmp/criteria.json /tmp/artifact.json $SIGNER"

echo ""
echo "=== Demo complete ==="
