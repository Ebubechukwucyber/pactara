# Pactara

**Verifiable Task Bonds for AI Agents on NEAR**

Pactara lets AI agents and humans lock funds, define work, and get paid only when an authorized verifier cryptographically proves the work was done correctly. No trusted intermediary. No human arbitration.

## Problem

AI agents are starting to hire other agents and humans to complete tasks. Today there is no clean way to:
- Escrow payment
- Define acceptance criteria on-chain
- Automatically release funds when work is proven
- Recover funds when work fails or times out

Existing escrow contracts still require a human admin or simple timeout. Pactara removes that trust.

## Solution

A single NEAR contract that combines:
- Policy-enforced treasury (available vs reserved balances)
- Task state machine (Funded → Submitted → Settled / Expired / Rejected)
- On-chain ed25519 verification of a structured work proof
- Automatic payout or refund

The verifier can be an AI system, a human, or a threshold of both. The contract only cares about the signature.

## Live Demo (Testnet)

| | |
|--|--|
| Contract | `pactara-v9.testnet` |
| Settled task | [62TnWPZ8bv2o2cXUR4cbwGKyLGSxCkfkogTJZDF6sGw8](https://explorer.testnet.near.org/transactions/62TnWPZ8bv2o2cXUR4cbwGKyLGSxCkfkogTJZDF6sGw8) |
| Result | Task settled, worker paid, reserved balance released |

Full flow executed on-chain:  
deposit → create_task → submit_artifact → register_verifier → submit_verification → automatic payout

## How It Works

1. Buyer deposits NEAR into the contract.
2. Owner creates a task with worker, amount, criteria hash, input hash, and deadline. Funds are reserved.
3. Worker submits the artifact hash.
4. Verifier (AI or human) signs the exact message: `pactara-v1 || network || contract || task data`.
5. Anyone submits the signature. Contract verifies it and either pays the worker or rejects.
6. After deadline, anyone can call `expire_task` to return reserved funds.

## Why This Fits the NEAR Hackathon

- Uses NEAR’s native account model and Promise-based payments
- Demonstrates real economic activity (escrow + conditional transfer)
- Designed for the emerging agent-to-agent economy
- Fully working on testnet with verifiable transactions
- Minimal but complete: treasury accounting + cryptographic settlement

## Contract Interface

- `deposit` / `withdraw` — treasury management
- `create_task` — reserve funds and open task
- `submit_artifact` — worker delivers hash
- `register_verifier` — set ed25519 public key
- `submit_verification` — submit signed proof → settle or reject
- `expire_task` — reclaim after deadline
- Views: `get_treasury`, `get_task_state`, `get_signing_message`

## Run the Demo

```bash
git clone https://github.com/Ebubechukwucyber/pactara.git
cd pactara
./demo.sh
```

Requires `near-cli-rs` and the verifier signature file at `/tmp/verify.json`.

## Security Properties

- Domain-separated message prevents cross-contract replay
- Nonce prevents double-spend of a verification
- Reserved balance accounting prevents over-commitment
- Only the registered verifier key can authorize payout
- Failed payouts automatically refund the treasury

Built for the NEAR Hackathon 2026.
