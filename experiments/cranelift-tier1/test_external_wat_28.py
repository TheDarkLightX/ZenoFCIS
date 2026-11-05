#!/usr/bin/env python3
"""Manifest/independent-oracle/refusal controls; no compiler result implied."""
from __future__ import annotations
import unittest
from pathlib import Path
from unittest.mock import patch
from qualify_external_wat_corpus_28 import (
    MANIFEST, SCHEMA, SMOKES, TARGET, validate_fixture,
)
from independent_recheck_external_wat_28 import oracle_bswap


class ExternalCorpusControls(unittest.TestCase):
    def test_manifest_is_deterministic_unique_and_external(self):
        self.assertEqual(SCHEMA, "zeno-fcis/tier1-external-upstream-wat-screen/1")
        self.assertEqual(TARGET, "x86_64-unknown-linux")
        self.assertEqual(len(MANIFEST), 12)
        self.assertEqual(len({p for p, b in MANIFEST}), len(MANIFEST))
        for path, blob in MANIFEST:
            self.assertTrue(path.startswith(("tests/disas/", "tests/all/cli_tests/")))
            self.assertTrue(path.endswith(".wat"))
            self.assertEqual(len(blob), 40)
            self.assertNotIn("..", Path(path).parts)
        self.assertEqual(
            set(SMOKES), {
                "tests/disas/byteswap.wat",
                "tests/all/cli_tests/simple.wat",
            })

    def test_exact_reversal_known_vectors_independent_of_producer(self):
        self.assertEqual(oracle_bswap(0x12345678, 32), 0x78563412)
        self.assertEqual(oracle_bswap(0x01020304, 32), 0x04030201)
        self.assertEqual(oracle_bswap(0x0102030405060708, 64),
                         0x0807060504030201)
        self.assertEqual(oracle_bswap(0, 64), 0)
        self.assertEqual(oracle_bswap(-1, 32), -1)
        self.assertEqual(oracle_bswap(-1, 64), -1)
        self.assertEqual(oracle_bswap(0x80000000, 32), 128)

    def test_refuse_nonexternal_filename_before_git_access(self):
        for path in ("/tmp/dummy.wat", "../upstream.wat", "README.md",
                     "tests/disas/../foo.wat", "tests/disas/arith.wast"):
            with self.subTest(path=path):
                with self.assertRaises(ValueError):
                    validate_fixture(Path("/tmp/nonexistent"), path, "0" * 40)

    def test_reject_wrong_source_pin_before_reading_disk(self):
        with patch("qualify_external_wat_corpus_28.git",
                   return_value="different") as query:
            with self.assertRaisesRegex(ValueError, "source git blob"):
                validate_fixture(Path("/tmp/nonexistent"),
                                 MANIFEST[0][0], MANIFEST[0][1])
            query.assert_called_once()

    def test_zero_and_negative_deltas_not_assumed_positive(self):
        baseline = [0, 4096, 8192, 12288]
        patched = [0, 8192, 8192, 4096]
        self.assertEqual(
            [a - b for a, b in zip(baseline, patched)],
            [0, -4096, 0, 8192])


if __name__ == "__main__":
    unittest.main()
