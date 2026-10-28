"""Exact-token JSON normalization and independent decimal controls."""
import hashlib
import json
import unittest
from decimal import Decimal

from zal_notation import normalizer
from zal_notation.core import MAX_BYTES, NotationError


class ExactJsonNormalizerTests(unittest.TestCase):
    def test_decimal_values_and_tokens_survive(self):
        cases = (
            "1e-400",
            "9007199254740993.0",
            "0.10000000000000000000001",
            "0.5",
            "1e+10000",
            "-0.00E-02",
        )
        for literal in cases:
            with self.subTest(literal=literal):
                source = '{ "value" : ' + literal + ' }'
                result = normalizer.canonical_json(source)
                self.assertEqual(result["source"], source)
                self.assertEqual(result["result"], '{"value":' + literal + '}')
                original = json.loads(source, parse_float=Decimal, parse_int=Decimal)["value"]
                normalized = json.loads(result["result"], parse_float=Decimal, parse_int=Decimal)["value"]
                self.assertEqual(original, normalized)
                self.assertEqual(result["data_digest"], hashlib.sha256(result["result"].encode("utf-8")).hexdigest())
                self.assertIn("numeric tokens retained", result["scope"])

    def test_types_arrays_unicode_and_idempotence(self):
        source = '{"z":[true,1,1.0,1e0,-0,-0.0,0.5,false,0,0.0,null,"1"],"\\u0061":"µ"}'
        expected = '{"a":"µ","z":[true,1,1.0,1e0,-0,-0.0,0.5,false,0,0.0,null,"1"]}'
        result = normalizer.canonical_json(source)
        self.assertEqual(result["result"], expected)
        again = normalizer.canonical_json(result["result"])
        self.assertEqual(again["result"], expected)
        self.assertEqual(again["data_digest"], result["data_digest"])
        decoded = json.loads(expected, parse_float=Decimal)
        self.assertIs(decoded["z"][0], True)
        self.assertIs(type(decoded["z"][1]), int)
        self.assertIs(type(decoded["z"][2]), Decimal)
        self.assertEqual(decoded["z"][-1], "1")

    def test_duplicate_nonfinite_and_invalid_inputs_refused(self):
        invalid = (
            '{"x":1,"\\u0078":2}', '{"x":NaN}', '{"x":Infinity}', '{"x":-Infinity}',
            '{"x":01}', '{"x":1.}', '{"x":+1}', '{"x":1e}',
            '{"x":"\\ud800"}', '{"x":"\\u0000"}', '[0]',
        )
        for source in invalid:
            with self.subTest(source=source), self.assertRaises(NotationError):
                normalizer.canonical_json(source)

    def test_existing_resource_bounds_remain_enforced(self):
        cases = (
            " " * (MAX_BYTES + 1),
            '{"x":"' + ("é" * (MAX_BYTES // 2)) + '"}',
            '{"x":"' + ("x" * 16001) + '"}',
            '{"x":' + ("[" * 150) + "0" + ("]" * 150) + '}',
            '{"x":[' + ("0," * 49999) + '0]}',
        )
        for source in cases:
            with self.subTest(length=len(source)), self.assertRaises(NotationError):
                normalizer.canonical_json(source)


if __name__ == "__main__":
    unittest.main()
