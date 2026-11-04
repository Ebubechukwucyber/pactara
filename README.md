# Pactara

Agents can hire and pay other agents without being trusted with the money.

Pactara is a NEAR contract that holds a prefunded treasury, reserves funds when an authorized buyer creates a task, and releases payment only to the original worker after verification. The hiring agent uses a restricted function-call key. The contract also enforces an on-chain buyer allowlist so arbitrary accounts cannot spend from the treasury.

## How it works

```
Authorized Buyer (restricted key)
        │
        ▼
   create_task
   (allowlist check)
        │
        ▼
┌─────────────────────┐
│   Pactara Contract  │
│  available → reserved│
│  worker + amount    │
│  locked             │
└─────────────────────┘
        │
        ▼
   Worker submits artifact
        │
        ▼
   Verification (owner-gated MVP)
   ┌────┴────┐
   PASS     FAIL
   │         │
   ▼         ▼
 Pay worker  Refund
```

## Guarantees (current)

- Prefunded treasury with available / reserved accounting
- On-chain buyer allowlist (only authorized buyers can create tasks)
- Worker and amount fixed at task creation
- PASS settles to the original worker
- FAIL or expiry refunds
- Restricted key cannot call owner methods
- One submission per task

## Not yet implemented

- Independent signed verifier (verification is owner-gated)
- Criteria and input hash enforcement during verification
- Daily spending limits
- Worker allowlist

These are documented as next steps. The settlement rules and buyer authorization are enforced today.

## Deployed

- Contract: `pactara-v2.testnet`
- Owner: `pactara-v2.testnet`

## Try it

```bash
chmod +x buyer_agent.sh demo.sh
./buyer_agent.sh
./demo.sh
```

Expected: an authorized buyer creates a task; the restricted key is rejected when it tries to withdraw.

## Tracks

- Primary: Agent Economy
- Secondary: AI Security & Defense

## Layout

```
├── src/lib.rs
├── Cargo.toml
├── buyer_agent.sh
├── demo.sh
├── flow.png
└── README.md
```
