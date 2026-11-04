#!/bin/bash
# Pactara Buyer Agent
# Uses only the restricted key to hire a worker.

set -e

CONTRACT=pactara-v6.testnet
RESTRICTED_KEY=~/.near-credentials/testnet/pactara-v6.testnet/ed25519_PLACEHOLDER.json
WORKER=pactara-v6.testnet

echo ""
echo "────────────────────────────────────────"
echo "  Pactara Buyer Agent"
echo "────────────────────────────────────────"
echo "  Contract : $CONTRACT"
echo "  Key      : restricted (create_task only)"
echo ""

echo "→ Reading treasury..."
near contract call-function as-read-only $CONTRACT get_treasury json-args '{}' network-config testnet now 2>/dev/null | grep -A3 "Function execution return value" || true

echo ""
echo "→ Creating task (0.1 NEAR) with restricted key..."

near contract call-function as-transaction $CONTRACT create_task \
  json-args '{"worker":"'"$WORKER"'","amount":"100000000000000000000000","criteria_hash":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],"input_hash":[2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2],"verification_deadline":2000000000000000000}' \
  prepaid-gas '50 Tgas' \
  attached-deposit '0 NEAR' \
  sign-as $CONTRACT \
  network-config testnet \
  sign-with-access-key-file $RESTRICTED_KEY \
  send 2>&1 | grep -E "Task [0-9]+ created|Function execution return value|Error" || true

echo ""
echo "✓ Buyer agent finished."
echo "────────────────────────────────────────"
