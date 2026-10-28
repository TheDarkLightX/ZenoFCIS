"""Diagrams retain exactly the checked finite graph, including hidden branches."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
import xml.etree.ElementTree as ET

import behavior
import behavior2
import observation_diagrams as diagrams
import relational_observation as relational
from arena_spec import parse as parse_arena
from knowledge import solve


FIXTURES = Path(__file__).resolve().parent / "examples" / "observations"
NS = {"svg": "http://www.w3.org/2000/svg"}


def arena():
    return dict(schema="zeno-fcis/arena/1", observation="partial", roots=[0, 1],
                nodes=[dict(owner=0, observation="unknown", edges=[["retry", 2], ["stop", 3]]),
                       dict(owner=0, observation="unknown", edges=[["retry", 2], ["stop", 2]]),
                       dict(owner=1, observation="complete", edges=[["hold", 2]]),
                       dict(owner=1, observation="violation", edges=[["hold", 3]])],
                objective=dict(kind="safety", states=[3]))


class ObservationDiagramTests(unittest.TestCase):
    def test_graph_has_exactly_every_node_and_transition(self):
        model = arena()
        result = diagrams.render(model, certificate=solve(parse_arena(model)))
        for key, nodes, rows in (("arena_svg", model["nodes"], model["nodes"]),):
            root = ET.fromstring(result[key])
            shown_nodes = {int(x.attrib["data-node"]) for x in root.findall(".//svg:g", NS)}
            self.assertEqual(shown_nodes, set(range(len(nodes))))
            expected = [(i, t) for i, row in enumerate(rows) for _, t in row["edges"]]
            paths = root.findall(".//svg:path[@data-edge]", NS)
            self.assertEqual([(int(p.attrib["data-source"]), int(p.attrib["data-target"]))
                              for p in paths], expected)
            self.assertEqual([p.find("svg:title", NS).text for p in paths],
                             [a for row in rows for a, _ in row["edges"]])
        self.assertEqual(result["metadata"]["verdict"]["evidence"], "certified winning strategy")
        knowledge = result["metadata"]["knowledge"]
        root = ET.fromstring(result["knowledge_svg"])
        self.assertEqual(len(root.findall(".//svg:g", NS)), len(knowledge["graph"]["nodes"]))
        self.assertEqual(len(root.findall(".//svg:path[@data-edge]", NS)), len(knowledge["edges"]))

    def test_checked_beliefs_cannot_erase_possible_worlds(self):
        model = arena(); certificate = solve(parse_arena(model))
        tampered = copy.deepcopy(certificate)
        root = tampered["knowledge"]["roots"][0]
        tampered["knowledge"]["nodes"][root]["belief"] = [0]
        with self.assertRaises(ValueError):
            diagrams.render(model, certificate=tampered)
        tampered = copy.deepcopy(certificate)
        tampered["certificate"]["winning"] = []
        with self.assertRaises(ValueError):
            diagrams.render(model, certificate=tampered)

    def test_model_identity_does_not_follow_a_different_certificate(self):
        model = arena(); certificate = solve(parse_arena(model))
        model["nodes"][0]["edges"][0][1] = 3
        with self.assertRaisesRegex(ValueError, "subject mismatch"):
            diagrams.render(model, certificate=certificate)

    def test_render_is_deterministic_and_does_not_mutate_or_alias_input(self):
        model = arena(); certificate = solve(parse_arena(model))
        before = copy.deepcopy((model, certificate))
        one = diagrams.render(model, certificate=certificate)
        self.assertEqual(one, diagrams.render(model, certificate=certificate))
        self.assertEqual((model, certificate), before)
        one["metadata"]["knowledge"]["graph"]["nodes"][0]["belief"].clear()
        self.assertEqual((model, certificate), before)

    def test_xml_is_escaped_and_complete_labels_are_retained(self):
        model = arena()
        model["nodes"][0]["observation"] = model["nodes"][1]["observation"] = "<unknown&>"
        labels = {i: '<label "&">' + str(i) for i in range(4)}
        result = diagrams.render(model, labels)
        root = ET.fromstring(result["arena_svg"])
        self.assertIn(labels[0], "".join(root.itertext()))
        self.assertEqual(result["metadata"]["labels"]["0"], labels[0])
        self.assertIsNone(result["knowledge_svg"])
        with self.assertRaises(ValueError):
            diagrams.render(model, {0: "incomplete"})
        labels[0] = "\0"
        with self.assertRaises(ValueError):
            diagrams.render(model, labels)

    def test_finite_render_bounds_and_supported_objective(self):
        model = arena()
        model["nodes"] += [dict(owner=1, observation="extra", edges=[["hold", 4]])] * 61
        with self.assertRaisesRegex(ValueError, "node bound"):
            diagrams.render(model)
        model = arena()
        model["observation"] = "perfect"
        model["objective"]["kind"] = "reachability"
        with self.assertRaisesRegex(ValueError, "safety arenas"):
            diagrams.render(model)
        model = arena()
        model["nodes"][2]["edges"] = [[f"hold{i}", 2] for i in range(256)]
        with self.assertRaisesRegex(ValueError, "edge bound"):
            diagrams.render(model)

    def test_real_zal_profiles_bind_the_model_and_round_trip_english(self):
        for profile_name, source_name, expected in (
                ("delivery.partial", "delivery", "certified unrealizability"),
                ("delivery.perfect", "delivery", "certified winning strategy"),
                ("delivery_idempotent.partial", "delivery_idempotent", "certified winning strategy"),
                ("delivery_idempotent.cost", "delivery_idempotent", "certified unrealizability")):
            with self.subTest(profile=profile_name):
                source = FIXTURES / (source_name + ".zal")
                profile = FIXTURES / (profile_name + ".json")
                result, english = diagrams.artifacts(source, profile)
                model = behavior.parse(source.read_text())
                self.assertEqual(behavior.parse(english, "english"), model)
                self.assertEqual(result["metadata"]["model_revision"], model.revision)
                self.assertEqual(result["metadata"]["verdict"]["evidence"], expected)
                self.assertEqual(result["metadata"]["arena"], result["metadata"]["compilation"]["arena"])

    def test_generation_check_detects_modified_diagram_bytes(self):
        with tempfile.TemporaryDirectory() as tmp:
            argv = [str(FIXTURES / "delivery_idempotent.zal"), "--profile",
                    str(FIXTURES / "delivery_idempotent.partial.json"), "--out", tmp]
            self.assertEqual(diagrams.main(argv), 0)
            self.assertEqual(diagrams.main([*argv, "--check"]), 0)
            path = Path(tmp) / "delivery.arena.svg"
            path.write_text(path.read_text().replace("controller", "changed", 1))
            self.assertEqual(diagrams.main([*argv, "--check"]), 2)

    def test_relational_diagram_contains_exact_replayed_two_run_witness(self):
        source = FIXTURES / "relational_delayed_leak.zal"
        profile_path = FIXTURES / "relational_delayed_leak.json"
        model = behavior2.parse(source.read_text())
        profile = relational.parse_profile(json.loads(profile_path.read_text()), model)
        report = relational.check(model, profile)
        result = diagrams.render_relational(model, profile, report)
        self.assertEqual(result["metadata"]["report"], report)
        root = ET.fromstring(result["svg"])
        witness = report["counterexample"]
        self.assertEqual(witness["length"], 2)
        self.assertEqual(len(root.findall(".//svg:g", NS)), 2 * (witness["length"] + 1))
        for side in ("left", "right"):
            paths = root.findall(f'.//svg:path[@data-run="{side}"]', NS)
            self.assertEqual([json.loads(p.find("svg:title", NS).text) for p in paths],
                             [step[side] for step in witness["steps"]])
        report["counterexample"]["steps"].pop()
        with self.assertRaisesRegex(ValueError, "report mismatch"):
            diagrams.render_relational(model, profile, report)

    def test_relational_diagram_does_not_invent_a_witness_for_positive_result(self):
        model = behavior2.parse((FIXTURES / "relational_private.zal").read_text())
        profile = relational.parse_profile(json.loads((FIXTURES / "relational_private.json").read_text()), model)
        with self.assertRaisesRegex(ValueError, "requires a counterexample"):
            diagrams.render_relational(model, profile)


    def test_xml_bmp_fffe_refused_but_supplementary_codepoints_retained(self):
        model = arena()
        labels = {i: "node" for i in range(len(model["nodes"]))}
        labels[0] = "\ufffe"
        with self.assertRaisesRegex(ValueError, "XML character"):
            diagrams.render(model, labels)
        labels[0] = "\U00010000\U0010ffff"
        result = diagrams.render(model, labels)
        root = ET.fromstring(result["arena_svg"])
        self.assertIn(labels[0], "".join(root.itertext()))

    def test_xml_bmp_ffff_refused(self):
        model = arena()
        labels = {i: "node" for i in range(len(model["nodes"]))}
        labels[0] = "\uffff"
        with self.assertRaisesRegex(ValueError, "XML character"):
            diagrams.render(model, labels)


if __name__ == "__main__":
    unittest.main()
