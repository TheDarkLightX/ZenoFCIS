"""The checked kernel library rechecks every entry, and the check can fail."""

from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import shutil
import tempfile
import unittest

from machine_code import library, model, session, targets
from machine_code.run import main as cli

ROOT = Path(__file__).resolve().parents[1]
KERNELS = ROOT / "test-data/machine-code/kernels"


def main(argv):
    with redirect_stdout(io.StringIO()):
        return cli(argv)


def entry(name):
    return json.loads((KERNELS / (name + ".json")).read_text())


class KernelLibrary(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zeno-kernel-library-")
        self.root = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def copy(self, *names):
        directory = self.root / "library"
        directory.mkdir()
        for name in names:
            shutil.copyfile(KERNELS / (name + ".json"), directory / (name + ".json"))
        return directory

    def write(self, directory, value, name=None):
        (directory / ((name or value["name"]) + ".json")).write_bytes(model.canonical(value) + b"\n")

    def test_every_repository_kernel_rechecks_at_its_recorded_cost(self):
        report = library.check(KERNELS)
        self.assertEqual(report["status"], "checked", [r for r in report["kernels"] if r["status"] != "equivalent"])
        names = {row["name"] for row in report["kernels"]}
        self.assertGreaterEqual(len(names), 12)
        targets_seen = {row["target"] for row in report["kernels"]}
        self.assertEqual(targets_seen, {"x86_64-sysv-register/1", "x86_64-sysv-register/2",
                                        "rv64i-register/1", "rv64i-register/2"})
        self.assertEqual(report["authority"], "none")

    def test_a_wrong_kernel_fails_the_library(self):
        directory = self.copy("abs-x86", "add128-x86")
        wrong = entry("abs-x86")
        # Drop the CMOVS: NEG alone is wrong for every non-negative x except 0.
        code = targets.encode_program(wrong["problem"]["target"], [["mov", "rax", "rdi"], ["neg", "rax"]])
        wrong["code_hex"], wrong["cost"] = code.hex(), {"bytes": len(code), "instructions": 2}
        self.write(directory, wrong)
        report = library.check(directory)
        self.assertEqual(report["status"], "failed")
        rows = {row["name"]: row["status"] for row in report["kernels"]}
        self.assertEqual(rows, {"abs-x86": "counterexample", "add128-x86": "equivalent"})
        self.assertEqual(main(["library", "check", "--dir", str(directory)]), 1)

    def test_a_recorded_cost_that_differs_fails(self):
        directory = self.copy("smax-x86")
        value = entry("smax-x86")
        value["cost"] = {"bytes": value["cost"]["bytes"] - 1, "instructions": value["cost"]["instructions"]}
        self.write(directory, value)
        self.assertEqual(library.check(directory)["kernels"][0]["status"], "cost-differs")

    def test_misnamed_entries_stray_files_and_open_schemas_are_refused(self):
        directory = self.copy("umin-x86")
        self.write(directory, entry("umin-x86"), name="umin-renamed")
        (directory / "notes.txt").write_text("not an entry\n")
        extra = entry("smax-x86")
        extra["verdict"] = "equivalent"
        self.write(directory, extra)
        report = library.check(directory)
        self.assertEqual(report["status"], "failed")
        rows = {row["file"]: row for row in report["kernels"]}
        self.assertEqual(rows["umin-x86.json"]["status"], "equivalent")
        self.assertIn("differs from its file name", rows["umin-renamed.json"]["reason"])
        self.assertIn("non-entry file", rows["notes.txt"]["reason"])
        self.assertIn("closed version-1 schema", rows["smax-x86.json"]["reason"])

    def test_an_empty_library_does_not_pass(self):
        directory = self.root / "empty"
        directory.mkdir()
        self.assertEqual(library.check(directory)["status"], "failed")

    def emitted(self, name):
        value = entry(name)
        out = self.root / name
        out.mkdir()
        (out / "problem.json").write_bytes(model.canonical(value["problem"]))
        (out / "proposal.json").write_bytes(model.canonical({
            "schema": "zeno-fcis/machine-proposal/1", "code_hex": value["code_hex"],
            "provenance": {"kind": "human", "name": "library-test", "note": "repository entry"}}))
        session.open_session(out / "problem.json", out / "session")
        self.assertEqual(session.submit(out / "session", out / "proposal.json")["status"], "equivalent")
        self.assertEqual(session.emit(out / "session", out / "artifact")["status"], "emitted")
        return out / "artifact", value

    def test_add_reproduces_an_entry_and_never_overwrites(self):
        artifact, value = self.emitted("add128-x86")
        directory = self.root / "library"
        directory.mkdir()
        added = library.add(artifact, directory, value["name"], value["summary"], value["route"])
        self.assertEqual(added["status"], "added")
        self.assertEqual((directory / "add128-x86.json").read_bytes(), (KERNELS / "add128-x86.json").read_bytes())
        self.assertEqual(main(["library", "add", "--artifact", str(artifact), "--dir", str(directory),
                               "--name", value["name"], "--summary", value["summary"],
                               "--route", value["route"]]), 65)
        self.assertEqual((directory / "add128-x86.json").read_bytes(), (KERNELS / "add128-x86.json").read_bytes())

    def test_add_refuses_an_artifact_that_does_not_replay(self):
        artifact, value = self.emitted("sub128-x86")
        code = bytearray((artifact / "kernel.bin").read_bytes())
        code[0] ^= 1
        (artifact / "kernel.bin").chmod(0o600)
        (artifact / "kernel.bin").write_bytes(bytes(code))
        directory = self.root / "library"
        directory.mkdir()
        with self.assertRaisesRegex(ValueError, "artifact bytes changed"):
            library.add(artifact, directory, value["name"], value["summary"], value["route"])
        self.assertEqual(list(directory.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
