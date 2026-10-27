"""Every example rung preserves its canonical meaning and declared context.

Finite tables and temporal fixture words test actual meanings, in addition to
parse-back. Unbounded statements retain their proof obligations; finite fixture
replays below are explicitly separate finite restrictions, not unbounded proofs.
"""
import copy
from itertools import product
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import ladder as l
import ladder_tools as tools
from test_ladder import small_data

HERE = Path(__file__).resolve().parent


def examples():
    for path in sorted((HERE / "examples").rglob("*.json")):
        data = json.loads(path.read_text())
        if isinstance(data, dict) and "formula" in data and "theory" in data:
            yield path, l.Context.from_data(data)


class PreservationTests(unittest.TestCase):
    def test_every_example_rung_derives_from_its_canonical_formula(self):
        counts = {"statements": 0, "rungs": 0, "finite_table_evaluations": 0}
        for path, ctx in examples():
            canonical = l.parse(ctx.data["formula"], "typed", ctx)
            expected = l.normal(canonical, ctx)
            counts["statements"] += 1
            for rung in l.RUNGS:
                with self.subTest(example=path.name, rung=rung):
                    text = l.render(canonical, rung, ctx)
                    parsed = l.parse(text, rung, ctx)
                    self.assertEqual(l.normal(parsed, ctx), expected)
                    self.assertEqual(l.render(parsed, rung, ctx), text)
                    self.assertIn(ctx.data["theory"], l.ladder_text(canonical, ctx, rung))
                    counts["rungs"] += 1
                    if ctx.data["theory"] != l.TEMPORAL_THEORY:
                        for fixture in ctx.data["fixtures"]:
                            truth = l.evaluate(canonical, ctx, fixture)
                            self.assertIs(truth, fixture["expected"], fixture["id"])
                            self.assertIs(l.evaluate(parsed, ctx, fixture), truth, fixture["id"])
                            counts["finite_table_evaluations"] += 1
        self.assertGreaterEqual(counts["statements"], 9)
        print("ALL EXAMPLE RUNGS: " + json.dumps(counts, sort_keys=True))

    def test_primed_boolean_definitions_keep_after_state_meaning(self):
        data = small_data()
        data["definitions"] += [
            {"name": "Low", "params": [{"name": "z", "domain": "D"}],
             "body": "f(z)<1", "clause": "{0} is low"},
            {"name": "Stable", "params": [{"name": "z", "domain": "D"}],
             "body": "Low(z) and P(z)", "clause": "{0} is stable"}]
        ctx = l.Context.from_data(data)
        sources = ["forall x in D: Low'(x)", "not Low'(0)", "Low'(0) and P(1)",
                   "not Stable'(0)", "#{x in D: Low'(x)} <= 1", "Low'(0) => Low(1)"]
        checks = 0
        for source in sources:
            tree = l.parse(source, "typed", ctx)
            for rung in l.RUNGS:
                text = l.render(tree, rung, ctx)
                got = l.parse(text, rung, ctx)
                self.assertEqual(l.normal(got, ctx), l.normal(tree, ctx), (source, rung, text))
                for before0, before1, after0, after1, selected in product((0, 1), repeat=5):
                    fixture = {"domains": {"D": [0, 1]}, "symbols": {
                        "f": {"rows": [[[0], before0], [[1], before1]]},
                        "f'": {"rows": [[[0], after0], [[1], after1]]},
                        "P": {"rows": [], "default": bool(selected)}}}
                    self.assertIs(l.evaluate(got, ctx, fixture), l.evaluate(tree, ctx, fixture),
                                  (source, rung, text, fixture))
                    checks += 1
        tree = l.parse("Low'(0)", "typed", ctx)
        self.assertIn("after", l.legend_text(tree, ctx))
        # Explicit counterexample to erasing the prime: before low, after high.
        witness = {"domains": {"D": [0]}, "symbols": {
            "f": {"rows": [[[0], 0]]}, "f'": {"rows": [[[0], 1]]}}}
        self.assertIs(l.evaluate(tree, ctx, witness), False)
        self.assertIs(l.evaluate(l.parse("Low(0)", "typed", ctx), ctx, witness), True)
        print(f"PRIMED DEFINITIONS: {checks} finite rung/valuation comparisons; before=0, after=1 distinguishes the defect")

    def test_primitive_state_predicates_are_refused_explicitly(self):
        data = small_data()
        data["symbols"]["P"]["state"] = True
        with self.assertRaisesRegex(l.Refusal, "state-predicate-needs-integer-definition"):
            l.Context.from_data(data)

    def test_file_and_tool_comparison_bind_the_same_declared_scope(self):
        base = small_data()
        base["formula"] = "P(0)"
        changes = [lambda d: d["symbols"]["P"].update(clause="{0} is rejected"),
                   lambda d: d["domains"]["D"].update(values=[0, 1]),
                   lambda d: d.update(kind="invariant"),
                   lambda d: d.update(scope="reachable steps only"),
                   lambda d: d["definitions"][0].update(body="f(z)+2")]
        with tempfile.TemporaryDirectory() as directory:
            left, right = (Path(directory) / (name + ".json") for name in ("left", "right"))
            left.write_text(json.dumps(base))
            for change in changes:
                data = copy.deepcopy(base)
                change(data)
                right.write_text(json.dumps(data))
                self.assertNotEqual(l.meaning_file(left), l.meaning_file(right))
                with patch.dict(os.environ, {"ZAL_LADDER_DIR": directory}):
                    compared = tools.same("project:left", tools.revision_of(base),
                                          other_statement="project:right", other_revision=tools.revision_of(data))
                self.assertFalse(compared["same"])
                for command in (["same", str(left), str(right)], ["find", str(left), "--corpus", directory]):
                    result = subprocess.run([sys.executable, "-I", str(HERE / "ladder.py"), *command],
                                            capture_output=True, text=True, timeout=30)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertNotIn("same meaning", result.stdout)
                    if command[0] == "find":
                        self.assertNotIn(str(right), result.stdout)
            # IDs and fixture annotations do not change the declared statement meaning.
            data = copy.deepcopy(base)
            data.update(id="another_name", fixtures=[])
            right.write_text(json.dumps(data))
            self.assertEqual(l.meaning_file(left), l.meaning_file(right))


@unittest.skipUnless(os.environ.get("ZAL_TEMPORAL_CLI"), "set ZAL_TEMPORAL_CLI for native fixture evaluation")
class NativePreservationTests(unittest.TestCase):
    def test_every_temporal_rung_has_the_same_finite_fixture_verdict(self):
        from test_ladder_temporal import case, native
        requests, expected = [], []
        restricted = []
        for path, declared in examples():
            if declared.data["theory"] != l.TEMPORAL_THEORY:
                continue
            data = l.plain_data(declared.data)
            if data["trace"]["mode"] == "unbounded":
                # Match the explicitly illustrative finite fixtures already supplied
                # with this example. Never report them as a verdict on infinite runs.
                data["trace"].update(mode="finite", horizon=4)
                restricted.append(path.name)
            ctx = l.Context.from_data(data)
            canonical = l.parse(ctx.data["formula"], "typed", ctx)
            for rung in l.RUNGS:
                parsed = l.parse(l.render(canonical, rung, ctx), rung, ctx)
                for fixture in ctx.data["fixtures"]:
                    requests.append(case(parsed, ctx, [fixture["events"]], fixture["paths"],
                                         constants=(("A", True),) if "A" in ctx.data["symbols"] else ()))
                    expected.append((path.name, rung, fixture["id"], fixture["expected"]))
        for result, (name, rung, fixture, truth) in zip(native(requests), expected, strict=True):
            self.assertIn(result["verdicts"][0].split(" ")[0], ("Satisfied", "Counterexample"), result)
            self.assertIs(result["verdicts"][0] == "Satisfied", truth, (name, rung, fixture))
        print(f"TEMPORAL EXAMPLE RUNGS: {len(expected)} native fixture verdicts; finite restrictions only for {restricted}")


if __name__ == "__main__":
    unittest.main()
