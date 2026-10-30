"""Native execution of the checked x86 memory kernels on this processor, on real aligned and disjoint buffers.

Positive runs only: a kernel runs only after profile-3 admission, so every access is an aligned word inside its
region. Results and every region word are compared with the problem's integer reference, and read-only regions
must come back unchanged. RV64I entries are reported as not run.
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
MEMORY = ROOT / "test-data/machine-code/memory-kernels"
X86 = "x86_64-sysv-memory/3"


def main(argv):
    with redirect_stdout(io.StringIO()) as output:
        code = cli(argv)
    return code, json.loads(output.getvalue())


def entry(name):
    return json.loads((MEMORY / (name + ".json")).read_text())


def rewritten(name, instructions):
    """The named entry with different bytes and their true cost: a planted wrong kernel."""
    value = entry(name)
    code = targets.encode_program(value["problem"]["target"], instructions)
    value["code_hex"], value["cost"] = code.hex(), {"bytes": len(code), "instructions": len(instructions)}
    return value


class Accepting:
    """A stub checker that accepts every entry, so only native execution can catch a wrong one."""
    values = []

    @classmethod
    def check(cls, problem, code, timeout_ms=1000):
        value = next(v for v in cls.values if v["code_hex"] == code.hex())
        return {"status": "equivalent", "cost": value["cost"]}


@unittest.skipUnless(platform.system() == "Linux" and platform.machine() == "x86_64",
                     "actual Linux x86-64 native profile required")
class MemoryNative(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if shutil.which("gcc", path="/usr/bin:/bin") is None:
            raise RuntimeError("required GNU tool is missing: gcc")

    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="zeno-memory-native-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def library(self, *values):
        directory = self.root / "library"
        directory.mkdir()
        for value in values:
            (directory / (value["name"] + ".json")).write_bytes(model.canonical(value) + b"\n")
        return directory

    def test_every_x86_memory_entry_agrees_natively_and_rv64i_is_not_run(self):
        report = library.check(MEMORY, native=True)
        self.assertEqual(report["status"], "checked", report["native_summary"])
        rows = {row["name"]: row for row in report["kernels"]}
        for name, row in rows.items():
            with self.subTest(kernel=name):
                if row["target"].startswith("rv64i"):
                    self.assertEqual(row["native"], {"status": "not-run", "reason": "needs RV64I execution"})
                    continue
                self.assertEqual(row["native"]["status"], "agreed", row["native"])
                self.assertEqual((row["native"]["seed_vectors"], row["native"]["edge_vectors"],
                                  row["native"]["random_vectors"]), (6, 1024, 4096))
                self.assertGreaterEqual(row["native"]["random_vectors"], 1000)
        self.assertEqual(rows["copy2-sum-x86"]["native"]["read_only_regions"], ["src"])
        summary = report["native_summary"]
        self.assertEqual((summary["agreed"], summary["mismatch"], summary["error"]), (3, 0, 0))
        self.assertEqual(set(summary["not_run"]),
                         {"swap-words-rv64i.json", "load-add-store-rv64i.json", "copy2-sum-rv64i.json"})
        self.assertIn("every region word", report["scope"])
        self.assertEqual(main(["library", "check", "--native", "--dir", str(MEMORY)])[0], 0)

    def test_a_dropped_store_fails_natively_even_when_the_model_check_passes(self):
        # The swap without its second store leaves word 1 unchanged.
        wrong = rewritten("swap-words-x86", [["load", "rax", "rdi", 0], ["load", "rcx", "rdi", 8],
                                             ["store", "rdi", 0, "rcx"]])
        right = entry("load-add-store-x86")
        Accepting.values = [wrong, right]
        with patch.object(session, "_checker", lambda problem: Accepting):
            report = library.check(self.library(wrong, right), native=True)
        rows = {row["name"]: row for row in report["kernels"]}
        self.assertEqual(rows["swap-words-x86"]["status"], "equivalent", "the stub model check accepted it")
        self.assertEqual(rows["swap-words-x86"]["native"]["status"], "mismatch")
        first = rows["swap-words-x86"]["native"]["first_mismatch"]
        initial = first["initial_regions"]["p"]
        self.assertEqual(first["expected_regions"]["p"], [initial[1], initial[0]])
        self.assertEqual(first["actual_regions"]["p"], [initial[1], initial[1]])
        self.assertEqual(rows["load-add-store-x86"]["native"]["status"], "agreed")
        self.assertEqual(report["status"], "failed")

    def test_a_wrong_result_beside_correct_region_words_fails_natively(self):
        # The copy is right but the returned sum is word 0 only.
        wrong = rewritten("copy2-sum-x86", [["load", "rax", "rdi", 0], ["store", "rsi", 0, "rax"],
                                            ["load", "rcx", "rdi", 8], ["store", "rsi", 8, "rcx"]])
        Accepting.values = [wrong]
        with patch.object(session, "_checker", lambda problem: Accepting):
            report = library.check(self.library(wrong), native=True)
        first = report["kernels"][0]["native"]["first_mismatch"]
        self.assertEqual(first["actual"], [first["initial_regions"]["src"][0]])
        self.assertEqual(first["actual_regions"], first["expected_regions"])
        self.assertEqual(first["actual_regions"]["src"], first["initial_regions"]["src"], "read-only unchanged")

    def test_an_inadmissible_memory_kernel_is_never_run(self):
        # A store to the read-only source: refused by admission, so it never touches a buffer.
        wrong = rewritten("copy2-sum-x86", [["load", "rax", "rdi", 0], ["store", "rdi", 0, "rax"]])
        Accepting.values = [wrong]
        with patch.object(session, "_checker", lambda problem: Accepting):
            report = library.check(self.library(wrong), native=True)
        self.assertEqual(report["kernels"][0]["native"],
                         {"status": "not-run", "reason": "memory access not admitted: StoreToReadOnlyRegion"})

    def test_a_host_without_native_execution_is_inconclusive_never_a_pass(self):
        directory = self.library(entry("swap-words-x86"), entry("swap-words-rv64i"))
        with patch.object(native, "unsupported_host", return_value="needs a Linux x86-64 host"):
            report = library.check(directory, native=True)
        self.assertEqual(report["status"], "inconclusive")
        rows = {row["name"]: row["native"] for row in report["kernels"]}
        self.assertEqual(rows["swap-words-x86"], {"status": "not-run", "reason": "needs a Linux x86-64 host"})
        self.assertEqual(rows["swap-words-rv64i"], {"status": "not-run", "reason": "needs RV64I execution"})

    def test_regions_are_separate_aligned_static_buffers(self):
        runner = native._memory_runner(entry("copy2-sum-x86")["problem"])
        self.assertIn("static uint64_t region0[2] __attribute__((aligned(64)));", runner)
        self.assertIn("static uint64_t region1[2] __attribute__((aligned(64)));", runner)
        self.assertIn("zeno_kernel((uint64_t)(uintptr_t)region0, (uint64_t)(uintptr_t)region1)", runner)
        runner = native._memory_runner(entry("load-add-store-x86")["problem"])
        self.assertIn("zeno_kernel((uint64_t)(uintptr_t)region0, a[0])", runner)

    def test_memory_kernels_are_not_timed(self):
        report = library.bench(self.library(entry("swap-words-x86")), self.root / "timing", iterations=20_000,
                               repeats=3)
        self.assertEqual(report["kernels"][0]["status"], "not-run")
        self.assertEqual(report["kernels"][0]["reason"], "memory kernels are not timed")


if __name__ == "__main__":
    unittest.main()
