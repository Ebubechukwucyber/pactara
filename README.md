# Pactara

Policy-enforced TaskBond treasury + verifiable task settlement on NEAR.

## Live Demo (testnet)

- Contract: `pactara-v9.testnet`
- Settled task tx: https://explorer.testnet.near.org/transactions/62TnWPZ8bv2o2cXUR4cbwGKyLGSxCkfkogTJZDF6sGw8
- Task state: Settled
- Full flow proven: deposit → create_task → submit_artifact → register_verifier → submit_verification → automatic payout

## How it works

1. Buyer deposits NEAR into the contract treasury.
2. Owner creates a task with worker, amount, criteria/input hashes, and deadline.
3. Worker submits the artifact hash.
4. Off-chain verifier signs the exact message (protocol + network + account + task data).
5. Anyone submits the signature; contract verifies ed25519 and settles (pays worker) or rejects.
6. Expired tasks can be reclaimed via `expire_task`.

## Run the demo

```bash
# requires near-cli-rs and the verifier signature file at /tmp/verify.json
./demo.sh
```

## Build

```bash
cargo near build non-reproducible-wasm
```

## Key methods

- `deposit`, `withdraw`
- `create_task`, `submit_artifact`, `submit_verification`
- `register_verifier`, `expire_task`
- Views: `get_treasury`, `get_task_state`, `get_signing_message`
