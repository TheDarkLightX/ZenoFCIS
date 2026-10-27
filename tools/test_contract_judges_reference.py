"""Checks of the independent contract-judge reference on hand-computed cases."""

import copy
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import check_contract_judges as judges
import contract_judges_reference as reference
import instantiate_core as core


def instance(family, parameters, complete=False):
    files = core.source(family, parameters, include_proof=False)
    project=files["project.zeno"].decode()
    policy=json.loads(files["v2/policy.json"])
    if not complete:
        project=project.split("// Independent complete relation")[0]
        policy["law_kinds"]={k:v for k,v in policy["law_kinds"].items() if int(k)<520}
    return reference.Contract(project, policy)


def value(text, grammar=reference.rule_expression, frame=()):
    return reference.compile_tree(grammar(text), {}, {})(list(frame))


class Grammar(unittest.TestCase):
    def test_rule_expressions_bind_as_the_rules_reference_lists(self):
        self.assertEqual(value("1 + 2 == 3 && !false"), 1)
        self.assertEqual(value("2 * 3 + 1"), 7)
        # `->` groups to the right: false -> (false -> false).
        self.assertEqual(value("false -> false -> false"), 1)
        self.assertEqual(value("choose(1 == 2, 5, 6)"), 6)
        self.assertEqual(value("-3 + 1"), -2)

    def test_law_negation_takes_a_whole_relation(self):
        self.assertEqual(value("!1 == 2", reference.law_formula), 1)
        self.assertEqual(value("div_floor(-3, 2) == -2 && div_ceil(-3, 2) == -1",
                               reference.law_formula), 1)
        with self.assertRaises(reference.Undefined):
            value("div_floor(1, 0) == 0", reference.law_formula)
        with self.assertRaises(reference.Unsupported):
            reference.law_formula("forall i in 0..3 { true }")

    def test_every_node_is_evaluated(self):
        # A missing value anywhere makes the formula undefined, even where a
        # short-circuit would not reach it.
        tree = reference.law_formula("1 == 1 || post.100.110 == 0")
        function = reference.compile_tree(tree, {"post.100.110": 0}, {})
        with self.assertRaises(reference.Undefined):
            function([None])
        self.assertEqual(function([4]), 1)


class Judges(unittest.TestCase):
    def test_first_match_case_counts_and_excluded_only_are_independent(self):
        counted = reference.case_coverage(instance("bounded-counter", {"C": 1}, complete=True))
        self.assertEqual(counted["tuples"], 8)
        self.assertEqual([r["decided"] for r in counted["cases"]], [4, 1, 1, 1, 1])
        self.assertEqual(counted["findings"], [])
        excluded = reference.case_coverage(instance("reservation-pool", {"C": 1, "Q": 1}, complete=True))
        self.assertEqual(excluded["cases"][0]["decided"], 8)
        self.assertEqual(excluded["cases"][0]["law_consistent"], 0)
        self.assertEqual(excluded["findings"], [{"case": 0,
                         "kind": "case-only-on-excluded-pre-states", "cause": None}])

    def test_determined_requires_the_unique_outcome_to_match_native_execution(self):
        files = core.source("bounded-counter", {"C": 1}, include_proof=False)
        policy = json.loads(files["v2/policy.json"])
        policy["cases"][0]["when"] = "true"
        contract = reference.Contract(files["project.zeno"].decode(), policy)
        machine = reference.Machine(contract)
        measured = reference.gaps(contract, machine)["admissible"]["full"]
        self.assertEqual(measured["tuples"], 8)
        self.assertEqual(measured["reference_refused"], 4)
        self.assertEqual(measured["determined"], 4)

    def test_counter_variants_preserve_partial_decisions_and_measure_refused_frames(self):
        recorded = json.loads(judges.KNOWN.read_text())["instances"]
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-judge-variants-") as raw:
            for name, files in judges.VARIANTS.items():
                with self.subTest(variant=name):
                    folder = Path(raw) / name
                    judges.write(folder, files())
                    expected = copy.deepcopy(recorded[name])
                    if name == "variant-bounded-counter-c2-refusal":
                        # Twelve frames admit several Reject reasons, including
                        # the one where an Accept is refused by law 504.
                        for domain in ("full", "valid"):
                            expected["gaps"]["admissible"][domain]["freedom"]["reject_reason"] = 12
                    self.assertEqual(judges.reference_answers(folder), expected)

    def test_gap_alternatives_remain_visible_when_the_program_refuses(self):
        files = judges.counter_variant(2, False, True, False)
        contract = reference.Contract(files["project.zeno"], json.loads(files["v2/policy.json"]))
        machine = reference.Machine(contract)
        state, frame = (1,), (150, 1)
        self.assertIsNone(machine.table[(state, frame)])
        for reason in (200, 201):
            self.assertTrue(contract.admits(state, frame, ("Reject", reason, state)))
        measured = reference.gaps(contract, machine)
        for domain in ("full", "valid"):
            self.assertEqual(measured["admissible"][domain]["reference_refused"], 1)
            self.assertEqual(measured["admissible"][domain]["freedom"]["reject_reason"], 12)
        order = [(s, x) for s in contract.states for x in contract.inputs]
        self.assertEqual(measured["admissible_lines"][order.index((state, frame))], "-")

    def test_a_bounded_counter_has_three_reachable_minimal_states_and_no_determined_tuple(self):
        contract = instance("bounded-counter", {"C": 2})
        machine = reference.Machine(contract)
        profile = reference.profile(contract, machine)
        self.assertEqual(profile["states"], {"schema": 3, "valid": 3, "reachable": 3, "minimal": 3})
        gaps = reference.gaps(contract, machine)
        self.assertEqual(gaps["admissible"]["full"]["tuples"], 12)
        self.assertEqual(gaps["admissible"]["full"]["determined"], 0)
        self.assertEqual(gaps["admissible"]["full"]["freedom"],
                         {"class": 4, "reject_reason": 12, "successor": 0})
        self.assertTrue(gaps["reject_everything"]["admitted_where_the_contract_decides"])
        self.assertEqual(gaps["activation"]["full"], {"500": 4, "501": 4, "502": 4})

    def test_a_retry_budget_stores_a_state_its_decisions_never_show(self):
        contract = instance("retry-budget", {"C": 1})
        profile = reference.profile(contract, reference.Machine(contract))
        self.assertEqual(profile["states"], {"schema": 4, "valid": 4, "reachable": 4, "minimal": 3})

    def test_lemma_verdicts(self):
        contract = instance("logical-deadline", {"C": 1})
        machine = reference.Machine(contract)
        verdict = lambda text: reference.lemma(contract, machine, text)
        self.assertEqual(verdict("post.100.110 <= 1")["verdict"], "inductive")
        self.assertEqual(verdict("post.100.112 == 0")["verdict"], "false")
        self.assertEqual(verdict("post.100.110 == 0")["verdict"], "false")
        weak = verdict("!(post.100.110 == 0 && post.100.111 == 0 && post.100.112 == 1)")
        self.assertEqual(weak["verdict"], "not-inductive")
        self.assertEqual(weak["counterexample_to_induction"]["state"], [0, 0, 0])

    def test_a_scenario_replays_and_kills_the_reject_everything_core(self):
        contract = instance("bounded-counter", {"C": 2})
        machine = reference.Machine(contract)
        up = "# scenario up\n0 | 150 | 1 | accept - 1 | -\n1 | 150 | 1 | accept - 2 | -\n"
        result = reference.scenarios(contract, machine, up)
        self.assertEqual([item["verdict"] for item in result["scenarios"]], ["passed"])
        self.assertTrue(result["reject_everything_killed"])
        # A scenario that fails on the contract credits no kill.
        wrong = up + "# scenario wrong\n0 | 150 | 1 | reject 201 0 | -\n"
        result = reference.scenarios(contract, machine, wrong)
        self.assertEqual([item["verdict"] for item in result["scenarios"]], ["passed", "failed"])
        self.assertEqual(result["reject_everything_killed"], "withheld")

    def test_committed_failures_and_refusals_are_decided_as_the_rules_say(self):
        files = core.source("bounded-counter", {"C": 2}, include_proof=False)
        policy = json.loads(files["v2/policy.json"])
        dec = next(case for case in policy["cases"] if case["when"] == "action == 151 && value == 0")
        dec["class"] = "CommittedFailure"
        policy["law_kinds"].update({"503": "CommittedFailureEffects", "504": "DebitCreditEffectEquality"})
        policy["law_kinds"]={k:v for k,v in policy["law_kinds"].items() if int(k)<520}
        project = files["project.zeno"].decode().split("// Independent complete relation")[0] + (
            "law 503 keeps on failure = post.100.110 == pre.100.110;\n"
            "law 504 short on accept = post.100.110 <= 1;\n")
        contract = reference.Contract(project, policy)
        machine = reference.Machine(contract)
        self.assertEqual(machine.table[((0,), (151, 1))], ("CommittedFailure", 202, (0,)))
        self.assertIsNone(machine.table[((1,), (150, 1))])
        self.assertEqual(machine.refusals[((1,), (150, 1))], ("law", 504))
        gaps = reference.gaps(contract, machine)
        self.assertEqual(gaps["admissible"]["full"]["reference_refused"], 1)
        probes = reference.probes(contract, machine)
        self.assertEqual(probes["decisions"]["Refused law 504"], 1)
        self.assertEqual(probes["decisions"]["CommittedFailure 202"], 1)

    def test_complete_counter_and_budget_relations(self):
        for family in ("bounded-counter", "consumable-budget"):
            contract=instance(family, {"C":2}, complete=True)
            measured=reference.gaps(contract, reference.Machine(contract))
            self.assertEqual(measured["admissible"]["full"]["determined"], measured["admissible"]["full"]["tuples"])
            self.assertFalse(measured["reject_everything"]["admitted_where_the_contract_decides"])
            self.assertTrue(measured["reject_everything"]["refused_where_the_contract_decides"])

    def test_deliveries_are_outside_the_reference(self):
        app = core.ROOT / "examples/dual-approval"
        with self.assertRaises(reference.Unsupported):
            reference.Contract((app / "project.zeno").read_text(),
                               json.loads((app / "v2/policy.json").read_text()))

    def test_the_command_line_prints_known_answers(self):
        files = core.source("bounded-counter", {"C": 1}, include_proof=False)
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-reference-") as raw:
            root = Path(raw)
            for name, contents in files.items():
                (root / name).parent.mkdir(parents=True, exist_ok=True)
                (root / name).write_bytes(contents)
            answers = reference.answers(root, ["true"], None)
        self.assertEqual(answers["lemmas"][0]["verdict"], "inductive")
        self.assertEqual(answers["profile"]["states"]["schema"], 2)


class ReviewPackets(unittest.TestCase):
    def test_missing_and_old_review_packets_are_mismatches(self):
        for packet in (None, {"schema": "zeno-fcis/contract-review/3"}):
            with self.subTest(packet=packet), patch.object(judges, "cli", return_value=packet):
                with self.assertRaises(judges.Mismatch):
                    judges.cli_answers(Path("unused"), Path("unused"), {"review_exit": 0}, Path("unused"))


@unittest.skipUnless(os.environ.get("ZENO_FCIS_CLI"), "set ZENO_FCIS_CLI for native agreement")
class ReasonClassAgreement(unittest.TestCase):
    def native_gaps(self, app, work, contract, proposals=None):
        packet = work / "review.json"
        args = [os.environ["ZENO_FCIS_CLI"], "contract", "review", str(app), "--gaps",
                "--out", str(packet), "--format", "json"]
        if proposals is not None:
            path = work / "laws.zeno"
            path.write_text(proposals)
            args += ["--gaps-laws", str(path)]
        run = subprocess.run(args, capture_output=True, text=True, check=False)
        report = json.loads(packet.read_text())
        cases = reference.case_coverage(contract)
        self.assertEqual(run.returncode, int(bool(cases["findings"])), run.stdout + run.stderr)
        self.assertEqual(report["summary"]["findings"], len(cases["findings"]))
        self.assertEqual([{k: r[k] for k in ("case", "guard_true", "decided", "law_consistent")}
                          for r in report["case_coverage"]["cases"]], cases["cases"])
        result = report["gaps"]
        self.assertEqual(result["status"], "measured")
        return result["proposals"]["after"] if proposals is not None else result

    def test_one_tuple_reason_classes_determine_one_reject_in_both_checkers(self):
        fixture = core.ROOT / "tools/test_data/contract_judges/reason-classes"
        project = (fixture / "project.zeno").read_text()
        policy = json.loads((fixture / "policy.json").read_text())
        contract = reference.Contract(project, policy)
        measured = reference.gaps(contract, reference.Machine(contract))
        self.assertEqual(measured["admissible"]["full"]["determined"], 1)
        self.assertEqual(measured["admissible_lines"], ["1"])
        self.assertEqual([d for d in reference.candidates(contract, (0,))
                          if contract.admits((0,), (0, 0), d)], [("Reject", 200, (0,))])
        with tempfile.TemporaryDirectory(prefix="reason-classes-") as raw:
            work = Path(raw)
            app = work / "app"
            judges.write(app, {"project.zeno": project, "v2/policy.json": json.dumps(policy)})
            native = self.native_gaps(app, work, contract)
            self.assertEqual(native["admissible"]["full"]["determined"], 1)
            self.assertEqual(native["tables"]["admissible_sha256"], measured["admissible_sha256"])

    def test_dual_approval_reject_requirement_determines_one_tuple_in_both_checkers(self):
        app = core.ROOT / "examples/dual-approval"
        project = (app / "project.zeno").read_text()
        policy = json.loads((app / "v2/policy.json").read_text())
        target = "pre.100.110 == 151 && pre.100.111 == 161 && pre.100.112 == 162 && command.101 == 140 && context.102.120 == 161"
        proposals = "".join(f"law {9000+i} closed_choice_{i} on {scope} = !({target});\n"
                            for i, scope in enumerate(["reject:201", "reject:203", "accept", "failure"]))
        # The reference deliberately does not interpret deliveries. Project only
        # outboxes and their declarations away, preserving all scalar domains,
        # laws and reason classes. The chosen Reject cannot emit an outbox.
        scalar_project = "\n".join(line for line in project.splitlines() if not line.startswith(
            ("type 103 ", "type 104 ", "field 130 ", "field 131 ", "channel ")))
        scalar_policy = copy.deepcopy(policy)
        scalar_policy["leaf_bindings"].pop("103")
        for case in scalar_policy["cases"]:
            case["outbox"] = []
        scalar_policy["law_kinds"].update({str(i): "AuthoritySubjectRecipient" for i in range(9000, 9004)})
        contract = reference.Contract(scalar_project + "\n" + proposals, scalar_policy)
        admitted = [d for d in reference.candidates(contract, (151, 161, 162))
                    if contract.admits((151, 161, 162), (140, 161), d)]
        self.assertEqual(admitted, [("Reject", 200, (151, 161, 162))])
        measured = reference.gaps(contract, reference.Machine(contract))
        # Every other input still admits all three Reject reasons. On the
        # chosen input only reason 200 remains; failure-only 202 is not a Reject.
        self.assertEqual(measured["admissible"]["full"]["determined"], 1)
        with tempfile.TemporaryDirectory(prefix="approval-reason-classes-") as raw:
            native = self.native_gaps(app, Path(raw), contract, proposals)
            self.assertEqual(native["admissible"]["full"]["determined"], 1)


if __name__ == "__main__":
    unittest.main()
