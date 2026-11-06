#!/usr/bin/env python3
"""
Pactara off-chain verifier.

This program evaluates an artifact against acceptance criteria, then signs a
domain-separated message that the contract verifies on-chain.

A valid signature proves that the registered verifier key signed this result.
It does not prove that the work is objectively correct.

Canonical JSON (shared by buyer, worker, and verifier):
  json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=True)

Input hash is SHA-256 of the task input document, not of a field inside the
criteria object.

Do not pass private keys on the command line. Use:
  PACTARA_VERIFIER_KEY_FILE  path to hex, base58, raw 32-byte seed, or NEAR credentials JSON
  PACTARA_VERIFIER_SK        hex or base58 seed (discouraged; file is preferred)

Usage:
  python3 verifier.py hash-file path.json
  python3 verifier.py pubkey
  python3 verifier.py submit --contract ID --task-id N --criteria c.json --input i.json --artifact a.json --account ACCOUNT
  python3 verifier.py self-test
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import stat
import subprocess
import sys
import urllib.error
import urllib.request
from typing import Any


PROTOCOL_ID = b"pactara-v1"
NETWORK_ID = b"testnet"
DEFAULT_RPC = "https://rpc.testnet.near.org"


def canonical_dumps(obj: Any) -> bytes:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode("utf-8")


def sha256_bytes(data: bytes) -> bytes:
    return hashlib.sha256(data).digest()


def hash_file(path: str) -> bytes:
    with open(path, encoding="utf-8") as handle:
        obj = json.load(handle)
    return sha256_bytes(canonical_dumps(obj))


def hex_digest(digest: bytes) -> str:
    return digest.hex()


def digest_to_near_array(digest: bytes) -> list[int]:
    return list(digest)


def load_json(path: str) -> Any:
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def is_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def is_number(value: Any) -> bool:
    return is_int(value) or isinstance(value, float)


def evaluate_value(value: Any, spec: Any) -> bool:
    if not isinstance(spec, dict):
        return False
    expected = spec.get("type")
    if expected is None:
        return False
    if expected == "string":
        if not isinstance(value, str):
            return False
        if "minLength" in spec and len(value) < spec["minLength"]:
            return False
        if "maxLength" in spec and len(value) > spec["maxLength"]:
            return False
        return True
    if expected == "integer":
        if not is_int(value):
            return False
        if "min" in spec and value < spec["min"]:
            return False
        if "max" in spec and value > spec["max"]:
            return False
        return True
    if expected == "number":
        if not is_number(value):
            return False
        if "min" in spec and value < spec["min"]:
            return False
        if "max" in spec and value > spec["max"]:
            return False
        return True
    if expected == "boolean":
        return isinstance(value, bool)
    if expected == "object":
        if not isinstance(value, dict):
            return False
        properties = spec.get("properties")
        if not isinstance(properties, dict):
            return False
        required = spec.get("required_keys", [])
        if not isinstance(required, list):
            return False
        for key in required:
            if key not in value:
                return False
        for key, nested in properties.items():
            if key in value and not evaluate_value(value[key], nested):
                return False
        return True
    return False


def evaluate(criteria: Any, artifact: Any) -> bool:
    if not isinstance(criteria, dict) or not isinstance(artifact, dict):
        return False
    schema = criteria.get("schema")
    if not isinstance(schema, dict) or not schema:
        return False
    for key, rule in schema.items():
        if not isinstance(rule, dict):
            return False
        required = bool(rule.get("required", False))
        if key not in artifact:
            if required:
                return False
            continue
        if not evaluate_value(artifact[key], rule):
            return False
    return True


def build_message(
    contract_id: str,
    task_id: int,
    criteria_hash: bytes,
    input_hash: bytes,
    artifact_hash: bytes,
    passed: bool,
    network_id: bytes = NETWORK_ID,
) -> bytes:
    return (
        PROTOCOL_ID
        + network_id
        + contract_id.encode()
        + int(task_id).to_bytes(8, "little")
        + criteria_hash
        + input_hash
        + artifact_hash
        + bytes([1 if passed else 0])
    )


def load_signing_key_bytes() -> bytes:
    path = os.environ.get("PACTARA_VERIFIER_KEY_FILE")
    raw: str | None
    if path:
        with open(path, encoding="utf-8") as handle:
            raw = handle.read().strip()
        mode = stat.S_IMODE(os.stat(path).st_mode)
        if mode & (stat.S_IRWXG | stat.S_IRWXO):
            print("warning: verifier key file is group/world accessible", file=sys.stderr)
    else:
        raw = os.environ.get("PACTARA_VERIFIER_SK")
    if not raw:
        raise SystemExit(
            "Set PACTARA_VERIFIER_KEY_FILE or PACTARA_VERIFIER_SK. Do not pass keys as arguments."
        )
    return decode_secret(raw)


def decode_secret(raw: str) -> bytes:
    if raw.startswith("{"):
        parsed = json.loads(raw)
        raw = parsed.get("private_key") or parsed.get("secret_key") or ""
    if raw.startswith("ed25519:"):
        raw = raw.split(":", 1)[1]
    try:
        import base58
    except ImportError:
        base58 = None  # type: ignore
    try:
        sk_bytes = bytes.fromhex(raw)
    except ValueError:
        if base58 is None:
            raise SystemExit("Install base58 to decode NEAR-format keys: pip install base58")
        sk_bytes = base58.b58decode(raw)
    if len(sk_bytes) == 64:
        sk_bytes = sk_bytes[:32]
    if len(sk_bytes) != 32:
        raise SystemExit("Verifier secret key must be 32 bytes")
    return sk_bytes


def signing_key():
    from nacl.signing import SigningKey

    return SigningKey(load_signing_key_bytes())


def near_public_key(sk) -> str:
    import base58

    return "ed25519:" + base58.b58encode(bytes(sk.verify_key)).decode()


def rpc_url() -> str:
    return os.environ.get("NEAR_RPC_URL", DEFAULT_RPC)


def view_call(contract_id: str, method: str, args: dict) -> Any:
    payload = {
        "jsonrpc": "2.0",
        "id": "pactara",
        "method": "query",
        "params": {
            "request_type": "call_function",
            "finality": "final",
            "account_id": contract_id,
            "method_name": method,
            "args_base64": base64.b64encode(json.dumps(args).encode()).decode(),
        },
    }
    request = urllib.request.Request(
        rpc_url(),
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            body = json.loads(response.read().decode())
    except urllib.error.URLError as exc:
        raise SystemExit(f"RPC query failed: {exc}") from exc
    if body.get("error"):
        raise SystemExit(f"RPC error: {body['error']}")
    result = body["result"]["result"]
    return json.loads(bytes(result).decode())


def bytes_from_near(value: Any) -> bytes:
    if isinstance(value, list):
        return bytes(value)
    if isinstance(value, str):
        return bytes.fromhex(value)
    raise SystemExit(f"Unexpected hash encoding: {type(value)}")


def cmd_hash_file(path: str, fmt: str) -> None:
    digest = hash_file(path)
    if fmt == "hex":
        print(hex_digest(digest))
    elif fmt == "near":
        print(json.dumps(digest_to_near_array(digest)))
    else:
        print(hex_digest(digest))
        print(json.dumps(digest_to_near_array(digest)))


def cmd_pubkey() -> None:
    print(near_public_key(signing_key()))


def cmd_submit(args: argparse.Namespace) -> None:
    criteria = load_json(args.criteria)
    artifact = load_json(args.artifact)
    input_obj = load_json(args.input)

    criteria_hash = sha256_bytes(canonical_dumps(criteria))
    input_hash = sha256_bytes(canonical_dumps(input_obj))
    artifact_hash = sha256_bytes(canonical_dumps(artifact))

    task = view_call(args.contract, "get_task", {"task_id": args.task_id})
    if task is None:
        raise SystemExit(f"Task {args.task_id} not found")
    if task.get("state") != "Submitted":
        raise SystemExit(f"Task is {task.get('state')}, expected Submitted")

    on_chain_criteria = bytes_from_near(task["criteria_hash"])
    on_chain_input = bytes_from_near(task["input_hash"])
    on_chain_artifact = bytes_from_near(task["artifact_hash"])
    if on_chain_criteria != criteria_hash:
        raise SystemExit("Criteria hash does not match the on-chain task")
    if on_chain_input != input_hash:
        raise SystemExit("Input hash does not match the on-chain task")
    if on_chain_artifact != artifact_hash:
        raise SystemExit("Artifact hash does not match the on-chain task")
    if int(task["id"]) != int(args.task_id):
        raise SystemExit("Task id mismatch")
    if args.expect_worker and task["worker"] != args.expect_worker:
        raise SystemExit(f"Worker mismatch: on-chain {task['worker']}")

    registered = view_call(args.contract, "get_verifier_public_key", {})
    sk = signing_key()
    local_pk = near_public_key(sk)
    if registered != local_pk:
        raise SystemExit(
            f"Signing key does not match registered verifier public key ({registered})"
        )

    passed = evaluate(criteria, artifact)
    network_id = view_call(args.contract, "get_network_id", {})
    if not isinstance(network_id, str) or not network_id:
        raise SystemExit("Contract get_network_id must return a non-empty string")
    protocol_id = view_call(args.contract, "get_protocol_id", {})
    if protocol_id != PROTOCOL_ID.decode():
        raise SystemExit(f"Unexpected protocol id: {protocol_id}")
    message = build_message(
        args.contract,
        args.task_id,
        criteria_hash,
        input_hash,
        artifact_hash,
        passed,
        network_id.encode(),
    )
    expected_message = bytes_from_near(
        view_call(
            args.contract,
            "get_signing_message",
            {
                "task_id": args.task_id,
                "criteria_hash": digest_to_near_array(criteria_hash),
                "input_hash": digest_to_near_array(input_hash),
                "artifact_hash": digest_to_near_array(artifact_hash),
                "passed": passed,
            },
        )
    )
    if message != expected_message:
        raise SystemExit("Local signing message does not match contract get_signing_message")

    signature = bytes(sk.sign(message).signature)

    print(f"passed={passed}")
    print(f"criteria_hash={hex_digest(criteria_hash)}")
    print(f"input_hash={hex_digest(input_hash)}")
    print(f"artifact_hash={hex_digest(artifact_hash)}")
    print(f"signature={signature.hex()}")
    print("note: signature proves the registered verifier signed this result, not objective correctness")

    if args.dry_run:
        return

    cmd = [
        "near",
        "contract",
        "call-function",
        "as-transaction",
        args.contract,
        "submit_verification",
        "json-args",
        json.dumps(
            {
                "task_id": args.task_id,
                "criteria_hash": digest_to_near_array(criteria_hash),
                "input_hash": digest_to_near_array(input_hash),
                "artifact_hash": digest_to_near_array(artifact_hash),
                "passed": passed,
                "signature": list(signature),
            }
        ),
        "prepaid-gas",
        "100.0 Tgas",
        "attached-deposit",
        "0 NEAR",
        "sign-as",
        args.account,
        "network-config",
        "testnet",
        "sign-with-keychain",
        "send",
    ]
    print("Submitting verification...")
    subprocess.run(cmd, check=True)
    print("Done.")


def cmd_self_test() -> None:
    criteria = {
        "schema": {
            "summary": {"type": "string", "minLength": 4, "required": True},
            "score": {"type": "integer", "min": 0, "max": 100, "required": True},
            "ok": {"type": "boolean", "required": True},
            "meta": {
                "type": "object",
                "required": True,
                "required_keys": ["source"],
                "properties": {
                    "source": {"type": "string"},
                },
            },
        }
    }
    good = {
        "summary": "done",
        "score": 10,
        "ok": True,
        "meta": {"source": "worker"},
    }
    assert evaluate(criteria, good)
    assert not evaluate(criteria, {**good, "ok": 1})
    assert not evaluate(criteria, {**good, "score": True})
    assert not evaluate(criteria, {**good, "score": 10.5})
    assert not evaluate(criteria, {**good, "meta": "nope"})
    assert not evaluate(criteria, {**good, "meta": {}})
    assert not evaluate({"schema": {"x": {"type": "array"}}}, {"x": []})
    assert not evaluate({"schema": {}}, good)
    assert canonical_dumps({"b": 1, "a": 2}) == b'{"a":2,"b":1}'
    print("self-test passed")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Pactara verifier")
    sub = parser.add_subparsers(dest="cmd", required=True)

    hash_file_cmd = sub.add_parser("hash-file", help="Print canonical SHA-256 of a JSON file")
    hash_file_cmd.add_argument("path")
    hash_file_cmd.add_argument("--format", choices=["hex", "near", "both"], default="both")

    sub.add_parser("pubkey", help="Print the NEAR-format public key for the configured secret")

    submit = sub.add_parser("submit", help="Evaluate, sign, and submit verification")
    submit.add_argument("--contract", required=True)
    submit.add_argument("--task-id", required=True, type=int)
    submit.add_argument("--criteria", required=True)
    submit.add_argument("--input", required=True)
    submit.add_argument("--artifact", required=True)
    submit.add_argument("--account", required=True, help="Account that submits the signed result")
    submit.add_argument("--expect-worker")
    submit.add_argument("--dry-run", action="store_true")

    sub.add_parser("self-test", help="Run local evaluation tests")
    return parser


def main(argv: list[str] | None = None) -> None:
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.cmd == "hash-file":
        cmd_hash_file(args.path, args.format)
    elif args.cmd == "pubkey":
        cmd_pubkey()
    elif args.cmd == "submit":
        cmd_submit(args)
    elif args.cmd == "self-test":
        cmd_self_test()
    else:
        parser.print_help()
        sys.exit(1)


if __name__ == "__main__":
    main()
