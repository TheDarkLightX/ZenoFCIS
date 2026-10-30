"""Profile-3 memory kernels through proposal sessions, search, emission, replay and the kernel library.

No memory kernel is executed natively here; `library check --native` reports them as not run.
"""

from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import tempfile
import unittest

from machine_code import library, model, profile3, session, targets
from machine_code.run import main

ROOT = Path(__file__).resolve().parents[1]
MEMORY = ROOT / "test-data/machine-code/memory-kernels"
X86 = profile3.TARGETS[0]
NAMES = ("swap-words-x86", "load-add-store-x86", "copy2-sum-x86")


def entry(name):
    return model.read_json(MEMORY / (name + ".json"))


class Profile3LoopTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="zeno-machine-loop3-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def opened(self, problem, name="session"):
        path = self.root / (name + ".json")
        path.write_bytes(model.canonical(problem))
        session.open_session(path, self.root / name)
        return self.root / name

    def proposal(self, instructions, name="proposal.json"):
        path = self.root / name
        path.write_bytes(model.canonical({"schema": "zeno-fcis/machine-proposal/1",
                         "code_hex": targets.encode_program(X86, instructions).hex(),
                         "provenance": {"kind": "human", "name": "integration-test", "note": "explicit test proposal"}}))
        return path

    def test_search_finds_each_library_kernel_and_it_emits_replays_and_adds(self):
        added = self.root / "library"
        added.mkdir()
        for name in NAMES:
            with self.subTest(kernel=name):
                value = entry(name)
                directory = self.opened(value["problem"], name)
                found = session.search(directory, deadline_ms=30_000, max_checks=64)
                self.assertEqual(found["status"], "checked-candidate", found)
                self.assertEqual(found["cost"], value["cost"])
                artifact = self.root / (name + "-artifact")
                self.assertEqual(session.emit(directory, artifact)["status"], "emitted")
                replayed = session.replay_artifact(artifact)
                self.assertEqual(replayed["status"], "replayed")
                self.assertEqual(replayed["verification"]["cost"], value["cost"])
                library.add(artifact, added, name, value["summary"], "search")
                self.assertEqual((added / (name + ".json")).read_bytes(), (MEMORY / (name + ".json")).read_bytes())
        report = library.check(added, native=True)
        self.assertEqual(report["status"], "checked")
        self.assertEqual(report["native_summary"]["not_run"],
                         {name + ".json": "memory kernels are not executed natively" for name in NAMES})

    def test_proposals_are_refuted_refused_or_accepted_by_the_checker(self):
        value = entry("swap-words-x86")
        directory = self.opened(value["problem"])
        wrong = self.proposal([["load", "rcx", "rdi", 8], ["store", "rdi", 0, "rcx"], ["load", "rax", "rdi", 0],
                               ["store", "rdi", 8, "rax"]], "wrong.json")
        result = session.submit(directory, wrong)
        self.assertEqual(result["status"], "counterexample")
        witness = session._witness(value["problem"], result["verification"])
        right = bytes.fromhex(value["code_hex"])
        self.assertTrue(session._agrees(value["problem"], right, witness))
        self.assertFalse(session._agrees(value["problem"], targets.encode_program(X86, [
            ["load", "rcx", "rdi", 8], ["store", "rdi", 0, "rcx"], ["load", "rax", "rdi", 0],
            ["store", "rdi", 8, "rax"]]), witness))
        packet = session.packet(directory)
        self.assertEqual(packet["feedback"][0]["verification"]["status"], "counterexample")
        accepted = session.submit(directory, self.proposal([["load", "rax", "rdi", 0], ["load", "rcx", "rdi", 8],
                                                            ["store", "rdi", 0, "rcx"], ["store", "rdi", 8, "rax"]]))
        self.assertEqual(accepted["status"], "equivalent")
        self.assertEqual(session.resume(directory)["status"], "replayed")

    def test_every_seed_separates_a_dropped_store_and_admits_the_right_kernel(self):
        value = entry("copy2-sum-x86")
        seeds = session._seeds(value["problem"])
        right = bytes.fromhex(value["code_hex"])
        self.assertTrue(all(session._agrees(value["problem"], right, seed) for seed in seeds))
        dropped = targets.encode_program(X86, [["load", "rax", "rdi", 0], ["load", "rcx", "rdi", 8],
                                               ["store", "rsi", 0, "rax"], ["add", "rax", "rcx"]])
        self.assertFalse(all(session._agrees(value["problem"], dropped, seed) for seed in seeds))
        read_only_store = targets.encode_program(X86, [["load", "rax", "rdi", 0], ["store", "rdi", 0, "rax"]])
        self.assertFalse(any(session._agrees(value["problem"], read_only_store, seed) for seed in seeds))

    def test_cli_open_and_search_dispatch_on_the_schema(self):
        problem = self.root / "problem.json"
        problem.write_bytes(model.canonical(entry("load-add-store-x86")["problem"]))
        output = io.StringIO()
        with redirect_stdout(output):
            self.assertEqual(main(["open", "--problem", str(problem), "--session", str(self.root / "cli")]), 0)
            self.assertEqual(main(["search", "--session", str(self.root / "cli"), "--deadline-ms", "20000"]), 0)
        found = json.loads(output.getvalue().splitlines()[-1])
        self.assertEqual((found["status"], found["cost"]), ("checked-candidate", {"bytes": 10, "instructions": 3}))


if __name__ == "__main__":
    unittest.main()
