# Pactara

**Verifiable Task Bonds on NEAR — escrow that only pays when work is cryptographically proven.**

Pactara is a policy-enforced treasury + task settlement contract. Buyers lock funds. Workers deliver. An authorized verifier signs a structured proof. The contract checks the signature on-chain and automatically releases payment — or returns the funds if the work fails or expires.

## Why it matters

Most escrow systems still rely on human arbitration or simple timeouts. Pactara removes the trust layer:
- Funds are reserved at task creation
- Release requires a valid ed25519 signature over the exact task data
- No admin can force a payout without the verifier’s key
- Expired tasks can be reclaimed by anyone

## Live on Testnet

| Item | Value |
|------|-------|
| Contract | `pactara-v9.testnet` |
| Settled task | [62TnWPZ8bv2o2cXUR4cbwGKyLGSxCkfkogTJZDF6sGw8](https://explorer.testnet.near.org/transactions/62TnWPZ8bv2o2cXUR4cbwGKyLGSxCkfkogTJZDF6sGw8) |
| Final state | Settled |
| Flow proven | deposit → create_task → submit_artifact → register_verifier → submit_verification → automatic payout |

## How it works

1. **Deposit** — Buyer sends NEAR to the contract treasury.
2. **Create task** — Owner defines worker, amount, criteria hash, input hash, and deadline. Funds are reserved.
3. **Submit artifact** — Worker posts the hash of their deliverable.
4. **Verify** — Off-chain verifier signs the canonical message (`protocol + network + account + task data`).
5. **Settle** — Anyone can submit the signature. Contract verifies it, pays the worker, and marks the task Settled.
6. **Expire** — After the deadline, anyone can call `expire_task` to return reserved funds.

## Quick Demo

```bash
# 1. Clone and build
git clone https://github.com/Ebubechukwucyber/pactara.git
cd pactara
cargo near build non-reproducible-wasm

# 2. Run the full flow (requires near-cli-rs + verifier signature file)
./demo.sh
```

## Contract Methods

| Method | Who | Description |
|--------|-----|-------------|
| `deposit` | Buyer | Add funds to treasury |
| `withdraw` | Owner | Remove unreserved funds |
| `create_task` | Owner | Reserve funds and open a task |
| `submit_artifact` | Worker | Attach deliverable hash |
| `register_verifier` | Owner | Set the ed25519 public key that can authorize payouts |
| `submit_verification` | Anyone | Submit signed proof → settle or reject |
| `expire_task` | Anyone | Reclaim funds after deadline |
| `get_treasury` | View | `(total, available, reserved)` |
| `get_task_state` | View | Current state of a task |
| `get_signing_message` | View | Exact bytes the verifier must sign |

## Security Model

- Only the registered verifier public key can authorize settlement
- Message is domain-separated (`pactara-v1` + network + contract account)
- Nonce prevents replay
- Reserved balance accounting prevents over-commitment
- Failed payouts automatically refund the treasury

## Build & Deploy

```bash
cargo near build non-reproducible-wasm
near contract deploy pactara-v9.testnet use-file target/near/pactara_contract.wasm \
  with-init-call new json-args '{"owner":"YOUR_ACCOUNT.testnet","max_amount_per_task":"1000000000000000000000000"}' \
  prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' \
  network-config testnet sign-with-keychain send
```

## Next Steps

- Frontend for task creation and verification submission
- Multi-verifier threshold
- Integration with NEAR’s Chain Signatures for off-chain work proofs

Built for the NEAR Hackathon.
