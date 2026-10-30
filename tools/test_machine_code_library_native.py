"""Native execution and paired timing of the checked kernel library on an actual Linux x86-64 host.

Each x86 entry's ELF object is rebuilt from its bytes, linked with a generated C runner and compared word by word
with the problem's integer reference. RV64I entries are reported as not run. Timing is measured on this machine and
load only; the test checks the table's shape and scope, never a speed.
"""

from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import platform
import shutil
import tempfile
import unittest
from unittest.mock import patch

from machine_code import library, model, native, session, targets
from machine_code.run import main as cli

ROOT = Path(__file__).resolve().parents[1]
KERNELS = ROOT / "test-data/machine-code/kernels"


def main(argv):
    with redirect_stdout(io.StringIO()) as output:
        code = cli(argv)
    return code, json.loads(output.getvalue())


def entry(name):
    return json.loads((KERNELS / (name + ".json")).read_text())


def rewritten(name, instructions):
    """The named entry with different bytes and their true cost: a planted wrong kernel."""
    value = entry(name)
    code = targets.encode_program(value["problem"]["target"], instructions)
    value["code_hex"], value["cost"] = code.hex(), {"bytes": len(code), "instructions": len(instructions)}
    return value


@unittest.skipUnless(platform.system() == "Linux" and platform.machine() == "x86_64",
                     "actual Linux x86-64 native profile required")
class LibraryNative(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if shutil.which("gcc", path="/usr/bin:/bin") is None:
            raise RuntimeError("required GNU tool is missing: gcc")

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zeno-library-native-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def library(self, *names, extra=()):
        directory = self.root / "library"
        directory.mkdir()
        for name in names:
            shutil.copyfile(KERNELS / (name + ".json"), directory / (name + ".json"))
        for value in extra:
            (directory / (value["name"] + ".json")).write_bytes(model.canonical(value) + b"\n")
        return directory

    def test_every_x86_entry_agrees_natively_and_rv64i_is_not_run(self):
        report = library.check(KERNELS, native=True)
        self.assertEqual(report["status"], "checked", report["native_summary"])
        rows = {row["name"]: row for row in report["kernels"]}
        for name, row in rows.items():
            with self.subTest(kernel=name):
                if row["target"].startswith("rv64i"):
                    self.assertEqual(row["native"], {"status": "not-run", "reason": "needs RV64I execution"})
                else:
                    self.assertEqual(row["native"]["status"], "agreed", row["native"])
                    self.assertEqual(row["native"]["random_vectors"], 4096)
        self.assertEqual(rows["add128-x86"]["native"]["edge_vectors"], 8 ** 4)
        self.assertEqual(rows["abs-x86"]["native"]["edge_vectors"], 8)
        summary = report["native_summary"]
        self.assertGreaterEqual(summary["agreed"], 10)
        self.assertEqual((summary["mismatch"], summary["error"]), (0, 0))
        self.assertEqual(set(summary["not_run"]), {"add128-rv64i.json", "add64-rv64i.json"})
        self.assertEqual(summary["vectors"], sum(r["native"].get("vectors", 0) for r in report["kernels"]))
        self.assertIn("RV64I code is not run", report["scope"])
        self.assertEqual(main(["library", "check", "--native", "--dir", str(KERNELS)])[0], 0)

    def test_a_planted_wrong_entry_fails_natively_as_well(self):
        # Dropping the CMOVS leaves NEG, which is wrong for every positive x.
        wrong = rewritten("abs-x86", [["mov", "rax", "rdi"], ["neg", "rax"]])
        directory = self.library("umin-x86", "add128-rv64i", extra=[wrong])
        report = library.check(directory, native=True)
        self.assertEqual(report["status"], "failed")
        rows = {row["name"]: row for row in report["kernels"]}
        self.assertEqual(rows["abs-x86"]["status"], "counterexample")
        self.assertEqual(rows["abs-x86"]["native"]["status"], "mismatch")
        first = rows["abs-x86"]["native"]["first_mismatch"]
        x = int(first["arguments"][0])
        self.assertEqual(int(first["actual"][0]), -x % (1 << 64))
        self.assertNotEqual(first["expected"], first["actual"])
        self.assertEqual(rows["umin-x86"]["native"]["status"], "agreed")
        self.assertEqual(report["native_summary"]["mismatch"], 1)
        code, printed = main(["library", "check", "--native", "--dir", str(directory)])
        self.assertEqual((code, printed["status"]), (1, "failed"))

    def test_native_execution_fails_a_wrong_entry_even_when_the_model_check_passes(self):
        # A wrong second result: ADD instead of ADC loses the carry into the high word.
        wrong = rewritten("add128-x86", [["mov", "rax", "rdi"], ["add", "rax", "rdx"], ["mov", "rdx", "rsi"],
                                         ["add", "rdx", "rcx"]])
        directory = self.library("smax-x86", extra=[wrong])
        values = [entry("smax-x86"), wrong]

        class Accepting:
            @staticmethod
            def check(problem, code, timeout_ms=1000):
                value = next(v for v in values if v["code_hex"] == code.hex())
                return {"status": "equivalent", "cost": value["cost"]}

        with patch.object(session, "_checker", lambda problem: Accepting):
            report = library.check(directory, native=True)
        rows = {row["name"]: row for row in report["kernels"]}
        self.assertEqual(rows["add128-x86"]["status"], "equivalent", "the stub model check accepted it")
        self.assertEqual(rows["add128-x86"]["native"]["status"], "mismatch")
        first = rows["add128-x86"]["native"]["first_mismatch"]
        self.assertEqual(first["expected"][0], first["actual"][0])
        self.assertEqual((int(first["expected"][1]) - int(first["actual"][1])) % (1 << 64), 1)
        self.assertEqual(report["status"], "failed")

    def test_a_host_without_native_execution_is_inconclusive_never_a_pass(self):
        directory = self.library("umin-x86", "add64-rv64i")
        with patch.object(native, "unsupported_host", return_value="needs a Linux x86-64 host"):
            report = library.check(directory, native=True)
            code, printed = main(["library", "check", "--native", "--dir", str(directory)])
        self.assertEqual(report["status"], "inconclusive")
        rows = {row["name"]: row["native"] for row in report["kernels"]}
        self.assertEqual(rows["umin-x86"], {"status": "not-run", "reason": "needs a Linux x86-64 host"})
        self.assertEqual(rows["add64-rv64i"], {"status": "not-run", "reason": "needs RV64I execution"})
        self.assertEqual((code, printed["status"]), (2, "inconclusive"))
        self.assertNotIn("native_summary", library.check(directory))

    def test_bench_writes_one_scoped_table_and_times_only_rechecked_x86_entries(self):
        wrong = rewritten("abs-x86", [["mov", "rax", "rdi"], ["neg", "rax"]])
        directory = self.library("add64-x86", "add128-x86", "add128-rv64i", extra=[wrong])
        report = library.bench(directory, self.root / "timing", iterations=20_000, repeats=3)
        self.assertEqual(report["status"], "failed")
        rows = {row["kernel"]: row for row in report["kernels"]}
        self.assertEqual({k: r["status"] for k, r in rows.items()},
                         {"abs-x86": "not-timed", "add64-x86": "measured", "add128-x86": "measured",
                          "add128-rv64i": "not-run"})
        self.assertEqual(rows["abs-x86"]["reason"], "did not recheck: counterexample")
        self.assertTrue(rows["add128-x86"]["baseline_c"].startswith("(pair){"))
        self.assertTrue(all(value > 0 for value in rows["add64-x86"]["ticks_per_call"].values()))
        table = (self.root / "timing/bench.md").read_text()
        self.assertEqual(table, report["table"])
        self.assertIn("This machine and load only; not a portable speedup claim.", table)
        self.assertIn("| add128-rv64i | rv64i-register/2 | 20 | not-run: needs RV64I execution |", table)
        self.assertEqual(json.loads((self.root / "timing/bench.json").read_text())["kernels"], report["kernels"])
        environment = report["environment"]
        self.assertTrue(environment["cpu"] and environment["compiler"].startswith("gcc"))
        self.assertEqual(len(environment["load_average_start"]), 3)
        self.assertFalse((self.root / "timing/abs-x86").exists(), "a kernel that does not recheck is never timed")

    def test_bench_cli_exit_codes(self):
        directory = self.library("umin-x86")
        code, printed = main(["library", "bench", "--dir", str(directory), "--out", str(self.root / "timed"),
                              "--iterations", "20000", "--repeats", "3"])
        self.assertEqual((code, printed["status"]), (0, "measured"))
        code, printed = main(["library", "bench", "--dir", str(directory), "--out", str(self.root / "other"),
                              "--iterations", "10", "--repeats", "3"])
        self.assertEqual((code, printed["status"]), (65, "refused"))
        self.assertFalse((self.root / "other").exists())


if __name__ == "__main__":
    unittest.main()
