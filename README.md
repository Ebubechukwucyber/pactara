# Pactara

Agents can hire and pay other agents without being trusted with the money.

Pactara is a NEAR contract that holds a prefunded treasury, reserves funds when an authorized buyer creates a task for an authorized worker, and releases payment only to the original worker after independent signed verification. The hiring agent uses a restricted function-call key. The contract enforces buyer and worker allowlists, daily spending limits (committed), and binds verification to the stored criteria and artifact hashes with domain separation.

## How it works

```
Authorized Buyer (restricted key)
        │
        ▼
   create_task
   (buyer + worker allowlist,
    daily committed limit, amount checks)
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
   Independent Verifier
   (off-chain JSON check)
        │
        ▼
   Signed verification
   (protocol ID + contract ID + hashes)
   ┌────┴────┐
   PASS     FAIL
   │         │
   ▼         ▼
 Pay worker  Refund
 (callback on failure)
```

## Guarantees

- Prefunded treasury with available / reserved accounting
- On-chain buyer allowlist
- On-chain worker allowlist
- Daily spending limit (committed: counted at creation, not refunded on FAIL/expiry)
- Worker and amount fixed at task creation
- Independent signed verification bound to protocol ID, contract ID, criteria, input, and artifact hashes
- PASS settles to the original worker; failed transfer is reversed via callback
- FAIL or expiry refunds
- Restricted key cannot call owner methods
- One submission per task

## Daily spending policy

The daily limit tracks **committed** spending. Creating a task increments `daily_spent` even if the task later fails verification or expires. This limits how much an agent can commit in a single UTC day, not only successful payouts. The counter resets at the UTC day boundary.

## Deployed

- Contract: `pactara-v6.testnet`
- Owner: `pactara-v6.testnet`
- Verifier: `ed25519:7cEwYj2UDg6HbrjAssTs8cdD96SeMEp9f5NNjXxcu4ex`

## Try it

```bash
chmod +x buyer_agent.sh demo.sh
./buyer_agent.sh
./demo.sh
```

Expected: an authorized buyer creates a task for an authorized worker; the restricted key is rejected when it tries to withdraw; verification requires a valid signature from the registered verifier that includes the protocol and contract identifiers.

## Verifier

```bash
python3 verifier.py <sk_hex> pactara-v5.testnet <task_id> criteria.json artifact.json
```

## Tracks

- Primary: Agent Economy
- Secondary: AI Security & Defense

## Layout

```
├── src/lib.rs
├── Cargo.toml
├── buyer_agent.sh
├── demo.sh
├── verifier.py
├── flow.png
└── README.md
```
