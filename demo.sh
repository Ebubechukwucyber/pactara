#!/bin/bash
set -e
CONTRACT="pactara-v6.testnet"
BUYER="$CONTRACT"
WORKER="$CONTRACT"

echo "=== Pactara Demo ==="
echo "Contract: $CONTRACT"

echo ""
echo "1. Treasury status"
near contract call-function as-read-only $CONTRACT get_treasury json-args '{}' network-config testnet now

echo ""
echo "2. Daily status"
near contract call-function as-read-only $CONTRACT get_daily_status json-args '{}' network-config testnet now

echo ""
echo "3. Create task (authorized buyer + worker)"
TASK_ID=$(near contract call-function as-transaction $CONTRACT create_task \
  json-args '{"worker":"'"$WORKER"'","amount":"100000000000000000000000","criteria_hash":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=","input_hash":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=","verification_deadline":"9999999999999999999"}' \
  prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as $BUYER network-config testnet sign-with-keychain send | grep -oP 'return value.*' || echo "1")
echo "Task created"

echo ""
echo "4. Attempt withdraw with restricted key (should fail if key is restricted)"
echo "(Skipped in this demo — use a function-call key without withdraw permission)"

echo ""
echo "5. Submit artifact"
near contract call-function as-transaction $CONTRACT submit_artifact \
  json-args '{"task_id":1,"artifact_hash":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}' \
  prepaid-gas '30 Tgas' attached-deposit '0 NEAR' sign-as $WORKER network-config testnet sign-with-keychain send

echo ""
echo "6. Verifier signs and submits (see verifier.py)"
echo "Run: python3 verifier.py <sk> $CONTRACT 1 criteria.json artifact.json"

echo ""
echo "=== Demo complete ==="
