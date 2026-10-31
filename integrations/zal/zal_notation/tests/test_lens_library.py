"""Library bindings and independent falsifiers; no application authority."""
from copy import deepcopy
from itertools import product
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from zal_notation import abstractions as A, builtin, lens_library as V, lenses as L, sharing
from zal_notation.core import NotationError, canonical, digest
from zal_notation.tests.test_lens_independent import machine


def library_entry(context=None, *, identifier="object", story="Exact original source: 1e-400, 1 and true."):
    if context is None:
        context = L.make_source()
    notation = builtin.generic_pack(context["grammar"])
    entry = A.make_entry(identifier, identifier, story=story,
                         discussion=sharing.make_discussion(context, notation))
    return A.register(A.new_library(), entry), digest(entry)


class LensLibraryTests(unittest.TestCase):
    def setUp(self):
        self.library, self.identity = library_entry()

    def test_builtin_catalogs_own_their_mutable_fields(self):
        first = V.default_registry()
        before = canonical(first)
        adapter_assumptions = deepcopy(L.ASSUMPTIONS)
        for row in first["definitions"]:
            for field in ("assumptions", "preserves", "forgets", "adds", "profile_hints"):
                row[field].append("Consumer-local edit")
        self.assertEqual(canonical(V.default_registry()), before)
        self.assertEqual(L.ASSUMPTIONS, adapter_assumptions)

    def test_same_object_has_structural_and_finite_candidates_without_proof_promotion(self):
        report = A.enumerate_lenses(self.library, self.identity)
        self.assertEqual(report["entry_digest"], self.identity)
        self.assertEqual(report["source_entry"], self.library["entries"][0])
        self.assertFalse(report["universal_completeness"])
        rows = {r["definition"]["id"]: r for r in report["lenses"]}
        self.assertEqual(rows["inductive-syntax"]["status"], "candidate")
        self.assertEqual(rows["relational-graph"]["status"], "conditional")
        self.assertEqual(rows["sequence-chunks"]["status"], "pending")
        self.assertEqual(rows["execution-tree"]["status"], "unsupported")
        self.assertTrue(all(r["evidence"] == "not-checked" for r in rows.values()))
        self.assertNotIn("checked", {r["status"] for r in rows.values()})

    def test_all_existing_counter_views_replay_through_entry_binding(self):
        before = deepcopy(self.library)
        results = {}
        for lid in L.SPECS:
            if lid in ("execution-tree", "automorphism-group"):
                continue
            app = V.apply(self.library, self.identity, lid)
            receipt = V.check(self.library, self.identity, app)
            results[lid] = receipt["status"]
            self.assertTrue(V.verify_receipt(self.library, self.identity, app, receipt))
            self.assertEqual(V.restore(self.library, self.identity, app), before["entries"][0])
        self.assertEqual(sum(s == "checked" for s in results.values()), 8)
        self.assertEqual(results["all-effects-group"], "refuted")
        self.assertEqual(self.library, before)

    def test_source_syntax_is_not_a_terminated_execution_tree(self):
        app = V.apply(self.library, self.identity, "inductive-syntax")
        receipt = V.check(self.library, self.identity, app)
        self.assertEqual(receipt["status"], "checked")
        self.assertIn("no execution-tree", receipt["evidence"]["claim"])
        graph = L.apply(L.make_source(), "state-machine")["target_context"]["model"]
        successors = {e["source"][0]: e["target"][0] for e in graph["body"]["edges"]
                      if json.loads(e["fields"]["event"])["event"] == "inc"}
        state = "unseen"
        for _ in range(4):
            state = successors[state]
        self.assertEqual(state, "unseen")
        with self.assertRaises(NotationError):
            V.apply(self.library, self.identity, "execution-tree")

    def test_custom_profile_has_checked_structure_without_borrowing_finite_semantics(self):
        context = L.make_source()
        context["semantics"] = {"profile": "user/unbounded-program/1", "parameters": {"exact": "1/3"},
                                "description": "Author-supplied custom semantics, unproved."}
        context["assumptions"] = ["The author has not established termination."]
        library, identity = library_entry(context)
        app = V.apply(library, identity, "inductive-syntax")
        self.assertEqual(V.check(library, identity, app)["status"], "checked")
        self.assertEqual(app["payload"]["target_context"], context)
        rows = {r["definition"]["id"]: r["status"] for r in V.discover(library, identity)["lenses"]}
        self.assertEqual(rows["unit-group"], "unsupported")
        with self.assertRaises(NotationError):
            V.apply(library, identity, "unit-group")

    def test_graph_structure_accepts_custom_interpretation_without_execution(self):
        grammar, notation, model = builtin.get_builtin("state-machine")
        context = sharing.make_context(grammar, model,
            semantics={"profile": "user/social-relations/1", "parameters": {}, "description": "Names are relations, not machine execution."},
            assumptions=[], observation={"id": "graph", "description": "Every declared node and edge.", "keeps": ["graph"], "omits": []})
        library, identity = library_entry(context)
        application = V.apply(library, identity, "relational-graph")
        self.assertEqual(V.check(library, identity, application)["status"], "checked")

    def test_informal_draft_and_alternatives_remain_exact(self):
        raw = "Maybe a tree; maybe a sequence. Exact source 1e-400."
        entry = A.make_entry("informal", "Unsettled object", story=raw,
            examples=["Alternative: non-hierarchical chunks", "Alternative: graph"],
            requirements=[{"id": "intent", "statement": "Meaning is unresolved.", "anchors": []}],
            open_questions=["Which distinctions matter?"])
        library = A.register(A.new_library(), entry)
        result = V.discover(library, digest(entry))
        self.assertTrue(all(row["status"] == "pending" for row in result["lenses"]))
        self.assertEqual(result["entry_status"]["status"], "draft")
        self.assertEqual(result["source_entry"], entry)
        result["source_entry"]["examples"].clear()
        self.assertEqual(library["entries"][0], entry)

    def test_registered_definitions_are_pending_or_analogy_not_self_certified(self):
        for mode, expected in (("construction", "pending"), ("analogy", "analogy")):
            proposed = V.definition("custom", "A possible view", summary="An author proposal.",
                assumptions=["Additional mathematics is needed."], preserves=["alleged property"],
                forgets=["unknown"], adds=[], profile_hints=[L.PROFILE], mode=mode)
            registry = sharing.dispatch_tool("zal_library_register_lens", {"registry": V.new_registry(), "proposed": proposed})
            result = sharing.dispatch_tool("zal_library_lenses", {"library": self.library,
                          "entry_digest": self.identity, "registry": registry})
            self.assertEqual(result["lenses"][0]["status"], expected)
            with self.assertRaises(NotationError):
                V.apply(self.library, self.identity, "custom", registry=registry)
            for key, value in (("status", "checked"), ("module", "os"), ("code", "raise SystemExit")):
                changed = {**proposed, key: value}
                with self.assertRaises(NotationError):
                    V.register(V.new_registry(), changed)

    def test_spoofed_registered_definition_cannot_borrow_a_builtin_checker(self):
        registry = V.default_registry()
        row = next(r for r in registry["definitions"] if r["id"] == "unit-group")
        row["preserves"] = ["everything about program execution"]
        result = V.discover(self.library, self.identity, registry)
        found = next(r for r in result["lenses"] if r["definition"]["id"] == "unit-group")
        self.assertEqual(found["status"], "pending")
        with self.assertRaises(NotationError):
            V.apply(self.library, self.identity, "unit-group", registry=registry)

    def test_relabelled_payload_cannot_check_as_another_lens(self):
        app = V.apply(self.library, self.identity, "transformation-monoid")
        target = next(r for r in V.default_registry()["definitions"] if r["id"] == "all-effects-group")
        app["definition"], app["definition_digest"] = target, digest(target)
        result = V.check(self.library, self.identity, app)
        self.assertEqual(result["status"], "unsupported")
        self.assertIn("payload lens", result["reason"])

    def test_source_revision_and_metadata_cannot_reuse_old_entry_receipt(self):
        app = V.apply(self.library, self.identity, "unit-group")
        receipt = V.check(self.library, self.identity, app)
        altered = deepcopy(self.library)
        altered["entries"][0]["story"] += " An added interpretation."
        new_id = digest(altered["entries"][0])
        self.assertEqual(V.check(altered, new_id, app)["status"], "unsupported")
        fresh = V.apply(altered, new_id, "unit-group")
        self.assertFalse(V.verify_receipt(altered, new_id, fresh, receipt))
        with self.assertRaises(NotationError):
            V.discover(altered, self.identity)

    def test_aliases_and_exact_numeric_types_are_retained(self):
        app = V.apply(self.library, self.identity, "transformation-monoid")
        before = deepcopy(app)
        restored = V.restore(self.library, self.identity, app)
        restored["story"] = "Consumer edit"
        self.assertEqual(app, before)
        self.assertEqual(app["source_entry"]["story"], "Exact original source: 1e-400, 1 and true.")
        app["payload"]["payload"]["algebra"]["elements"][0][0] = False
        self.assertEqual(V.check(self.library, self.identity, app)["status"], "unsupported")
        self.assertNotEqual(canonical({"x": True}), canonical({"x": 1}))
        self.assertNotEqual(canonical({"x": 1.0}), canonical({"x": 1}))

    def test_invalid_budgets_and_zero_work_stay_unchecked(self):
        for lid in ("inductive-syntax", "unit-group"):
            app = V.apply(self.library, self.identity, lid)
            self.assertEqual(V.check(self.library, self.identity, app, 0)["status"], "incomplete")
            for budget in (True, 1.0, -1, 1000001, None):
                self.assertEqual(V.check(self.library, self.identity, app, budget)["status"], "unsupported")

    def test_general_authoring_works_without_optional_finite_adapter(self):
        root = Path(__file__).resolve().parents[1]
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp)
            shutil.copytree(root, target / "zal_notation", ignore=shutil.ignore_patterns("__pycache__"))
            code = "import unittest; from zal_notation import lens_library as V, abstractions as A; from zal_notation.core import digest; check=unittest.TestCase(); e=A.make_entry('draft','Draft',story='An informal object.'); lib=A.register(A.new_library(),e); r=V.discover(lib,digest(e)); check.assertEqual({x['definition']['id'] for x in r['lenses']},{'inductive-syntax','relational-graph','sequence-chunks','typed-category'}); check.assertTrue(all(x['status']=='pending' for x in r['lenses']))"
            completed = subprocess.run([sys.executable, "-B", "-c", code], cwd=target, text=True,
                                       capture_output=True, timeout=10)
            self.assertEqual(completed.returncode, 0, completed.stderr)

    def test_legacy_library_schema_and_selection_are_unchanged(self):
        self.assertEqual(set(self.library), {"schema", "label", "entries", "relations"})
        self.assertEqual(A.select(self.library, [self.identity]), self.library)
        self.assertEqual(self.library["schema"], "zal.abstraction-library/1")
        names = {row["name"] for row in sharing.toolkit()["tools"]}
        self.assertIn("zal_library_lenses", names)
        self.assertIn("zal_library_register_lens", names)
        self.assertNotIn("MCP", sharing.toolkit()["transport"])

    def test_fresh_process_dependency_change_invalidates_a_prior_application(self):
        root = Path(__file__).resolve().parents[2]
        application = V.apply(self.library, self.identity, "unit-group")
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp)
            for name in ("zal_notation", "finite_library"):
                shutil.copytree(root / name, target / name, ignore=shutil.ignore_patterns("__pycache__"))
            request = {"library": self.library, "entry_digest": self.identity, "application": application}
            (target / "request.json").write_text(json.dumps(request))
            code = "import json; from zal_notation import lens_library as V; p=json.load(open('request.json')); print(json.dumps(V.check(**p)))"
            before = subprocess.run([sys.executable, "-B", "-c", code], cwd=target,
                                    capture_output=True, text=True, timeout=10)
            self.assertEqual(before.returncode, 0, before.stderr)
            self.assertEqual(json.loads(before.stdout)["status"], "checked")
            dependency = target / "finite_library/prototype.py"
            dependency.write_text(dependency.read_text() + "\n# Isolated source-identity drift control.\n")
            after = subprocess.run([sys.executable, "-B", "-c", code], cwd=target,
                                   capture_output=True, text=True, timeout=10)
            self.assertEqual(after.returncode, 0, after.stderr)
            self.assertEqual(json.loads(after.stdout)["status"], "unsupported")

    def test_all_27_three_state_functions_match_independent_bijection_oracle(self):
        template = machine(3, reset=False)
        states = template.inputs[0].values
        checked = refuted = 0
        for images in product(states, repeat=3):
            rows = {(s, "inc", "yes"): (("ok", t, "done"),) for s, t in zip(states, images)}
            contract = L.finite.build(template.inputs, template.outputs, rows, template.state)
            source = L.make_source(contract)
            result = L.check(source, L.apply(source, "all-effects-group"))
            # Pairwise injectivity and full finite image, independent of the
            # production closure/composition/unit implementation.
            bijective = all(images[i] != images[j] for i in range(3) for j in range(i)) and set(images) == set(states)
            self.assertEqual(result["status"], "checked" if bijective else "refuted")
            checked += result["status"] == "checked"
            refuted += result["status"] == "refuted"
        self.assertEqual((checked, refuted), (6, 21))

    def test_informal_proposed_view_retains_both_drafts_and_declared_map(self):
        source = A.make_entry("idea", "Idea", story="A program, perhaps a tree? 1e-400.",
                              open_questions=["What does execution mean?"], examples=["Tree", "Sequence"])
        target = A.make_entry("chunks", "Chunk interpretation", story="A proposed sequence, not a settled meaning.")
        library = A.register(A.new_library(), source)
        relation = {"description": "Proposed: source story corresponds to target story.",
                    "pairs": [{"source": "story", "target": "story"}]}
        result = V.propose(library, digest(source), "sequence-chunks", target, relation)
        self.assertEqual(result["status"], "proposed")
        self.assertEqual(result["source_status"]["status"], "draft")
        self.assertEqual(result["target_status"]["status"], "draft")
        self.assertEqual(result["source_entry"], source)
        self.assertEqual(result["target_entry"], target)
        self.assertEqual(result["correspondence"], relation)
        self.assertEqual(result["adoption"], "none")
        self.assertEqual(V.check(library, digest(source), result)["status"], "unsupported")
        result["source_entry"]["examples"].clear()
        result["target_entry"]["story"] = "Changed"
        relation["description"] = "Changed"
        self.assertEqual(library["entries"][0], source)
        self.assertNotEqual(result["target_entry"], target)

    def test_analogy_proposal_is_inert_even_with_code_like_text(self):
        proposed = V.definition("analogy", "An analogy", summary="An unproved correspondence.",
            assumptions=["A carrier remains unspecified."], preserves=[], forgets=[], adds=[], mode="analogy")
        registry = V.register(V.new_registry(), proposed)
        target = A.make_entry("other", "Other", story="An authored target.")
        with tempfile.TemporaryDirectory() as tmp:
            marker = Path(tmp) / "executed"
            relation = {"description": f"__import__('pathlib').Path({str(marker)!r}).write_text('wrong')", "pairs": []}
            packet = sharing.dispatch_tool("zal_library_lens_propose", {"library": self.library,
                "entry_digest": self.identity, "lens_id": "analogy", "target_entry": target,
                "correspondence": relation, "registry": registry})
            self.assertEqual(packet["status"], "analogy")
            self.assertFalse(marker.exists())
            self.assertEqual(packet["correspondence"], relation)

    def test_proposal_rejects_executable_fields_and_numeric_label_aliases(self):
        target = A.make_entry("other", "Other", story="An authored target.")
        for relation in ({"description": "Map", "pairs": [], "module": "os"},
                         {"description": "Map", "pairs": [{"source": True, "target": "story"}]},
                         {"description": "Map", "pairs": [{"source": "story", "target": 1}]}):
            with self.assertRaises(NotationError):
                V.propose(self.library, self.identity, "sequence-chunks", target, relation)

    def test_general_toolkit_executes_actual_structural_and_refuted_entry_flows(self):
        base = {"library": self.library, "entry_digest": self.identity}
        for lid, status in (("inductive-syntax", "checked"), ("all-effects-group", "refuted")):
            app = sharing.dispatch_tool("zal_library_lens_apply", {**base, "lens_id": lid})
            receipt = sharing.dispatch_tool("zal_library_lens_check", {**base, "application": app})
            self.assertEqual(receipt["status"], status)
            valid = sharing.dispatch_tool("zal_library_lens_verify", {**base, "application": app, "receipt": receipt})
            self.assertTrue(valid)
            restored = sharing.dispatch_tool("zal_library_lens_restore", {**base, "application": app})
            self.assertEqual(restored, self.library["entries"][0])
        for name in ("zal_library_lens_unknown", "zal_library_lens_import"):
            with self.assertRaises(NotationError):
                sharing.dispatch_tool(name, {"module": "os"})

    def test_registry_membership_is_discovery_provenance_not_admission(self):
        app = V.apply(self.library, self.identity, "unit-group")
        catalog = V.discover(self.library, self.identity, V.new_registry())
        self.assertEqual(catalog["lenses"], [])
        self.assertEqual(V.check(self.library, self.identity, app)["status"], "checked")


if __name__ == "__main__":
    unittest.main()
