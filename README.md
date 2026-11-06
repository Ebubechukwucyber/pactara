# Pactara

**Independent, authorized verification with cryptographically enforced settlement on NEAR.**

Pactara is a NEAR contract that lets a buyer reserve funds for a task, a worker submit an artifact, and an authorized verifier sign a structured result. The contract checks that signature on-chain and then either pays the worker or returns the reserved funds.

This is not a claim of trustless work evaluation. The contract enforces settlement rules around a registered verifier key. It does not decide whether the work is objectively correct.

## What the contract guarantees

- Deposited funds are tracked as `total`, `available`, and `reserved`.
- Creating a task moves the payout amount from `available` to `reserved` and enforces per-task and daily spending limits. FAIL verification, failed payout, and expiry restore `daily_spent` if they happen on the same UTC day the task was created.
- Only the owner or an authorized buyer may `deposit`. Withdrawals always pay the owner.
- Accounting identity: `total == available + reserved + pending_out` (so `total >= available + reserved`). Callbacks log and clamp rather than panic if an underflow is observed, so a completed transfer receipt can still be recorded.
- Settlement requires a valid ed25519 signature from the registered verifier over a domain-separated message:
  `pactara-v1 || network_id || contract_account || task_id_le || criteria_hash || input_hash || artifact_hash || pass_flag`
  `network_id` is stored at init (`new` uses `testnet`; `new_with_network` for other networks).
- Criteria, input, and artifact hashes in that message must match the hashes stored on the task.
- A PASS signature moves the task `Funded → Submitted → PayoutPending`. Only a successful payout callback marks it `Settled` and reduces `total`.
- A FAIL signature, a failed payout, or an expired task returns the reserved amount to `available`. Failed verification and failed payout become `Refunded`. Timeout becomes `Expired`.
- Withdrawals deduct from `available` only after tracking a pending withdrawal. A failed transfer restores the amount.
- Replay is blocked by task state plus a one-time nonce. Callbacks may be executed only by the contract account (`#[private]` plus an explicit predecessor check).
- Anyone can submit a verification signature or expire a timed-out task. Only an authorized buyer can create a task. Only the named worker can submit the artifact.

## What the verifier is trusted to do

The verifier is trusted to evaluate the artifact against the agreed criteria and to sign `passed=true` or `passed=false` honestly.

A signature proves that the registered key signed that result. It does not prove that the evaluation was correct, complete, or free of bias. A compromised or malicious verifier can still authorize an incorrect payout or refund.

## What happens when verification passes or fails

1. Buyer deposits NEAR.
2. Authorized buyer creates a task with worker, amount, criteria hash, **input hash of the actual task input**, and an absolute `verification_deadline` (block timestamp nanoseconds). Funds are reserved.
3. Worker submits the artifact hash.
4. The verifier loads criteria, input, and artifact, queries the on-chain task, checks hashes and its own public key, then signs the contract’s canonical message.
5. PASS: task becomes `PayoutPending`, reserved funds are marked in-flight, and a transfer is started. The payout callback sets `Settled` on success or `Refunded` on failure (restoring `available`).
6. FAIL: reserved funds return to `available`, task becomes `Refunded`.
7. After the deadline, anyone can call `expire_task` for `Funded` or `Submitted` tasks.

## Buyer access keys

The buyer agent (`buyer_agent.sh`) must sign as the account that owns the restricted key.

```bash
export PACTARA_CONTRACT=pactara-v9.testnet
export PACTARA_BUYER=<buyer-account.testnet>
export PACTARA_WORKER=pactara-v6.testnet
export PACTARA_RESTRICTED_KEY=/path/to/buyer-function-call-key.json
./buyer_agent.sh
```

The script reads `account_id` and `public_key` from the key file (never prints the private key), checks that `PACTARA_BUYER` matches, and checks that the public key is installed on that account. It refuses FullAccess keys.

Inspect permissions:

```bash
near account list-keys "$PACTARA_BUYER" network-config testnet now
```

The buyer key should be a function-call key limited to `create_task` on the contract. It must not allow `withdraw`, `set_policy`, `register_verifier`, or allowlist changes. The contract still enforces owner-only on those methods even if a key is mis-issued.

## Supported JSON validation (not JSON Schema)

`verifier.py` is a small evaluator, not a JSON Schema implementation.

Allowed criteria shape:

```json
{"schema":{"field":{"type":"string|integer|number|boolean|object","required":true}}}
```

Allowed rule keywords: `type`, `required`, `minLength`, `maxLength`, `min`, `max`, `properties`, `required_keys`. Any other keyword (`pattern`, `$ref`, `enum`, `items`, …) or type (`array`, …) is rejected **before signing**. A schema error is not a FAIL signature.

Python `True` is not accepted as an integer. Canonical hashing is `json.dumps(..., sort_keys=True, separators=(",", ":"), ensure_ascii=True)` then SHA-256.

## Canonical hashes

Buyer, worker, and verifier must hash JSON the same way:

```text
json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=True)
SHA-256 of those UTF-8 bytes
```

- `criteria_hash` is the hash of the criteria document (schema).
- `input_hash` is the hash of the task input document, not of an `input` field nested inside the criteria.
- `artifact_hash` is the hash of the submitted artifact document.

## Live on testnet

| Item | Value |
|------|-------|
| Contract account | `pactara-v9.testnet` |
| Worker used by the demo | `pactara-v6.testnet` |
| Example settled task | [62TnWPZ8bv2o2cXUR4cbwGKyLGSxCkfkogTJZDF6sGw8](https://explorer.testnet.near.org/transactions/62TnWPZ8bv2o2cXUR4cbwGKyLGSxCkfkogTJZDF6sGw8) |

The account `pactara-v9.testnet` is the currently documented deployment. After this settlement-callback change, redeploy the wasm from this repository before expecting `PayoutPending` on that account. Set `PACTARA_CONTRACT` if you deploy to a new account.

## Implemented vs planned

Implemented:

- Policy-limited treasury (available / reserved / pending outbound)
- Worker and buyer allowlists
- Hash-bound tasks and domain-separated ed25519 verification
- Payout and withdrawal callbacks with balance recovery
- Off-chain verifier that queries the task before signing
- End-to-end testnet demo scripts

Not implemented (do not assume they exist):

- TEE-backed or hardware-attested verification
- Multi-verifier or threshold consensus
- Objective verification of arbitrary work
- Dispute windows, human arbitration, or task retry after refund
- In-place network switching after init
- A marketplace, dashboard, or additional tokens

## Limitations

These are remaining properties of the current design, not planned features.

### Trust

- One registered ed25519 key decides PASS or FAIL. The contract does not evaluate work, run a TEE, or require a verifier quorum.
- A signature proves only that the registered key signed that task, those hashes, and that boolean. It does not prove the work is correct.
- The owner can replace the verifier key, change spending policy, and add or remove buyers and workers. There is no on-chain timelock or governance.
- Criteria, input, and artifact JSON never appear on-chain. Only 32-byte hashes do. Parties must exchange the real documents off-chain. If the verifier is shown different bytes than the buyer hashed, the on-chain hash check fails; if the verifier is malicious, it can still sign PASS or FAIL for the stored hashes without looking at the documents.
- `submit_verification` is permissionless. Anyone who obtains a valid signature can submit it. That is intentional, but it means the verifier’s signed result is a bearer credential until the task nonce is consumed.
- The Python verifier trusts the configured NEAR RPC when it calls `get_task` and `get_signing_message`.

### Accounting and funds

- Reserved funds cannot be withdrawn. Owner `withdraw` only spends `available`.
- A failed payout marks the task `Refunded` and does not reopen it. The worker cannot resubmit on that task id.
- `expire_task` works only in `Funded` or `Submitted`. A task in `PayoutPending` cannot be expired, and the transfer is never retried (that would risk a double pay). Callbacks use 50 Tgas and avoid panicking after the predecessor check so accounting can finish.
- Withdrawals always pay the owner, not the original depositor.
- Treasury maps are not iterable. The contract cannot list all tasks, pending payouts, or pending withdrawals.

### Cryptography and environment

- Protocol id is `pactara-v1`. Together with `network_id` and the contract account it domain-separates signatures; it is not a chain-id from the host.
- Only ed25519 verifier keys are accepted. secp256k1 is rejected.
- Hashing is SHA-256 over a specific canonical JSON encoding. A different serializer produces a different task and will not verify.

### Operations

- This state layout (extra enum variant, `pending_withdrawals`, `pending_out`) is not a safe in-place upgrade of an already-initialized contract. Migrating `pactara-v9.testnet` requires a fresh account or an intentional wipe and re-init.
- `get_code_version` currently returns `pactara-2.0.0`. `demo.sh` and `buyer_agent.sh` abort if that view is missing or different, so an old testnet account cannot be mistaken for this source.
- `pactara-v9.testnet` was verified as older bytecode (`get_code_version` / `get_policy` absent). Do not in-place upgrade it; use a fresh account.
- Unit tests mock receipts; they do not prove live NEAR transfers.
- Restricted buyer access keys are a NEAR account-key convention. The contract still authorizes `create_task` via the buyer allowlist; key permissions only limit what that account can sign. A FullAccess buyer key can call any method the account is allowed to call.
- TEE attestation, multi-verifier consensus, and on-chain document storage are not implemented and are not planned in this prototype.

## Build and test

```bash
rustup target add wasm32-unknown-unknown
cargo test
cargo build --target wasm32-unknown-unknown --release
python3 verifier.py self-test
```

Deploy (replace the account if you are not using v9):

```bash
near contract deploy pactara-v9.testnet \
  use-file target/wasm32-unknown-unknown/release/pactara_contract.wasm \
  without-init-call \
  network-config testnet \
  sign-with-keychain \
  send
```

A state-layout change (new enum variant and withdrawal fields) is not a safe in-place upgrade of an already-initialized contract. Use a fresh account or re-initialize only if you intend to wipe existing tasks.

## Demos

Requires `near-cli-rs`, testnet keys in the keychain, and `pip install pynacl base58`.

Successful payment:

```bash
chmod +x demo.sh
./demo.sh pass
```

Failed verification / refund:

```bash
./demo.sh fail
```

```bash
./demo.sh deny
```

`pass` and `fail` generate hashes and task ids at runtime, then check the on-chain task state (`Settled` or `Refunded`). They exit non-zero if the chain state does not match. `deny` shows live rejections for an unauthorized worker, an oversized amount, and worker calls to `set_policy` / `withdraw`.

The demo and source are not proof that `pactara-v9.testnet` is running this wasm. Redeploy before treating testnet results as current.

## Contract methods

| Method | Description |
|--------|-------------|
| `deposit` / `withdraw` | Treasury in/out. Withdrawals use `on_withdraw` recovery |
| `create_task` | Reserve funds and open a task |
| `submit_artifact` | Worker posts artifact hash |
| `register_verifier` | Set the ed25519 public key |
| `submit_verification` | Signed result → `PayoutPending` or `Refunded` |
| `on_payout` / `on_withdraw` | Private callbacks |
| `expire_task` | Restore reserved funds after the deadline |
| Views | `get_treasury`, `get_pending_out`, `get_policy`, `get_task`, `get_task_state`, `get_signing_message`, `get_verifier_public_key`, `get_network_id`, `get_protocol_id`, `get_code_version` |

Built for the NEAR Hackathon 2026.
