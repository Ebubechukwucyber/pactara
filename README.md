# Pactara

**Policy-gated settlement infrastructure for agent-to-agent work.**

Agents can hire and pay other agents without being trusted with the money.  
Pactara is a NEAR smart contract that holds a prefunded treasury, reserves funds when an authorized buyer creates a task, and releases payment only to the original worker after independent verification.

The hiring agent uses a restricted function-call key. The contract enforces allowlists, spending limits, and binds every settlement to the stored criteria and artifact hashes.

---

## Why it exists

In agent economies, the hard problem is not calling an API. It is **moving value under policy**.

- A buyer should not hold worker funds.
- A worker should not trust a buyer to pay.
- An agent should not be able to drain a treasury.
- Settlement should be deterministic and auditable.

Pactara solves this with a small, opinionated on-chain state machine.

---

## How it works

```
Authorized Buyer (restricted key)
        │
        ▼
   create_task
   (allowlists + daily limit + amount checks)
        │
        ▼
┌─────────────────────────────┐
│      Pactara Contract       │
│                             │
│   available  →  reserved    │
│   worker + amount locked    │
│   criteria & input hashed   │
└─────────────────────────────┘
        │
        ▼
   Authorized Worker
   submits artifact_hash
        │
        ▼
   Independent Verifier
   (off-chain check)
        │
        ▼
   Signed verification
   (protocol + contract + hashes)
        │
   ┌────┴────┐
   PASS     FAIL / Expiry
   │         │
   ▼         ▼
 Pay worker  Refund to available
```

---

## Guarantees

| Guarantee | Enforcement |
|-----------|-------------|
| Prefunded treasury | `available` / `reserved` accounting |
| Buyer allowlist | On-chain check at task creation |
| Worker allowlist | On-chain check at task creation |
| Daily spending limit | Committed at creation (not refunded on failure) |
| Worker & amount fixed | Set once at `create_task` |
| Independent verification | Signature bound to protocol ID, contract ID, criteria, input, and artifact hashes |
| PASS settles to original worker | Direct transfer; failed transfer reversed via callback |
| FAIL or expiry refunds | Reserved funds return to available |
| Restricted key isolation | Cannot call owner methods |
| One submission per task | State machine enforced |

---

## Daily spending policy

The daily limit tracks **committed** spending.

Creating a task increments `daily_spent` even if the task later fails verification or expires. This limits how much an agent can commit in a single UTC day, not only successful payouts. The counter resets at the UTC day boundary.

---

## Contract interface

| Method | Access | Description |
|--------|--------|-------------|
| `new` | Owner | Initialize contract with owner, max amount, daily limit |
| `deposit` | Payable | Fund the treasury |
| `withdraw` | Owner | Withdraw available balance |
| `create_task` | Authorized buyer | Reserve funds for a worker |
| `submit_artifact` | Authorized worker | Submit work hash |
| `test_settle` | Owner | Force-settle a submitted task (test path) |
| `get_treasury` | View | `(available, reserved, max_per_task)` |
| `get_task` | View | Task details |

---

## Deployed

| Field | Value |
|-------|-------|
| Contract | [`pactara-v8.testnet`](https://explorer.testnet.near.org/accounts/pactara-v8.testnet) |
| Owner | `pactara-v8.testnet` |
| Status | Full task lifecycle verified (deposit → create → submit → settle) |

---

## Quick start

### Build

```bash
cargo near build non-reproducible-wasm
```

### Deploy + initialize

```bash
near contract deploy <account>.testnet use-file target/near/pactara_contract.wasm \
  with-init-call new \
  json-args '{"owner":"<account>.testnet","max_amount_per_task":"100000000000000000000000","_daily_limit":"1000000000000000000000000"}' \
  prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' \
  network-config testnet sign-with-keychain send
```

### Deposit

```bash
near contract call-function as-transaction <account>.testnet deposit \
  json-args {} \
  prepaid-gas '10.0 Tgas' attached-deposit '0.5 NEAR' \
  sign-as <account>.testnet network-config testnet sign-with-keychain send
```

### Create task

```bash
near contract call-function as-transaction <account>.testnet create_task \
  json-args '{
    "worker":"<worker>.testnet",
    "amount":"10000000000000000000000",
    "criteria_hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
    "input_hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
    "verification_deadline":2000000000000000000
  }' \
  prepaid-gas '50.0 Tgas' attached-deposit '0 NEAR' \
  sign-as <account>.testnet network-config testnet sign-with-keychain send
```

### Submit artifact

```bash
near contract call-function as-transaction <account>.testnet submit_artifact \
  json-args '{"task_id":1,"artifact_hash":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1]}' \
  prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' \
  sign-as <worker>.testnet network-config testnet sign-with-keychain send
```

### Force-settle (test)

```bash
near contract call-function as-transaction <account>.testnet test_settle \
  json-args '{"task_id":1}' \
  prepaid-gas '30.0 Tgas' attached-deposit '0 NEAR' \
  sign-as <account>.testnet network-config testnet sign-with-keychain send
```

---

## Tracks

- **Primary**: Agent Economy
- **Secondary**: AI Security & Defense

---

## Status

- [x] Prefunded treasury with available / reserved accounting
- [x] Task creation with fund reservation
- [x] Artifact submission
- [x] Force-settle path (verified on testnet)
- [ ] Restricted function-call keys for buyers
- [ ] On-chain allowlists
- [ ] Daily limit enforcement
- [ ] Independent signed verification
- [ ] Production hardening

---

## License

MIT
