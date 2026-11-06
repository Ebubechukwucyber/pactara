#!/usr/bin/env python3
import unittest

from pactara_rpc import KeyCheckError, check_buyer_key


def fn_key(methods, receiver="pactara.testnet", pk="ed25519:abc"):
    return [
        {
            "public_key": pk,
            "access_key": {
                "permission": {
                    "FunctionCall": {
                        "receiver_id": receiver,
                        "method_names": methods,
                        "allowance": "0",
                    }
                }
            },
        }
    ]


class BuyerKeyTests(unittest.TestCase):
    def test_exact_create_task_ok(self):
        check_buyer_key("buyer.testnet", "ed25519:abc", "pactara.testnet", fn_key(["create_task"]))

    def test_full_access_rejected(self):
        keys = [{"public_key": "ed25519:abc", "access_key": {"permission": "FullAccess"}}]
        with self.assertRaises(KeyCheckError):
            check_buyer_key("buyer.testnet", "ed25519:abc", "pactara.testnet", keys)

    def test_wrong_receiver_rejected(self):
        with self.assertRaises(KeyCheckError):
            check_buyer_key(
                "buyer.testnet",
                "ed25519:abc",
                "pactara.testnet",
                fn_key(["create_task"], receiver="other.testnet"),
            )

    def test_extra_methods_rejected(self):
        with self.assertRaises(KeyCheckError):
            check_buyer_key(
                "buyer.testnet",
                "ed25519:abc",
                "pactara.testnet",
                fn_key(["create_task", "withdraw"]),
            )

    def test_empty_methods_rejected(self):
        with self.assertRaises(KeyCheckError):
            check_buyer_key("buyer.testnet", "ed25519:abc", "pactara.testnet", fn_key([]))

    def test_missing_key_rejected(self):
        with self.assertRaises(KeyCheckError):
            check_buyer_key("buyer.testnet", "ed25519:abc", "pactara.testnet", [])

    def test_wrong_account_key_list(self):
        with self.assertRaises(KeyCheckError):
            check_buyer_key(
                "buyer.testnet",
                "ed25519:not-installed",
                "pactara.testnet",
                fn_key(["create_task"]),
            )


if __name__ == "__main__":
    unittest.main()
