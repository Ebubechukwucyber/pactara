#!/usr/bin/env python3
"""Buyer key preflight + verifier hashing/signing pieces, without a live chain."""

import os
import unittest
import unittest.mock

import pactara_rpc
import verifier
from test_pactara_rpc import fn_key


class BuyerVerifierIntegrationTests(unittest.TestCase):
    def test_restricted_key_then_canonical_hashes_then_message(self):
        pactara_rpc.check_buyer_key(
            "buyer.testnet",
            "ed25519:abc",
            "pactara.testnet",
            fn_key(["create_task"]),
        )
        criteria = {
            "schema": {
                "summary": {"type": "string", "minLength": 4, "required": True},
            }
        }
        artifact = {"summary": "done"}
        self.assertTrue(verifier.evaluate(criteria, artifact))
        criteria_hash = verifier.sha256_bytes(verifier.canonical_dumps(criteria))
        input_hash = verifier.sha256_bytes(verifier.canonical_dumps({"job": 1}))
        artifact_hash = verifier.sha256_bytes(verifier.canonical_dumps(artifact))
        message = verifier.build_message(
            "pactara.testnet",
            1,
            criteria_hash,
            input_hash,
            artifact_hash,
            True,
            b"testnet",
        )
        self.assertTrue(message.startswith(b"pactara-v1testnetpactara.testnet"))
        env = {"NEAR_NETWORK": "testnet", "NEAR_RPC_URL": "https://rpc.testnet.near.org"}
        with unittest.mock.patch.dict(os.environ, env, clear=False):
            verifier.assert_network_consistent("testnet")


if __name__ == "__main__":
    unittest.main()
