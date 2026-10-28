"""Focused controls for the finite eligibility synthesis preparation boundary."""
from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import behavior2
import program_synthesis as synthesis

SOURCE = b"""zal 2;
machine repair_desk;
states awaiting, queued, repairing, ready, collected;
events drop_off, begin, complete, pick_up, reopen;
context technician;
var attempts in 0..=2 initially 0;
initial awaiting;
step atomic; frame control_and_variables;
default reject not_enabled unchanged;
rule check_in: awaiting + drop_off [!technician] -> accept queued;
rule start_repair: queued + begin [technician & attempts < 2] -> accept repairing, attempts += 1;
rule finish_repair: repairing + complete [technician] -> accept ready;
rule collect_item: ready + pick_up [!technician] -> accept collected;
rule request_rework: ready + reopen [!technician & attempts < 2] -> accept queued;
require technician_steps: ((event == begin | event == complete) & accepted) => technician;
require customer_steps: ((event == drop_off | event == pick_up | event == reopen) & accepted) => !technician;
require count_starts: (event == begin & accepted) => attempts' == attempts + 1;
require other_steps_preserve_count: (!(event == begin) & accepted) => attempts' == attempts;
"""


def binding(source=SOURCE):
    model = behavior2.parse(source.decode("utf-8"))
    return json.dumps({"format": "zal/app-binding/1", "model_revision": model.revision,
                       "observations": [{"name": "technician", "kind": "fact",
                                         "source": "server-context", "role": "authorization",
                                         "label": "Technician", "unit": "unitless"}],
                       "ui": {}}).encode()


def evaluate(graph, values):
    """Independent test evaluator for the emitted closed, eager graph subset."""
    results = []
    for node in graph["nodes"]:
        op, *args = node
        op = op.lower()
        if op == "input":
            value = values[args[0]]
        elif op in {"int", "bool"}:
            value = int(args[0])
        elif op == "not":
            value = int(not results[args[0]])
        elif op == "eq":
            value = int(results[args[0]] == results[args[1]])
        elif op == "and":
            value = int(bool(results[args[0]]) and bool(results[args[1]]))
        elif op == "select":
            value = results[args[1] if results[args[0]] else args[2]]
        else:
            raise AssertionError(op)
        results.append(value)
    return [results[index] for index in graph["roots"]]


class ProgramSynthesisTests(unittest.TestCase):
    def test_complete_projection_and_independent_graph_encodings(self):
        artifacts = synthesis.prepare(SOURCE, binding(), "begin")
        table, problem, reference, mapping = (artifacts[name] for name in
            ("table.json", "problem.json", "reference-program.json", "mapping.json"))
        rows = table["rows"]
        self.assertEqual(len(rows), 30)
        self.assertEqual([r["input"] for r in rows if r["eligible"]], [[152, 0, 1], [152, 1, 1]])
        self.assertEqual(mapping["event_id"], 120)
        self.assertEqual(mapping["states"]["queued"], 152)
        self.assertEqual(mapping["fact"]["source"], "server-context")
        self.assertEqual((mapping["accepted_tuples"], mapping["rejected_tuples"]), (2, 28))
        self.assertEqual(mapping["grammar"]["candidate_space"], 5832)
        self.assertEqual(mapping["grammar"]["max_assignments"], 6000)
        self.assertEqual(mapping["grammar"]["max_steps"], 8_000_000)
        reference_sketch = artifacts["reference-problem.json"]["sketch"]
        lookup = {"nodes": [node["alternatives"][0] if isinstance(node, dict) else node
                            for node in reference_sketch["nodes"]], "roots": reference_sketch["roots"]}
        for row in rows:
            expected = int(row["eligible"])
            self.assertEqual(evaluate(reference, row["input"]), [expected])
            self.assertEqual(evaluate(lookup, row["input"]), [expected])
            for output in (0, 1):
                self.assertEqual(evaluate(problem["contract"], [*row["input"], output]), [int(output == expected)])
        self.assertLessEqual(len(reference["nodes"]), 256)
        self.assertLessEqual(len(problem["contract"]["nodes"]), 256)

    def test_binding_and_unsupported_models_refuse(self):
        original = json.loads(binding())
        for change in (
            lambda d: d.update(model_revision="0" * 64),
            lambda d: d["observations"][0].update(source="command"),
            lambda d: d["observations"][0].update(role="observation"),
            lambda d: d["observations"][0].update(name="other"),
        ):
            doc = json.loads(binding())
            change(doc)
            with self.assertRaises(behavior2.Refusal):
                synthesis.prepare(SOURCE, json.dumps(doc).encode(), "begin")
        with self.assertRaises(behavior2.Refusal):
            synthesis.prepare(SOURCE, binding(), "unknown")
        duplicate = binding().replace(b'"format":', b'"format":"duplicate","format":', 1)
        with self.assertRaisesRegex(behavior2.Refusal, "duplicate"):
            synthesis.prepare(SOURCE, duplicate, "begin")
        changed = SOURCE.replace(b"var attempts", b"input amount in 0..=2;\nvar attempts")
        with self.assertRaisesRegex(behavior2.Refusal, "only one"):
            synthesis.prepare(changed, binding(changed), "begin")
        self.assertEqual(original["observations"][0]["source"], "server-context")

    def test_domain_budget_precedes_model_check(self):
        large = SOURCE.replace(b"attempts in 0..=2", b"attempts in 0..=20")
        with (patch.object(behavior2, "check", side_effect=AssertionError("must not run")),
              self.assertRaisesRegex(behavior2.Refusal, "exceeds 32")):
            synthesis.prepare(large, binding(large), "begin")

    def test_model_counterexample_does_not_become_a_relation(self):
        trapping = SOURCE.replace(b"technician & attempts < 2", b"technician")
        with self.assertRaisesRegex(behavior2.Refusal, "finite check did not pass"):
            synthesis.prepare(trapping, binding(trapping), "begin")

    def test_vacuous_fixed_event_projections_refuse(self):
        never = SOURCE.replace(b"technician & attempts < 2", b"technician & attempts < 0")
        with self.assertRaisesRegex(behavior2.Refusal, "both Accept and Reject"):
            synthesis.prepare(never, binding(never), "begin")
        header = SOURCE.split(b"rule check_in:")[0]
        rules = b"".join((f"rule all_{state}: {state} + begin [technician | !technician] "
                          f"-> accept {state};\n").encode()
                         for state in ("awaiting", "queued", "repairing", "ready", "collected"))
        always = header + rules
        with self.assertRaisesRegex(behavior2.Refusal, "both Accept and Reject"):
            synthesis.prepare(always, binding(always), "begin")

    def test_artifacts_are_new_and_non_overwriting(self):
        artifacts = synthesis.prepare(SOURCE, binding(), "begin")
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "prepared"
            hashes = synthesis.write_new(artifacts, output)
            self.assertEqual(set(hashes), set(artifacts))
            before = (output / "problem.json").read_bytes()
            with self.assertRaises(FileExistsError):
                synthesis.write_new(artifacts, output)
            self.assertEqual((output / "problem.json").read_bytes(), before)

    def test_executed_symmetry_dependency_source_is_pinned(self):
        modules = ("adt", "behavior", "behavior2", "constructs", "diagnostics", "effects",
                   "symmetry2", "typed2", "program_synthesis")
        script = """import json, sys
sys.path.insert(0, sys.argv[1])
import program_synthesis
source, binding = json.load(sys.stdin)
print(json.dumps(program_synthesis.prepare(source.encode(), binding.encode(), "begin")))
"""
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for name in modules:
                (directory / f"{name}.py").write_bytes(
                    (Path(synthesis.__file__).parent / f"{name}.py").read_bytes())

            def prepare_fresh():
                process = subprocess.run(
                    [sys.executable, "-I", "-B", "-c", script, str(directory)],
                    input=json.dumps([SOURCE.decode(), binding().decode()]),
                    capture_output=True, text=True, check=True, timeout=10)
                return json.loads(process.stdout)

            before = prepare_fresh()
            dependency = directory / "symmetry2.py"
            dependency.write_bytes(dependency.read_bytes() + b"\n# isolated dependency source change\n")
            after = prepare_fresh()
            pins_before = before["mapping.json"]["interpreter_sources"]
            pins_after = after["mapping.json"]["interpreter_sources"]
            self.assertEqual(set(pins_before), set(modules) - {"program_synthesis"})
            self.assertEqual(pins_after["symmetry2"], hashlib.sha256(dependency.read_bytes()).hexdigest())
            self.assertNotEqual(pins_before["symmetry2"], pins_after["symmetry2"])
            self.assertEqual({**pins_before, "symmetry2": pins_after["symmetry2"]}, pins_after)
            self.assertEqual({**before["mapping.json"], "interpreter_sources": pins_after}, after["mapping.json"])
            for name in before.keys() - {"mapping.json"}:
                self.assertEqual(before[name], after[name], name)


if __name__ == "__main__":
    unittest.main()
