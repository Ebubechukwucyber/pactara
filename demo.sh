#!/bin/bash
set -e
CONTRACT=pactara-v9.testnet
WORKER=pactara-v6.testnet

echo "=== 1. Deposit 1 NEAR ==="
near contract call-function as-transaction $CONTRACT deposit \
  json-args '{}' prepaid-gas '30.0 Tgas' attached-deposit '1 NEAR' \
  sign-as $CONTRACT network-config testnet sign-with-keychain send

echo "=== 2. Create task ==="
near contract call-function as-transaction $CONTRACT create_task \
  json-args '{"worker":"'"$WORKER"'","amount":"100000000000000000000000","criteria_hash":[17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17,17],"input_hash":[34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34,34],"deadline_seconds":3600}' \
  prepaid-gas '40.0 Tgas' attached-deposit '0 NEAR' \
  sign-as $CONTRACT network-config testnet sign-with-keychain send

echo "=== 3. Submit artifact ==="
near contract call-function as-transaction $CONTRACT submit_artifact \
  json-args '{"task_id":1,"artifact_hash":[51,51,51,51,51,51,51,51,51,51,51,51,51,51,51,51,51,51,51,51,51,51,51,51,51,51,51,51,51,51,51,51]}' \
  prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' \
  sign-as $WORKER network-config testnet sign-with-keychain send

echo "=== 4. Submit verification ==="
near contract call-function as-transaction $CONTRACT submit_verification \
  file-args /tmp/verify.json prepaid-gas '60.0 Tgas' attached-deposit '0 NEAR' \
  sign-as $CONTRACT network-config testnet sign-with-keychain send

echo "=== Final state ==="
near contract call-function as-read-only $CONTRACT get_task_state json-args '{"task_id":1}' network-config testnet now
near contract call-function as-read-only $CONTRACT get_treasury json-args '{}' network-config testnet now
