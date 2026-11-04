#!/usr/bin/env python3
"""
Pactara Verifier Service
Retrieves artifact, validates against criteria, signs result.
"""
import json
import hashlib
import base64
import sys
from nacl.signing import SigningKey
from nacl.encoding import HexEncoder

PROTOCOL_ID = b"PACTARA-v1"

def sha256(data: bytes) -> bytes:
    return hashlib.sha256(data).digest()

def build_message(contract_id: str, task_id: int, criteria_hash: bytes, input_hash: bytes, artifact_hash: bytes, passed: bool) -> bytes:
    msg = bytearray()
    msg.extend(PROTOCOL_ID)
    msg.extend(contract_id.encode())
    msg.extend(task_id.to_bytes(8, "little"))
    msg.extend(criteria_hash)
    msg.extend(input_hash)
    msg.extend(artifact_hash)
    msg.append(1 if passed else 0)
    return bytes(msg)

def verify_artifact(artifact_json: dict, criteria: dict) -> bool:
    """Simple criteria check. Extend as needed."""
    if "required_fields" in criteria:
        for field in criteria["required_fields"]:
            if field not in artifact_json:
                return False
    if "min_length" in criteria and "content" in artifact_json:
        if len(artifact_json["content"]) < criteria["min_length"]:
            return False
    return True

def main():
    if len(sys.argv) < 6:
        print("Usage: verifier.py <private_key_hex> <contract_id> <task_id> <criteria.json> <artifact.json>")
        sys.exit(1)

    sk_hex = sys.argv[1]
    contract_id = sys.argv[2]
    task_id = int(sys.argv[3])
    criteria_path = sys.argv[4]
    artifact_path = sys.argv[5]

    with open(criteria_path) as f:
        criteria = json.load(f)
    with open(artifact_path) as f:
        artifact = json.load(f)

    criteria_hash = sha256(json.dumps(criteria, sort_keys=True).encode())
    artifact_hash = sha256(json.dumps(artifact, sort_keys=True).encode())
    input_hash = sha256(b"input-placeholder")

    passed = verify_artifact(artifact, criteria)

    message = build_message(contract_id, task_id, criteria_hash, input_hash, artifact_hash, passed)

    sk = SigningKey(sk_hex, encoder=HexEncoder)
    signature = sk.sign(message).signature

    result = {
        "task_id": task_id,
        "passed": passed,
        "criteria_hash": base64.b64encode(criteria_hash).decode(),
        "input_hash": base64.b64encode(input_hash).decode(),
        "artifact_hash": base64.b64encode(artifact_hash).decode(),
        "signature": base64.b64encode(signature).decode(),
        "message_hex": message.hex()
    }
    print(json.dumps(result, indent=2))

if __name__ == "__main__":
    main()
