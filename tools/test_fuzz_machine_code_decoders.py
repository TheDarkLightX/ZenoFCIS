"""The decoder fuzzer passes on the committed decoders and catches planted defects.

Each control plants one defect by patching a decoder table or rule and
requires the fuzzer to report it under the right oracle. A control that the
fuzzer cannot see would be a silent weakening of the oracle, so every control
here asserts the finding, never only the exit code.
"""
from pathlib import Path
import io
import json
import sys
import time
import unittest
from contextlib import redirect_stdout
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import fuzz_machine_code_decoders as fuzz  # noqa: E402
from machine_code import model, profile2, profile3, verify, verify2  # noqa: E402

X86_1, RV_1 = model.TARGETS
X86_2, RV_2 = profile2.TARGETS
X86_3, RV_3 = profile3.TARGETS


def findings(summary, oracle):
    return [row for row in summary["findings"] if row["oracle"] == oracle]


class CleanDecoders(unittest.TestCase):
    def test_every_oracle_passes_on_every_target(self):
        summary = fuzz.run(seed=1, iterations=60, selected=tuple(fuzz.DECODERS))
        self.assertEqual(summary["findings"], [], summary["findings"][:3])
        self.assertEqual(summary["counts"]["round_trip"], 6 * 60)
        self.assertEqual(summary["counts"]["unencodable_draws"], 0, "the drawn programs must stay inside the grammar")
        self.assertGreater(summary["counts"]["mutation_refused"], 0)
        self.assertGreater(summary["counts"]["mutation_accepted"], 0, "some edits must still decode, or the mutation oracle is idle")
        self.assertEqual(summary["counts"]["cross_profile"], 4 * 60 + 2 * 60, "every lower-profile program is decoded by every higher profile")
        for target, histogram in summary["refusals"].items():
            with self.subTest(target=target):
                self.assertTrue(histogram, "no refusal reason was exercised")
                self.assertNotIn("ValueError", histogram, "a refusal without a reason")

    def test_same_seed_reproduces_the_run(self):
        first = fuzz.run(seed=5, iterations=20, selected=(X86_2, RV_3))
        second = fuzz.run(seed=5, iterations=20, selected=(X86_2, RV_3))
        self.assertEqual(first, second)

    def test_cli_exit_zero_and_summary_file(self):
        out = Path(self.id().replace(".", "-") + ".json")
        try:
            with redirect_stdout(io.StringIO()) as stdout:
                code = fuzz.main(["--seed", "3", "--iterations", "10", "--target", RV_1, "--json", str(out)])
            self.assertEqual(code, 0)
            self.assertIn("machine-code decoder fuzz: PASS", stdout.getvalue())
            summary = json.loads(out.read_text())
            self.assertEqual(summary["schema"], "zeno-fcis/machine-code-decoder-fuzz/1")
            self.assertEqual(summary["targets"], [RV_1])
            self.assertEqual(summary["authority"], "none")
        finally:
            out.unlink(missing_ok=True)


class PlantedDefects(unittest.TestCase):
    """Planted: the normal result is PASS (above); each mutated decoder must yield the named finding."""

    def test_non_canonical_encoding_accepted_is_a_mutation_finding(self):
        # Planted: the profile-1 x86 decoder stops refusing ModRM memory forms (mod != 3), so a
        # memory operand decodes like the register form; re-encoding yields mod == 3 and differs.
        original = verify._require

        def lenient(condition, reason):
            if reason == "MemoryOperandRefused":
                return
            original(condition, reason)

        with patch.object(verify, "_require", lenient):
            summary = fuzz.run(seed=11, iterations=400, selected=(X86_1,))
        rows = findings(summary, "mutation")
        self.assertTrue(rows, "the fuzzer must notice an accepted non-canonical encoding")
        self.assertTrue(all(row["problem"] == "accepted a non-canonical encoding" for row in rows), rows[:2])
        self.assertEqual(findings(summary, "crash"), [])

    def test_decoder_crash_is_a_crash_finding(self):
        # Planted: the profile-2 RV64I decoder indexes a table that lacks the entry.
        def broken(code):
            if len(code) % 4 == 0 and len(code) >= 8:
                raise KeyError(("planted", len(code)))
            return verify2._decode_rv64(code)

        with patch.object(verify2, "_decode_rv64", broken):
            summary = fuzz.run(seed=2, iterations=5, selected=(RV_2,))
        rows = findings(summary, "crash")
        self.assertTrue(rows)
        self.assertEqual(rows[0]["exception"], "KeyError")
        self.assertEqual(rows[0]["target"], RV_2)
        self.assertIn("code", rows[0])

    def test_profiles_disagreeing_is_a_cross_profile_finding(self):
        # Planted: profile 2 reads the R-type XOR signature as OR while profile 1 keeps XOR.
        table = dict(verify2._RV_REGISTER)
        table[(0, 4)] = "or"
        with patch.object(verify2, "_RV_REGISTER", table):
            summary = fuzz.run(seed=4, iterations=200, selected=(RV_1,))
        rows = findings(summary, "cross-profile")
        self.assertTrue(rows)
        self.assertEqual({row["problem"] for row in rows}, {"profiles decode the same bytes differently"})
        self.assertEqual({row["high"] for row in rows}, {RV_2, RV_3}, "profile 3 reuses the profile-2 decoder, so both diverge")

    def test_encoder_decoder_disagreement_is_a_round_trip_finding(self):
        # Planted: the profile-1 x86 decoder reports SHR for the SHL ModRM extension.
        original = verify._decode_x86

        def swapped(code):
            result = original(code)
            return [verify.Instruction("shr" if ins.op == "shl" else ins.op, ins.operands, ins.offset, ins.size)
                    for ins in result]

        with patch.object(verify, "_decode_x86", swapped):
            summary = fuzz.run(seed=6, iterations=120, selected=(X86_1,))
        rows = findings(summary, "round-trip")
        self.assertTrue(rows)
        self.assertEqual({row["problem"] for row in rows}, {"re-encoding differs"})

    def test_slow_decode_is_a_hang_finding(self):
        original = verify.decode

        def slow(target, code):
            time.sleep(fuzz.WALL_BUDGET_S + 0.05)
            return original(target, code)

        with patch.object(verify, "decode", slow):
            summary = fuzz.run(seed=8, iterations=1, selected=(RV_1,))
        self.assertTrue(findings(summary, "hang"))

    def test_findings_give_exit_one(self):
        table = dict(verify2._RV_REGISTER)
        table[(0, 4)] = "or"
        with patch.object(verify2, "_RV_REGISTER", table), redirect_stdout(io.StringIO()) as stdout:
            code = fuzz.main(["--seed", "4", "--iterations", "200", "--target", RV_1])
        self.assertEqual(code, 1)
        self.assertIn("FINDINGS", stdout.getvalue())


if __name__ == "__main__":
    unittest.main()
