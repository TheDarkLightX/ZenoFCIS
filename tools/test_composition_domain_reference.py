#!/usr/bin/env python3
"""Tests of the independent composed-domain reference against the profile's documented behaviour."""
from __future__ import annotations

import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import composition_domain_reference as reference  # noqa: E402

FIXTURES = Path(__file__).resolve().parents[1] / "crates/zeno-fcis-contract/tests/fixtures/checked-composition-3"


def load(name: str) -> reference.Composition:
    return reference.Composition(FIXTURES / name)


def build(directory: Path, components: list[dict], couplings: list[dict] | None = None) -> reference.Composition:
    """A manifest over copies of the fault fixtures' components; pins are not the reference's concern."""
    for component in components:
        source = FIXTURES / "faults" / component.pop("from")
        if not (directory / component["path"]).exists():
            shutil.copytree(source, directory / component["path"])
    manifest = {"schema": reference.SCHEMA, "checker": "zeno-fcis/composed-domain-checker/1", "name": "test",
                "request": [{"name": "op", "domain": {"kind": "int", "min": 0, "max": 2}},
                            {"name": "qty", "domain": {"kind": "int", "min": 1, "max": 2}},
                            {"name": "authorized", "domain": {"kind": "bool"}}],
                "components": components, "couplings": couplings or []}
    (directory / "manifest.json").write_text(json.dumps(manifest))
    return reference.Composition(directory / "manifest.json")


def counter(**changes) -> dict:
    entry = {"name": "counter", "path": "counter", "from": "counter", "owns": ["counter.value"],
             "participates": "op == 0", "bind": {"action": "150", "authorized": "authorized"},
             "assume": "true", "invariant": "true"}
    entry.update(changes)
    return entry


def budget(**changes) -> dict:
    entry = {"name": "budget", "path": "budget", "from": "budget", "owns": ["budget.remaining"],
             "participates": "op != 2", "bind": {"amount": "qty", "authorized": "authorized"},
             "assume": "amount >= 1", "invariant": "true"}
    entry.update(changes)
    return entry


def wire(**changes) -> dict:
    entry = {"name": "wire", "path": "wire", "from": "tripwire", "owns": ["wire.tripped", "wire.touches"],
             "participates": "true", "bind": {"fire": "qty == 2", "authorized": "authorized"},
             "assume": "true", "invariant": "true"}
    entry.update(changes)
    return entry


class Formulas(unittest.TestCase):
    def value(self, text: str, **env) -> int | None:
        return reference.evaluate(reference.parse(text), env)

    def test_precedence_and_grouping(self):
        self.assertEqual(self.value("1 + 2 * 3"), 7)
        self.assertEqual(self.value("1 < 2 == 1"), 1)
        self.assertEqual(self.value("false -> false -> false"), 1)
        self.assertEqual(self.value("(false -> false) -> false"), 0)
        self.assertEqual(self.value("a.b - -2", **{"a.b": 3}), 5)

    def test_numbers_are_true_exactly_at_one_and_choose_selects(self):
        self.assertEqual(self.value("!2"), 1)
        self.assertEqual(self.value("2 && true"), 0)
        self.assertEqual(self.value("choose(x, 10, 20)", x=1), 10)
        self.assertEqual(self.value("choose(x, 10, 20)", x=2), 20)

    def test_a_binding_overflowing_64_bits_has_no_value(self):
        self.assertIsNone(self.value("9223372036854775807 + q", q=1))
        self.assertEqual(self.value("9223372036854775806 + q", q=1), (1 << 63) - 1)

    def test_a_formula_without_a_value_is_false(self):
        self.assertIsNone(self.value("div_floor(1, 0)"))
        self.assertFalse(reference.holds(reference.parse("div_floor(1, 0) == 0"), {}))
        self.assertEqual(self.value("div_ceil(-3, 2)"), -1)
        self.assertEqual(self.value("div_floor(-3, 2)"), -2)


class JointStep(unittest.TestCase):
    def setUp(self):
        self.directory = Path(tempfile.mkdtemp(prefix="composition-domain-reference-"))

    def tearDown(self):
        shutil.rmtree(self.directory)

    def step(self, composition, states, request):
        named = dict(zip(("op", "qty", "authorized"), request))
        return composition.joint_step({k: tuple(v) for k, v in states.items()}, named)

    def test_every_accept_commits_and_bindings_read_the_pre_state(self):
        composition = build(self.directory, [counter(), budget(bind={"amount": "choose(counter.value == 0, 1, 2)",
                                                                    "authorized": "authorized"})])
        outcome, after = self.step(composition, {"counter": [0], "budget": [2]}, (0, 1, 1))
        self.assertEqual(outcome, ("Accept",))
        self.assertEqual(after, {"counter": (1,), "budget": (1,)})

    def test_a_skipped_component_and_a_later_reject_changing_nothing(self):
        composition = build(self.directory, [counter(), budget()])
        outcome, after = self.step(composition, {"counter": [0], "budget": [2]}, (1, 1, 1))
        self.assertEqual((outcome, after["counter"]), (("Accept",), (0,)))
        outcome, after = self.step(composition, {"counter": [0], "budget": [1]}, (0, 2, 1))
        self.assertEqual(outcome, ("Reject", "budget", 201))
        self.assertEqual(after, {"counter": (0,), "budget": (1,)})

    def test_refusals_end_the_step_with_no_change(self):
        composition = build(self.directory, [counter(), budget(bind={"amount": "qty + 1", "authorized": "authorized"})])
        outcome, after = self.step(composition, {"counter": [0], "budget": [2]}, (0, 2, 1))
        self.assertEqual(outcome, ("Refused", "budget", "binding-outside-domain"))
        self.assertEqual(after, {"counter": (0,), "budget": (2,)})
        ticker = {"name": "ticker", "path": "ticker", "from": "ticker", "owns": ["ticker.count"],
                  "participates": "true", "bind": {"step": "qty - 1", "authorized": "authorized"},
                  "assume": "true", "invariant": "true"}
        composition = build(self.directory, [counter(), ticker])
        outcome, _ = self.step(composition, {"counter": [0], "ticker": [2]}, (0, 2, 1))
        self.assertEqual(outcome, ("Refused", "ticker", "law"))

    def test_the_first_committed_failure_commits_its_prefix(self):
        composition = build(self.directory, [counter(), wire(), budget()])
        outcome, after = self.step(composition, {"counter": [0], "wire": [0, 0], "budget": [2]}, (0, 2, 1))
        self.assertEqual(outcome, ("CommittedFailure", "wire", 201))
        self.assertEqual(after, {"counter": (1,), "wire": (1, 0), "budget": (2,)})

    def test_a_read_only_component_that_changes_its_state_is_refused(self):
        read_only = counter(owns=[], bind={"value": "0", "action": "150", "authorized": "authorized"})
        composition = build(self.directory, [budget(), read_only])
        outcome, after = self.step(composition, {"budget": [2]}, (0, 1, 1))
        self.assertEqual(outcome, ("Refused", "counter", "read-only-state-changed"))
        self.assertEqual(after, {"budget": (2,)})


class Obligations(unittest.TestCase):
    def test_the_committed_fixtures(self):
        expected = {
            "guarded/composition.json": ("proved", None),
            "faults/coupling-false-at-genesis.json": ("violated", "couple"),
            "faults/coupling-broken-by-step.json": ("violated", "couple"),
            "faults/binding-out-of-domain.json": ("violated", "assume"),
            "faults/invariant-not-preserved.json": ("violated", "local"),
            "faults/read-only-changes.json": ("violated", "local"),
            "faults/ticker-tripwire.json": ("proved", None),
        }
        for name, (verdict, kind) in expected.items():
            with self.subTest(name):
                result = load(name).obligations()
                self.assertEqual(result["verdict"], verdict)
                self.assertEqual((result["first"] or {}).get("kind"), kind)

    def test_a_binding_with_multiplication_is_refused(self):
        with tempfile.TemporaryDirectory() as directory, self.assertRaises(reference.Refused):
            build(Path(directory), [counter(), budget(bind={"amount": "qty * 1", "authorized": "authorized"})])

    def test_scope_refusals(self):
        for name in ("faults/binding-reads-later.json", "faults/two-owners.json"):
            with self.subTest(name), self.assertRaises(reference.Refused):
                load(name)

    def test_the_wide_coupling_is_over_budget(self):
        result = load("ladder-10/wide.json").obligations()
        self.assertEqual(result["verdict"], "refused")
        self.assertIn(["couple", "wide", "86782189536"], result["sizes"])

    def test_a_committed_failure_between_members_may_cut_the_step(self):
        with tempfile.TemporaryDirectory() as directory:
            spender = budget(participates="op == 0", bind={"amount": "1", "authorized": "authorized"})
            coupling = [{"name": "k", "members": ["counter", "budget"],
                         "invariant": "counter.value + budget.remaining <= 2"}]
            between = build(Path(directory), [counter(), wire(), dict(spender)], coupling)
            self.assertEqual(between.check_couple("k")["cut"], ["wire"])
        with tempfile.TemporaryDirectory() as directory:
            spender = budget(participates="op == 0", bind={"amount": "1", "authorized": "authorized"})
            after = build(Path(directory), [counter(), dict(spender), wire()], [
                {"name": "k", "members": ["counter", "budget"],
                 "invariant": "counter.value + budget.remaining <= 2"}])
            self.assertIsNone(after.check_couple("k"))


def documented(directory: Path, components: list[dict], couplings: list[dict] | None = None) -> reference.Composition:
    """A manifest over the documented cases' components, with request op 0..1 and authorized."""
    for component in components:
        source = FIXTURES / "documented" / component.pop("from")
        if not (directory / component["path"]).exists():
            shutil.copytree(source, directory / component["path"])
        component.setdefault("assume", "true")
        component.setdefault("invariant", "true")
    manifest = {"schema": reference.SCHEMA, "checker": "zeno-fcis/composed-domain-checker/1", "name": "documented",
                "request": [{"name": "op", "domain": {"kind": "int", "min": 0, "max": 1}},
                            {"name": "authorized", "domain": {"kind": "bool"}}],
                "components": components, "couplings": couplings or []}
    (directory / "manifest.json").write_text(json.dumps(manifest))
    return reference.Composition(directory / "manifest.json")


class DocumentedCases(unittest.TestCase):
    """The profile specification's documented cases, restated over ZenoFCIS contracts."""

    def setUp(self):
        self.directory = Path(tempfile.mkdtemp(prefix="composition-domain-documented-"))

    def tearDown(self):
        shutil.rmtree(self.directory)

    def test_the_procurement_is_proved_and_its_product_is_clean(self):
        composition = load("documented/procurement.json")
        self.assertEqual(composition.obligations()["verdict"], "proved")
        oracle = composition.oracle(10_000)
        self.assertTrue(oracle["complete"])
        self.assertEqual(oracle["violations"], [])

    def test_a_refused_successor_commits_nothing(self):
        composition = documented(self.directory, [
            {"name": "ticker", "path": "ticker", "from": "ticker", "owns": ["ticker.count"], "participates": "true",
             "bind": {"step": "1", "authorized": "authorized"}, "invariant": "count <= 2"},
            {"name": "budget", "path": "budget", "from": "small-budget", "owns": ["budget.remaining"],
             "participates": "true",
             "bind": {"amount": "choose(ticker.count <= 2, 0, 5)", "authorized": "authorized"}}])
        self.assertEqual(composition.obligations()["verdict"], "proved")
        oracle = composition.oracle(10_000)
        self.assertEqual(oracle["violations"], [])
        self.assertGreater(oracle["outcomes"].get("Refused", 0), 0)

    def test_a_committed_failure_can_break_an_invariant(self):
        composition = documented(self.directory, [
            {"name": "wire", "path": "wire", "from": "tripwire", "owns": ["wire.tripped", "wire.touches"],
             "participates": "true", "bind": {"fire": "true", "authorized": "authorized"},
             "invariant": "tripped == 0"}])
        self.assertEqual(composition.obligations()["verdict"], "violated")
        self.assertEqual(composition.oracle(10_000)["violations"][0]["invariant"], "wire")

    def test_an_outside_failure_between_lockstep_members(self):
        def counter(name):
            return {"name": name, "path": name, "from": "counter-c2", "owns": [f"{name}.value"],
                    "participates": "true",
                    "bind": {"action": "choose(op == 0, 150, 151)", "authorized": "authorized"}}
        wire = {"name": "wire", "path": "wire", "from": "tripwire", "owns": ["wire.tripped", "wire.touches"],
                "participates": "true", "bind": {"fire": "true", "authorized": "authorized"}}
        lock = [{"name": "lock", "members": ["a", "b"], "invariant": "a.value == b.value"}]
        between = documented(self.directory, [counter("a"), dict(wire), counter("b")], lock)
        self.assertEqual(between.obligations()["verdict"], "violated")
        self.assertEqual(between.oracle(10_000)["violations"][0]["coupling"], "lock")
        shutil.rmtree(self.directory)
        self.directory.mkdir()
        after = documented(self.directory, [counter("a"), counter("b"), dict(wire)], lock)
        self.assertEqual(after.obligations()["verdict"], "proved")
        oracle = after.oracle(10_000)
        self.assertEqual(oracle["violations"], [])
        self.assertGreater(oracle["outcomes"].get("CommittedFailure", 0), 0)


class CommittedFailureCoverage(unittest.TestCase):
    def test_a_proved_composition_reaches_committed_failures_in_its_product(self):
        composition = load("documented/lockstep-failure.json")
        self.assertEqual(composition.obligations()["verdict"], "proved")
        oracle = composition.oracle(10_000)
        self.assertTrue(oracle["complete"])
        self.assertEqual(oracle["violations"], [])
        self.assertGreater(oracle["outcomes"].get("CommittedFailure", 0), 0)


class Oracle(unittest.TestCase):
    def test_violations_are_found_in_the_reachable_product(self):
        self.assertEqual(load("guarded/composition.json").oracle(1000)["violations"], [])
        oracle = load("faults/coupling-broken-by-step.json").oracle(1000)
        self.assertTrue(oracle["complete"])
        self.assertTrue(any("coupling" in violation for violation in oracle["violations"]))
        oracle = load("faults/binding-out-of-domain.json").oracle(1000)
        self.assertTrue(any(violation.get("failed") == "binding-outside-domain" for violation in oracle["violations"]))

    def test_an_exploration_over_budget_is_incomplete(self):
        self.assertFalse(load("faults/ticker-tripwire.json").oracle(10)["complete"])


if __name__ == "__main__":
    unittest.main()
