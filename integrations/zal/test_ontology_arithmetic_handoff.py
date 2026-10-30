"""Acceptance: conditional inference cannot silently select the wrong numeric API."""
from copy import deepcopy
import itertools
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from arithmetic_handoff import KNOWN, SIGNED, UNSIGNED, WIDE, build, select
from arithmetic_handoff_tools import handoff, replay, source_binding
from mcp import Bridge, TOOLS
from zal_notation import core
from zal_notation.ontology_cnl import parse_ontology_cnl

HERE = Path(__file__).resolve().parent
EXAMPLE = HERE / "examples/ontology"


def example():
    return parse_ontology_cnl((EXAMPLE / "defi_arithmetic.english.txt").read_text())


def bindings():
    return json.loads((EXAMPLE / "defi_arithmetic.bindings.json").read_text())["bindings"]


def finite_oracle(status, complete, consistency, present, wide, unsigned, signed, unknown, target):
    """Independent decision table; no production selection/helpers used."""
    if status in {"unsupported", "incomplete", "inconsistent"}:
        return status
    if (complete, consistency) != (True, "consistent"):
        return "incomplete"
    if present is False:
        return "missing"
    if (unknown, target in {"U256", "I256"}) != (False, True):
        return "unsupported"
    table = {
        (False, False, False): "missing", (False, False, True): "missing",
        (False, True, False): "missing", (False, True, True): "inconsistent",
        (True, False, False): "missing", (True, True, True): "inconsistent",
        (True, True, False): "U256", (True, False, True): "I256",
    }
    result = table[(wide, unsigned, signed)]
    return ("bound" if result == target else "type-mismatch") if result in {"U256", "I256"} else result


def decision_cases():
    return itertools.product(("classified", "unsupported", "incomplete", "inconsistent"),
        (False, True), ("consistent", "inconsistent", "unknown"),
        *((False, True),) * 5, ("U256", "I256", "U128"))


class ArithmeticHandoffTests(unittest.TestCase):
    def test_entire_finite_selector_against_independent_decision_table(self):
        count = 0
        for case in decision_cases():
            actual = select(*case)
            self.assertEqual(actual, finite_oracle(*case), case)
            if actual == "bound":
                status, complete, consistency, present, wide, unsigned, signed, unknown, target = case
                self.assertEqual((status, complete, consistency, present, wide, unknown),
                                 ("classified", True, "consistent", True, True, False))
                self.assertNotEqual(unsigned, signed)
                self.assertEqual(target, "U256" if unsigned else "I256")
            count += 1
        self.assertEqual(count, 2304)

    def test_example_has_exact_distinct_domains_and_actual_interfaces(self):
        packet = handoff(example(), bindings())
        self.assertEqual(packet["status"], "ready")
        rows = {r["binding"]["individual"]: r for r in packet["requirements"]}
        for who, target, lower, upper in (("swapBalance", "U256", 0, (1 << 256)-1),
                                        ("signedPnl", "I256", -(1 << 255), (1 << 255)-1)):
            row = rows[who]
            req = row["type_requirement"]
            self.assertEqual((req["nominal_type"], req["minimum"], req["maximum"]), (target, str(lower), str(upper)))
            self.assertEqual(req["schema_kind"], "TypeKind::" + target)
            self.assertEqual(len(req["fact_ids"]), 2)
            self.assertTrue(all(o["status"] == "open" for o in row["obligations"] if o["kind"] != "nominal-binding"))
            self.assertTrue(all(o["authority"] == "none" for o in row["obligations"]))
        self.assertNotEqual(rows["swapBalance"]["requirement_id"], rows["signedPnl"]["requirement_id"])

    def test_swapped_signedness_is_refuted_in_both_directions(self):
        planned = bindings()
        for row in planned:
            row["target"] = "I256" if row["target"] == "U256" else "U256"
        packet = handoff(example(), planned)
        self.assertEqual(packet["status"], "blocked")
        for row in packet["requirements"]:
            self.assertEqual(row["status"], "type-mismatch")
            nominal = row["obligations"][0]
            self.assertEqual(nominal["status"], "refuted")
            self.assertNotEqual(nominal["witness"]["required"], nominal["witness"]["supplied"])

    def test_missing_individual_width_or_signedness_is_not_false_or_default(self):
        for remove in ("wide-requirement", "signed-requirement", "unsigned-requirement"):
            doc = example()
            doc["axioms"] = [a for a in doc["axioms"] if a["id"] != remove]
            packet = build(doc, bindings())
            self.assertEqual(packet["status"], "blocked")
            for row in packet["requirements"]:
                if row["status"] == "missing":
                    self.assertIsNone(row["type_requirement"])
                    self.assertEqual(row["obligations"], [])
        planned = [{"individual": "absent", "target": "U256", "type_id": 3}]
        self.assertEqual(build(example(), planned)["requirements"][0]["status"], "missing")

    def test_conflicting_signs_refuse_even_without_disjoint_axiom(self):
        doc = example()
        doc["facts"].append({"id": "contrary", "kind": "type", "individual": "swapBalance", "class": SIGNED})
        result = build(doc, bindings())
        self.assertEqual(result["classification"]["status"], "classified")
        row = next(r for r in result["requirements"] if r["binding"]["individual"] == "swapBalance")
        self.assertEqual(row["status"], "inconsistent")
        self.assertEqual(row["obligations"], [])

    def test_ontology_contradiction_blocks_all_bindings_without_explosion(self):
        doc = example()
        doc["axioms"].append({"id": "disjoint", "kind": "disjoint", "classes": [UNSIGNED, SIGNED]})
        doc["facts"].append({"id": "contrary", "kind": "type", "individual": "swapBalance", "class": SIGNED})
        result = build(doc, bindings())
        self.assertEqual(result["classification"]["status"], "inconsistent")
        self.assertTrue(all(r["status"] == "inconsistent" and not r["obligations"] for r in result["requirements"]))

    def test_exhaustion_keeps_justified_prefix_non_authoritative(self):
        for limit in (0, 12, 20):
            result = build(example(), bindings(), max_work=limit)
            self.assertEqual(result["classification"]["status"], "incomplete")
            self.assertTrue(all(r["status"] == "incomplete" and not r["obligations"] for r in result["requirements"]))
            self.assertEqual(result["classification"]["usage"]["work"], limit)

    def test_unsupported_axiom_schema_requirement_and_target_are_preserved(self):
        for change in ("schema", "axiom", "requirement", "target"):
            doc, planned = example(), bindings()
            if change == "schema":
                doc["schema"] = "zal.ontology/unsupported"
            elif change == "axiom":
                doc["axioms"].append({"id": "unsupported", "kind": "create-existential"})
            elif change == "requirement":
                doc["classes"].append("NeedsDecimalArithmetic")
                doc["facts"].append({"id": "decimal", "kind": "type", "individual": "swapBalance", "class": "NeedsDecimalArithmetic"})
            else:
                planned[0]["target"] = "U128"
            result = build(doc, planned)
            self.assertEqual(result["status"], "blocked")
            row = next(r for r in result["requirements"] if r["binding"]["individual"] == "swapBalance")
            self.assertEqual(row["status"], "unsupported")
            self.assertEqual(row["obligations"], [])

    def test_new_evidence_can_block_a_previously_bound_requirement(self):
        doc = example()
        before = handoff(doc, bindings())
        doc["facts"].append({"id": "new-sign", "kind": "type", "individual": "swapBalance", "class": SIGNED})
        self.assertFalse(replay(doc, bindings(), before)["matches_replay"])
        self.assertEqual(handoff(doc, bindings())["status"], "blocked")

    def test_exact_replay_rejects_forged_proof_status_type_and_source(self):
        doc, planned = example(), bindings()
        packet = handoff(doc, planned)
        self.assertTrue(replay(doc, planned, packet)["matches_replay"])
        for mutation in ("status", "type", "source", "authority", "schema-id", "proof"):
            bad = deepcopy(packet)
            if mutation == "status": bad["requirements"][0]["obligations"][1]["status"] = "satisfied"
            if mutation == "type": bad["requirements"][0]["type_requirement"]["nominal_type"] = "U256"
            if mutation == "source": bad["source_binding"]["contracts"]["base_revision"] = "0" * 40
            if mutation == "authority": bad["authority"] = "execute"
            if mutation == "schema-id": bad["requirements"][0]["type_requirement"]["schema_type_id"] = 99
            if mutation == "proof": bad["classification"]["proofs"][0]["assertion"] = "forged"
            self.assertFalse(replay(doc, planned, bad)["matches_replay"], mutation)

    def test_matching_incomplete_packet_stays_blocked(self):
        doc, planned = example(), bindings()
        packet = handoff(doc, planned, max_work=0)
        result = replay(doc, planned, packet, max_work=0)
        self.assertTrue(result["matches_replay"])
        self.assertEqual(result["current_status"], "blocked")

    def test_inputs_and_returned_results_have_no_mutable_aliases(self):
        doc, planned = example(), bindings()
        originals = deepcopy((doc, planned))
        packet = handoff(doc, planned)
        packet["requirements"][0]["binding"]["target"] = "changed"
        packet["classification"]["proofs"].clear()
        packet["source_binding"]["bridge"].clear()
        self.assertEqual((doc, planned), originals)
        self.assertEqual(handoff(doc, planned)["status"], "ready")

    def test_reordering_bindings_preserves_output(self):
        self.assertEqual(handoff(example(), bindings()), handoff(example(), list(reversed(bindings()))))

    def test_source_changes_are_refused_before_and_after_core_invocation(self):
        import arithmetic_handoff_tools as adapter
        with patch.object(adapter, "implementation", return_value={}):
            with self.assertRaises(core.NotationError): handoff(example(), bindings())
        original = source_binding()
        with patch.object(adapter, "source_binding", side_effect=[original, {}]):
            with self.assertRaises(core.NotationError): handoff(example(), bindings())

    def test_malformed_bindings_never_become_open_or_satisfied(self):
        for bad in ([], [{}], [{"individual": "x", "target": "U256", "type_id": True}],
                    bindings() * 2, [{"individual": "x", "target": "U256", "type_id": 1 << 32}],
                    [{"individual": "x", "target": "U256", "type_id": 0}],
                    [{**bindings()[0], "execute": True}]):
            with self.assertRaises(core.NotationError): build(example(), bad)

    def test_source_pin_mutation_is_detected(self):
        import arithmetic_handoff_tools as adapter
        original = adapter._read
        path = next(iter(source_binding()["contracts"]["files"]))
        def changed(p):
            data = original(p)
            return data + b"\n" if p == adapter.ROOT / path else data
        with patch.object(adapter, "_read", side_effect=changed):
            with self.assertRaises(core.NotationError): handoff(example(), bindings())


class ArithmeticTransportTests(unittest.TestCase):
    def test_real_mcp_stdio_is_inert_and_rejects_extra_authority(self):
        requests = [{"jsonrpc": "2.0", "id": 0, "method": "tools/list"}]
        args = {"ontology": example(), "bindings": bindings()}
        for i, arguments in enumerate((args, {**args, "execute": True}, {**args, "max_work": 0}), 1):
            requests.append({"jsonrpc": "2.0", "id": i, "method": "tools/call", "params": {
                "name": "zal_arithmetic_handoff", "arguments": arguments}})
        with tempfile.TemporaryDirectory() as temporary:
            workspace = Path(temporary) / "never-created.json"
            process = subprocess.run([sys.executable, *(["-O"] if sys.flags.optimize else []), "-B",
                str(HERE / "mcp.py"), "--workspace", str(workspace)], cwd=HERE,
                input="".join(json.dumps(r) + "\n" for r in requests), capture_output=True, text=True, timeout=30)
            self.assertEqual(process.returncode, 0, process.stderr)
            replies = [json.loads(line) for line in process.stdout.splitlines()]
            self.assertEqual(len(replies), 4)
            names = [t["name"] for t in replies[0]["result"]["tools"]]
            self.assertEqual(len(names), len(set(names)))
            self.assertIn("zal_arithmetic_replay", names)
            self.assertFalse(replies[1]["result"]["isError"])
            self.assertTrue(replies[2]["result"]["isError"])
            blocked = json.loads(replies[3]["result"]["content"][0]["text"])
            self.assertEqual((blocked["status"], blocked["authority"]), ("blocked", "none"))
            self.assertFalse(workspace.exists())

    def test_actual_mcp_replay_cannot_import_satisfied_evidence(self):
        class NoWorkspace:
            def use(self, action): raise AssertionError("behavioral workspace accessed")
        doc, planned = example(), bindings()
        packet = handoff(doc, planned)
        packet["requirements"][0]["obligations"][1]["status"] = "satisfied"
        result = Bridge(NoWorkspace()).call("zal_arithmetic_replay", {"ontology": doc, "bindings": planned, "packet": packet})
        self.assertEqual(result["status"], "mismatch")

    def test_cli_exits_two_on_incomplete_and_preserves_existing_outputs(self):
        argv = [sys.executable, *(["-O"] if sys.flags.optimize else []), "-B", "-m", "zal_notation", "arithmetic-handoff",
                "--source", str(EXAMPLE / "defi_arithmetic.english.txt"),
                "--bindings", str(EXAMPLE / "defi_arithmetic.bindings.json")]
        for extra, code, status in (([], 0, "ready"), (["--max-work", "0"], 2, "blocked")):
            run = subprocess.run(argv + extra, cwd=HERE, capture_output=True, text=True, timeout=30)
            self.assertEqual(run.returncode, code, run.stderr)
            self.assertEqual(json.loads(run.stdout)["status"], status)
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "retained.json"
            path.write_text("keep")
            run = subprocess.run(argv + ["--output", str(path)], cwd=HERE, capture_output=True, text=True, timeout=30)
            self.assertEqual(run.returncode, 2)
            self.assertEqual(path.read_text(), "keep")


if __name__ == "__main__":
    unittest.main()
