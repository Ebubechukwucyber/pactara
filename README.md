# Pactara

Agents can hire and pay other agents without being trusted with the money.

Pactara is a NEAR contract that holds a prefunded treasury, reserves funds when an authorized buyer creates a task for an authorized worker, and releases payment only to the original worker after independent signed verification. The hiring agent uses a restricted function-call key. The contract enforces buyer and worker allowlists, daily spending limits, and binds verification to the stored criteria and artifact hashes.

## How it works

```
Authorized Buyer (restricted key)
        │
        ▼
   create_task
   (buyer + worker allowlist,
    daily limit, amount checks)
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
   Authorized Worker submits artifact
        │
        ▼
   Signed verification
   (verifier key, criteria/input/artifact hashes)
   ┌────┴────┐
   PASS     FAIL
   │         │
   ▼         ▼
 Pay worker  Refund
```

## Guarantees

- Prefunded treasury with available / reserved accounting
- On-chain buyer allowlist
- On-chain worker allowlist
- Daily spending limit
- Worker and amount fixed at task creation
- Independent signed verification bound to criteria, input, and artifact hashes
- PASS settles to the original worker
- FAIL or expiry refunds
- Restricted key cannot call owner methods
- One submission per task

## Deployed

- Contract: `pactara-v3.testnet`
- Owner: `pactara-v3.testnet`

## Try it

```bash
chmod +x buyer_agent.sh demo.sh
./buyer_agent.sh
./demo.sh
```

Expected: an authorized buyer creates a task for an authorized worker; the restricted key is rejected when it tries to withdraw; verification requires a valid signature from the registered verifier.

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
