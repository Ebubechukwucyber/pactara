#!/usr/bin/env python3
"""NEAR JSON-RPC helpers for Pactara scripts. Does not print private keys."""

from __future__ import annotations

import json
import os
import sys
import urllib.request
from typing import Any

DEFAULT_RPC = {
    "testnet": "https://rpc.testnet.near.org",
    "mainnet": "https://rpc.mainnet.near.org",
}
EXPECTED_CODE_VERSION = "pactara-2.2.0"


class KeyCheckError(ValueError):
    pass


def rpc_url() -> str:
    if os.environ.get("NEAR_RPC_URL"):
        return os.environ["NEAR_RPC_URL"]
    network = os.environ.get("NEAR_NETWORK", "testnet")
    return DEFAULT_RPC.get(network, DEFAULT_RPC["testnet"])


def rpc(method: str, params: Any) -> Any:
    payload = {"jsonrpc": "2.0", "id": "pactara", "method": method, "params": params}
    request = urllib.request.Request(
        rpc_url(),
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        body = json.loads(response.read().decode())
    if body.get("error"):
        raise SystemExit(f"RPC error: {body['error']}")
    return body["result"]


def view_access_keys(account_id: str) -> list[dict]:
    result = rpc(
        "query",
        {
            "request_type": "view_access_key_list",
            "finality": "final",
            "account_id": account_id,
        },
    )
    return result.get("keys") or []


def view_function(contract_id: str, method: str, args: dict) -> Any:
    import base64

    result = rpc(
        "query",
        {
            "request_type": "call_function",
            "finality": "final",
            "account_id": contract_id,
            "method_name": method,
            "args_base64": base64.b64encode(json.dumps(args).encode()).decode(),
        },
    )
    raw = result.get("result")
    if raw is None:
        raise SystemExit(f"No result for {method}")
    return json.loads(bytes(raw).decode())


def check_buyer_key(
    account_id: str,
    public_key: str,
    contract_id: str,
    keys: list[dict] | None = None,
) -> None:
    if keys is None:
        keys = view_access_keys(account_id)
    match = None
    for item in keys:
        if item.get("public_key") == public_key:
            match = item
            break
    if match is None:
        raise KeyCheckError(f"Public key is not installed on {account_id}")
    permission = match.get("access_key", {}).get("permission")
    if permission is None:
        raise KeyCheckError("Missing access_key.permission")
    if permission == "FullAccess":
        raise KeyCheckError("Buyer key is FullAccess; it must be function-call only (create_task)")
    if not isinstance(permission, dict) or "FunctionCall" not in permission:
        raise KeyCheckError(f"Buyer key is not a FunctionCall key: {permission!r}")
    fn = permission["FunctionCall"]
    receiver = fn.get("receiver_id")
    methods = fn.get("method_names")
    if receiver != contract_id:
        raise KeyCheckError(f"Buyer key receiver is {receiver!r}, expected {contract_id}")
    if not isinstance(methods, list):
        raise KeyCheckError("Buyer key method_names is missing")
    if methods != ["create_task"]:
        raise KeyCheckError(
            f"Buyer key methods must be exactly ['create_task'], got {methods!r}"
        )
    print("Buyer key is FunctionCall create_task only")
    print(f"receiver={receiver}")
    allowance = fn.get("allowance")
    print(f"allowance={allowance}")


def main() -> None:
    if len(sys.argv) < 2:
        raise SystemExit("Usage: pactara_rpc.py check-buyer-key <account> <public_key> <contract>")
    cmd = sys.argv[1]
    if cmd == "check-buyer-key":
        if len(sys.argv) != 5:
            raise SystemExit("check-buyer-key <account> <public_key> <contract>")
        try:
            check_buyer_key(sys.argv[2], sys.argv[3], sys.argv[4])
        except KeyCheckError as exc:
            raise SystemExit(str(exc)) from exc
    elif cmd == "view":
        if len(sys.argv) != 5:
            raise SystemExit("view <contract> <method> <json-args>")
        print(json.dumps(view_function(sys.argv[2], sys.argv[3], json.loads(sys.argv[4]))))
    else:
        raise SystemExit(f"Unknown command {cmd}")


if __name__ == "__main__":
    main()
