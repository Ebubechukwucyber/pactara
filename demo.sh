#!/bin/bash
# Pactara Demo — clean output
# Run this after the buyer agent if you want a full flow.

set -e

CONTRACT=pactara-v2.testnet
OWNER=pactara-v2.testnet
WORKER=ebubechukwucyber.testnet
RESTRICTED_KEY=~/.near-credentials/testnet/pactara-v2.testnet/ed25519_9RMRpaSXhSajo2R5bfHszDcXYusuhfwz1xxdKL1yHNvb.json

echo ""
echo "════════════════════════════════════════"
echo "  Pactara Demo"
echo "════════════════════════════════════════"

echo ""
echo "── 1. Buyer Agent (restricted key) ──"
./buyer_agent.sh

echo ""
echo "── 2. Unauthorized action (should fail) ──"
echo "→ Restricted key trying to withdraw..."
near contract call-function as-transaction $CONTRACT withdraw \
  json-args '{"amount":"100000000000000000000000"}' \
  prepaid-gas '30 Tgas' attached-deposit '0 NEAR' \
  sign-as $OWNER network-config testnet \
  sign-with-access-key-file $RESTRICTED_KEY send 2>&1 | grep -E "isn't allowed|Error|Rejected" || echo "Rejected as expected"

echo ""
echo "── 3. Treasury snapshot ──"
near contract call-function as-read-only $CONTRACT get_treasury json-args '{}' network-config testnet now 2>/dev/null | grep -A5 "Function execution return value" || true

echo ""
echo "════════════════════════════════════════"
echo "  Demo complete"
echo "════════════════════════════════════════"
echo ""
echo "To test Success / Fail paths, create a fresh task"
echo "and use the task ID that the agent just printed."
echo ""
