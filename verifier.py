#!/usr/bin/env python3
"""
Pactara Verifier Service
Retrieves artifact, validates against criteria, signs and submits result.
"""
import json
import hashlib
import base64
import sys
import subprocess

PROTOCOL_ID = b"PACTARA-v1"
NETWORK_ID = b"testnet"

def sha256(data: bytes) -> bytes:
    return hashlib.sha256(data).digest()

def canonical_json(obj) -> bytes:
    return json.dumps(obj, sort_keys=True, separators=(",", ":")).encode()

def build_message(contract_id: str, task_id: int, criteria_hash: bytes, input_hash: bytes, artifact_hash: bytes, passed: bool) -> bytes:
    msg = bytearray()
    msg.extend(PROTOCOL_ID)
    msg.extend(NETWORK_ID)
    msg.extend(contract_id.encode())
    msg.extend(task_id.to_bytes(8, "little"))
    msg.extend(criteria_hash)
    msg.extend(input_hash)
    msg.extend(artifact_hash)
    msg.append(1 if passed else 0)
    return bytes(msg)

def verify_artifact(artifact: dict, criteria: dict) -> bool:
    schema = criteria.get("schema", {})
    for field, spec in schema.items():
        if spec.get("required", False) and field not in artifact:
            return False
        if field in artifact:
            val = artifact[field]
            if "type" in spec and not isinstance(val, {"string": str, "number": (int, float), "boolean": bool}.get(spec["type"], object)):
                return False
            if "min" in spec and isinstance(val, (int, float)) and val < spec["min"]:
                return False
            if "max" in spec and isinstance(val, (int, float)) and val > spec["max"]:
                return False
            if "minLength" in spec and isinstance(val, str) and len(val) < spec["minLength"]:
                return False
    return True

def submit_verification(contract_id: str, task_id: int, passed: bool, criteria_hash: bytes, input_hash: bytes, artifact_hash: bytes, signature: bytes, signer: str):
    args = {
        "task_id": task_id,
        "passed": passed,
        "criteria_hash": list(criteria_hash),
        "input_hash": list(input_hash),
        "artifact_hash": list(artifact_hash),
        "signature": list(signature)
    }
    cmd = [
        "near", "contract", "call-function", "as-transaction", contract_id, "submit_verification",
        "json-args", json.dumps(args),
        "prepaid-gas", "50 Tgas",
        "attached-deposit", "0 NEAR",
        "sign-as", signer,
        "network-config", "testnet",
        "sign-with-keychain",
        "send"
    ]
    result = subprocess.run(cmd, capture_output=True, text=True)
    print(result.stdout)
    if result.returncode != 0:
        print(result.stderr, file=sys.stderr)
        sys.exit(result.returncode)

def main():
    if len(sys.argv) < 7:
        print("Usage: verifier.py <sk_hex> <contract_id> <task_id> <criteria.json> <artifact.json> <signer_account>")
        sys.exit(1)

    sk_hex = sys.argv[1]
    contract_id = sys.argv[2]
    task_id = int(sys.argv[3])
    criteria_path = sys.argv[4]
    artifact_path = sys.argv[5]
    signer = sys.argv[6]

    with open(criteria_path) as f:
        criteria = json.load(f)
    with open(artifact_path) as f:
        artifact = json.load(f)

    criteria_hash = sha256(canonical_json(criteria))
    artifact_hash = sha256(canonical_json(artifact))
    input_hash = sha256(canonical_json(criteria.get("input", {})))

    passed = verify_artifact(artifact, criteria)

    message = build_message(contract_id, task_id, criteria_hash, input_hash, artifact_hash, passed)

    from nacl.signing import SigningKey
    from nacl.encoding import HexEncoder
    sk = SigningKey(sk_hex, encoder=HexEncoder)
    signature = sk.sign(message).signature

    print(json.dumps({
        "task_id": task_id,
        "passed": passed,
        "criteria_hash": base64.b64encode(criteria_hash).decode(),
        "input_hash": base64.b64encode(input_hash).decode(),
        "artifact_hash": base64.b64encode(artifact_hash).decode()
    }, indent=2))

    print("\nSubmitting verification...")
    submit_verification(contract_id, task_id, passed, criteria_hash, input_hash, artifact_hash, signature, signer)

if __name__ == "__main__":
    main()
