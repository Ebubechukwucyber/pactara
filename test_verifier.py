#!/usr/bin/env python3
import json
import unittest

import verifier


class CanonicalJsonTests(unittest.TestCase):
    def test_key_order_does_not_change_hash(self):
        a = verifier.canonical_dumps({"b": 1, "a": 2})
        b = verifier.canonical_dumps({"a": 2, "b": 1})
        self.assertEqual(a, b)
        self.assertEqual(a, b'{"a":2,"b":1}')

    def test_whitespace_is_normalized(self):
        compact = verifier.sha256_bytes(verifier.canonical_dumps({"k": "v"}))
        pretty = json.loads('{\n  "k": "v"\n}')
        self.assertEqual(compact, verifier.sha256_bytes(verifier.canonical_dumps(pretty)))

    def test_malformed_json_rejected(self):
        with self.assertRaises(json.JSONDecodeError):
            json.loads("{not json")


class CriteriaTests(unittest.TestCase):
    def setUp(self):
        self.criteria = {
            "schema": {
                "summary": {"type": "string", "minLength": 4, "required": True},
                "score": {"type": "integer", "min": 0, "max": 100, "required": True},
                "ok": {"type": "boolean", "required": True},
                "meta": {
                    "type": "object",
                    "required": True,
                    "required_keys": ["source"],
                    "properties": {"source": {"type": "string", "minLength": 1}},
                },
            }
        }
        self.good = {
            "summary": "done",
            "score": 10,
            "ok": True,
            "meta": {"source": "worker"},
        }

    def test_good_artifact_passes(self):
        self.assertTrue(verifier.evaluate(self.criteria, self.good))

    def test_bool_is_not_integer(self):
        self.assertFalse(verifier.evaluate(self.criteria, {**self.good, "score": True}))

    def test_int_is_not_boolean(self):
        self.assertFalse(verifier.evaluate(self.criteria, {**self.good, "ok": 1}))

    def test_string_min_length(self):
        self.assertFalse(verifier.evaluate(self.criteria, {**self.good, "summary": "no"}))

    def test_numeric_bounds(self):
        self.assertFalse(verifier.evaluate(self.criteria, {**self.good, "score": 101}))
        self.assertFalse(verifier.evaluate(self.criteria, {**self.good, "score": -1}))

    def test_nested_required(self):
        self.assertFalse(verifier.evaluate(self.criteria, {**self.good, "meta": {}}))

    def test_unsupported_keyword_rejected(self):
        with self.assertRaises(verifier.SchemaError):
            verifier.evaluate(
                {"schema": {"x": {"type": "string", "pattern": "^a"}}},
                {"x": "a"},
            )

    def test_unsupported_type_rejected(self):
        with self.assertRaises(verifier.SchemaError):
            verifier.evaluate({"schema": {"x": {"type": "array"}}}, {"x": []})


class MessageTests(unittest.TestCase):
    def test_pass_flag_changes_message(self):
        args = ("contract.testnet", 3, b"\x01" * 32, b"\x02" * 32, b"\x03" * 32)
        a = verifier.build_message(*args, True, b"testnet")
        b = verifier.build_message(*args, False, b"testnet")
        self.assertNotEqual(a, b)
        self.assertTrue(a.startswith(b"pactara-v1testnet"))


if __name__ == "__main__":
    unittest.main()
