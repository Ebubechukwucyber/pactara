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
EXPECTED_CODE_VERSION = "pactara-2.1.0"


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


def check_buyer_key(account_id: str, public_key: str, contract_id: str) -> None:
    keys = view_access_keys(account_id)
    match = None
    for item in keys:
        if item.get("public_key") == public_key:
            match = item
            break
    if match is None:
        raise SystemExit(f"Public key is not installed on {account_id}")
    permission = match.get("access_key", {}).get("permission")
    if permission == "FullAccess":
        raise SystemExit("Buyer key is FullAccess; it must be function-call only (create_task)")
    if not isinstance(permission, dict) or "FunctionCall" not in permission:
        raise SystemExit(f"Buyer key is not a FunctionCall key: {permission!r}")
    fn = permission["FunctionCall"]
    receiver = fn.get("receiver_id")
    methods = fn.get("method_names") or []
    if receiver != contract_id:
        raise SystemExit(f"Buyer key receiver is {receiver!r}, expected {contract_id}")
    if methods and "create_task" not in methods:
        raise SystemExit(f"Buyer key methods {methods} do not include create_task")
    privileged = {"withdraw", "set_policy", "register_verifier", "add_authorized_buyer", "add_authorized_worker"}
    if any(name in privileged for name in methods):
        raise SystemExit(f"Buyer key allows privileged methods: {methods}")
    print("Buyer key is FunctionCall")
    print(f"receiver={receiver}")
    print(f"methods={methods or ['(any method on receiver; restrict to create_task)']}")
    if not methods:
        raise SystemExit("Buyer key allows every method on the contract; restrict it to create_task")


def main() -> None:
    if len(sys.argv) < 2:
        raise SystemExit("Usage: pactara_rpc.py check-buyer-key <account> <public_key> <contract>")
    cmd = sys.argv[1]
    if cmd == "check-buyer-key":
        if len(sys.argv) != 5:
            raise SystemExit("check-buyer-key <account> <public_key> <contract>")
        check_buyer_key(sys.argv[2], sys.argv[3], sys.argv[4])
    else:
        raise SystemExit(f"Unknown command {cmd}")


if __name__ == "__main__":
    main()
