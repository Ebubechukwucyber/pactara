#!/usr/bin/env python3
"""
Pactara off-chain verifier.
Evaluates an artifact against criteria, signs the result with ed25519,
and submits the verification to the contract.

Usage:
  python3 verifier.py <sk_hex_or_base58> <contract_id> <task_id> <criteria.json> <artifact.json> <account_id>

The private key can be hex or the base58 key from ~/.near-credentials.
"""

import sys
import json
import hashlib
import subprocess
import base58


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def evaluate(criteria: dict, artifact: dict) -> bool:
    schema = criteria.get("schema", {})
    for key, rule in schema.items():
        if key not in artifact:
            if rule.get("required", False):
                return False
            continue
        val = artifact[key]
        if rule.get("type") == "string" and not isinstance(val, str):
            return False
        if rule.get("type") == "number" and not isinstance(val, (int, float)):
            return False
        if "minLength" in rule and isinstance(val, str) and len(val) < rule["minLength"]:
            return False
        if "min" in rule and isinstance(val, (int, float)) and val < rule["min"]:
            return False
        if "max" in rule and isinstance(val, (int, float)) and val > rule["max"]:
            return False
    return True


def build_message(contract_id: str, task_id: int, criteria_hash: str,
                  input_hash: str, artifact_hash: str, passed: bool) -> bytes:
    msg = b"pactara-v1"
    msg += b"testnet"
    msg += contract_id.encode()
    msg += task_id.to_bytes(8, "little")
    msg += bytes.fromhex(criteria_hash)
    msg += bytes.fromhex(input_hash)
    msg += bytes.fromhex(artifact_hash)
    msg += bytes([1 if passed else 0])
    return msg


def main():
    if len(sys.argv) != 7:
        print("Usage: verifier.py <sk> <contract_id> <task_id> <criteria.json> <artifact.json> <account_id>")
        sys.exit(1)

    sk_input, contract_id, task_id_s, criteria_path, artifact_path, account_id = sys.argv[1:]
    task_id = int(task_id_s)

    with open(criteria_path) as f:
        criteria = json.load(f)
    with open(artifact_path) as f:
        artifact = json.load(f)

    criteria_hash = sha256(json.dumps(criteria, sort_keys=True).encode())
    input_hash = sha256(json.dumps(criteria.get("input", {}), sort_keys=True).encode())
    artifact_hash = sha256(json.dumps(artifact, sort_keys=True).encode())

    passed = evaluate(criteria, artifact)
    message = build_message(contract_id, task_id, criteria_hash, input_hash, artifact_hash, passed)

    from nacl.signing import SigningKey
    try:
        sk_bytes = bytes.fromhex(sk_input)
    except ValueError:
        sk_bytes = base58.b58decode(sk_input)
    if len(sk_bytes) == 64:
        sk_bytes = sk_bytes[:32]
    sk = SigningKey(sk_bytes)
    sig = sk.sign(message).signature.hex()

    def hex_to_bytes(h: str) -> list:
        return list(bytes.fromhex(h))

    print(f"passed={passed}")
    print(f"criteria_hash={criteria_hash}")
    print(f"artifact_hash={artifact_hash}")
    print(f"signature={sig}")

    cmd = [
        "near", "contract", "call-function", "as-transaction",
        contract_id, "submit_verification",
        "json-args", json.dumps({
            "task_id": task_id,
            "criteria_hash": hex_to_bytes(criteria_hash),
            "input_hash": hex_to_bytes(input_hash),
            "artifact_hash": hex_to_bytes(artifact_hash),
            "passed": passed,
            "signature": hex_to_bytes(sig),
        }),
        "prepaid-gas", "100.0 Tgas",
        "attached-deposit", "0 NEAR",
        "sign-as", account_id,
        "network-config", "testnet",
        "sign-with-keychain",
        "send",
    ]
    print("Submitting verification...")
    subprocess.run(cmd, check=True)
    print("Done.")


if __name__ == "__main__":
    main()
