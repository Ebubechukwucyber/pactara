# Pactara

Independent, authorized verification with cryptographically enforced settlement on NEAR.

**AI agents can hire other agents, reserve funds under enforceable spending policies, and release payment only after a registered verifier signs the result.**

A valid signature proves the registered verifier signed that result. It does not prove the work is objectively correct.

## Problem

Agent-to-agent work needs escrow that cannot be overspent, and payout that cannot happen without a bound verification result. Human arbitration does not scale. Pactara is a small NEAR contract for that job, not a marketplace.

## How it works

```mermaid
flowchart LR
  Human[Human / treasury owner] --> Buyer[Buyer agent\nfunction-call key]
  Buyer --> Contract[Pactara contract]
  Contract --> Worker[Worker agent]
  Worker --> Verifier[Independent verifier]
  Verifier --> Contract
  Contract --> Pay[Settled payout or refund]
```

1. Owner deposits NEAR. Only the owner or an authorized buyer may deposit. Withdrawals pay the owner.
2. An authorized buyer calls `create_task` (optionally with a function-call key limited to that method). The contract checks worker allowlist, per-task cap, daily cap, available funds, and deadline, then moves the amount `available → reserved`.
3. The named worker submits an artifact hash.
4. The verifier hashes canonical JSON, queries the on-chain task, checks its key and the contract signing message, and signs PASS or FAIL.
5. Anyone may submit that signature. PASS → `PayoutPending` → callback `Settled` (worker paid) or `Refunded` (transfer failed, funds restored). FAIL or expiry returns reserved funds to `available`.

The contract is the authority. Scripts only fail early.

## Treasury

`total == available + reserved + pending_out`

| Event | Effect |
|-------|--------|
| Deposit | `total` and `available` increase |
| Create task | `available → reserved`, `daily_spent` increases |
| PASS | `reserved → pending_out` |
| Payout success | `pending_out` and `total` decrease, state `Settled` |
| Payout/FAIL/expiry | amount returns to `available`; same-day `daily_spent` restored |
| Withdraw | `available → pending_out`; success drops `total`; failure restores `available` |

`get_health` returns `(healthy, total, available, reserved, pending_out, fault_reason)`. Callbacks never panic after the predecessor check (a panic would strand a completed transfer). Arithmetic that cannot be applied is skipped rather than clamped; the first fault reason is stored, `accounting_fault` is set, and later deposits, withdrawals, task creation, verification, and expiry are frozen.

## Verification

Signing message:

`pactara-v1 || network_id || contract_account || task_id_le || criteria_hash || input_hash || artifact_hash || pass_flag`

Canonical JSON (buyer, worker, verifier):

`json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=True)` then SHA-256.

Evaluator subset (**not** JSON Schema): `schema` map of fields with `type` in `string|integer|number|boolean|object`, plus `required`, `minLength`, `maxLength`, `min`, `max`, `properties`, `required_keys`. Any other keyword is rejected before signing.

## Buyer restricted key

The contract sees the **buyer account**, not which access key signed. Policy is enforced on-chain regardless of keys.

Off-chain, `buyer_agent.sh` uses NEAR RPC `view_access_key_list` to require:

- key `account_id` equals `PACTARA_BUYER`
- public key installed on that account
- `FunctionCall` (not FullAccess)
- `receiver_id` equals the contract
- methods are exactly `create_task` (empty method list is rejected)

```bash
export PACTARA_CONTRACT=...
export PACTARA_BUYER=...
export PACTARA_WORKER=...
export PACTARA_RESTRICTED_KEY=/path/to/create_task_only.json
./buyer_agent.sh
```

Inspect: `near account list-keys "$PACTARA_BUYER" network-config testnet now`

## Demo

Requires `near-cli-rs`, `pip install -r requirements.txt`, and a deployment whose `get_code_version` is **`pactara-2.2.0`**.

```bash
export PACTARA_CONTRACT=pactara-v10.testnet   # or another fresh account; do not in-place upgrade pactara-v9
export PACTARA_SIGNER=$PACTARA_CONTRACT
export PACTARA_WORKER=<worker.testnet>
./demo.sh pass    # Scenario A: Settled
./demo.sh fail    # Scenario B: Refunded
./demo.sh deny    # Scenario C: on-chain rejections
```

Scripts abort if the on-chain state is not `Settled` / `Refunded`. They never print success first.

Payout/withdraw **callback recovery** is covered by Rust unit tests (mocked receipts). A live failed NEAR transfer is not always reproducible on testnet.

## Tests and build

```bash
cargo fmt
cargo test --lib
cargo build --target wasm32-unknown-unknown --release
python3 verifier.py self-test
python3 test_verifier.py
python3 test_pactara_rpc.py
python3 test_integration.py
bash -n demo.sh buyer_agent.sh
```

## Deployment

`get_code_version` = `pactara-2.2.0` on **`pactara-v10.testnet`**. `pactara-v9.testnet` is older bytecode. Do not in-place upgrade old accounts (state layout changed).

```bash
cargo build --target wasm32-unknown-unknown --release
near contract deploy <NEW_ACCOUNT> \
  use-file target/wasm32-unknown-unknown/release/pactara_contract.wasm \
  with-init-call new \
  json-args '{"owner":"<NEW_ACCOUNT>","max_amount_per_task":"1000000000000000000000000","daily_limit":"5000000000000000000000000"}' \
  prepaid-gas '100 Tgas' \
  network-config testnet sign-with-keychain send
```

## What is implemented vs not

| Implemented | Not implemented |
|-------------|-----------------|
| Allowlists, spending limits, hash-bound tasks | TEE / hardware attestation |
| ed25519 verifier + domain-separated message | Multi-verifier / threshold |
| Payout and withdraw callbacks | Objective work evaluation |
| Restricted-key buyer script | Marketplace, extra tokens, arbitration UI |

## Evidence

| Kind | Status |
|------|--------|
| Rust unit tests | Local: 40 passed |
| Python verifier / RPC / integration tests | Local: 16 + 7 + 1 passed |
| WASM `pactara-2.2.0` | Built; SHA-256 `8c4836e27e1e1abe381f728c0680daaa443347fa3a2a08355e992072db252233` |
| Testnet `pactara-v10.testnet` | Deployed and initialized (`4bMeQ7ExY1hWGZaqN24kSu273WGVbQR7TyMcEC6zztU6`) |
| Live PASS | Task 4 `Settled`; payout [3u4s3X9HGevoiXgB9EYCwRXzBcZi4RdnvvNCu1RgWHtm](https://explorer.testnet.near.org/transactions/3u4s3X9HGevoiXgB9EYCwRXzBcZi4RdnvvNCu1RgWHtm) |
| Live FAIL | Task 5 `Refunded`; [Bf7CMJVfUzorchVog81jCEu8d8JG2YhsywV6wqWFjzU4](https://explorer.testnet.near.org/transactions/Bf7CMJVfUzorchVog81jCEu8d8JG2YhsywV6wqWFjzU4) |
| Live deny | Unauthorized worker / over-limit / worker `set_policy` / worker `withdraw` rejected |
| Failed-transfer recovery | Local unit tests; not forced on testnet |

## Limitations

- One owner-chosen verifier key.
- Documents are off-chain; only hashes are stored.
- Signed PASS is a bearer credential until the task nonce is used.
- `PayoutPending` is not expired or retried (retrying the transfer could double-pay).
- Restricted keys are a NEAR account feature; the contract authorizes accounts, not key ids.
- Not a production-audited system.
