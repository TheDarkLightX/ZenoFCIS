"""Focused independent engine/plan cases; native Rust execution is separate."""
from copy import deepcopy
import re
import unittest
from unittest.mock import patch
import strategy_knowledge_reference as R


class KnowledgeReferenceTests(unittest.TestCase):
    def test_public_rust_inventory_signatures_and_named_plans_match_reference(self):
        shell = (R.EXAMPLE.parents[3] / "crates/zeno-fcis-cli/src/loop_strategy_knowledge.rs").read_text()
        self.assertNotIn('env!("CARGO_MANIFEST_DIR")', shell)
        source = (R.EXAMPLE.parents[1] / "src/strategy_algebra.rs").read_text()
        artifact_impl = source.split("impl Artifact {")[1].split("/// Fixed built-in")[0]
        artifact_names = dict(re.findall(r'Self::(\w+) => "([a-z-]+)"', artifact_impl))
        artifact_variants = re.findall(r"^\s*(\w+),$", source.split("pub enum Artifact {")[1].split("}")[0], re.M)
        self.assertEqual(set(artifact_names), set(artifact_variants))
        self.assertEqual(set(artifact_names.values()), R.ARTIFACTS)
        self.assertEqual(set(re.findall(r"Self::(\w+)", artifact_impl.split("pub const ALL:")[1].split("];")[0])), set(artifact_variants))
        step_impl = source.split("impl Step {")[1].split("/// Owned plan:")[0]
        names = dict(re.findall(r'Self::(\w+) => "([a-z-]+)"', step_impl))
        variants = re.findall(r"^\s*(\w+),$", source.split("pub enum Step {")[1].split("}")[0], re.M)
        self.assertEqual(set(names), set(variants))
        self.assertEqual(set(names.values()), set(R.SIGNATURES))
        self.assertEqual(set(re.findall(r"Self::(\w+)", step_impl.split("pub const ALL:")[1].split("];")[0])), set(variants))
        signatures = {}
        pattern = r"(Self::\w+(?:\s*\|\s*Self::\w+)*)\s*=>\s*(?:\{\s*)?\(\s*&\[([A-Za-z_,\s]*)\],\s*&\[([A-Za-z_,\s]*)\]\s*\)"
        for group, need, made in re.findall(pattern, step_impl.split("fn signature")[1]):
            inputs = {artifact_names[a.strip()] for a in need.split(",") if a.strip()}
            outputs = {artifact_names[a.strip()] for a in made.split(",") if a.strip()}
            for variant in re.findall(r"Self::(\w+)", group):
                signatures[names[variant]] = (inputs, outputs)
        self.assertEqual(signatures, R.SIGNATURES)
        factory = source.split("pub fn named(")[1].split("#[cfg(test)]")[0]
        self.assertEqual(set(re.findall(r'"([a-z-]+)" => Plan::new', factory)), set(R.STRATEGIES))
        for name, (needs, goals, used) in R.STRATEGIES.items():
            chunk = factory.split('"' + name + '" => Plan::new(')[1].split('),\n        "')[0].split("_ => return None")[0]
            n, g = re.findall(r"vec!\[([^\]]*)\]", chunk)[:2]
            self.assertEqual({artifact_names[a.strip()] for a in n.split(",") if a.strip()}, needs)
            self.assertEqual({artifact_names[a.strip()] for a in g.split(",") if a.strip()}, goals)
            self.assertEqual({names[s] for s in re.findall(r"step\(Step::(\w+)\)", chunk)}, used)

    def test_recursive_stratified_negation_comparisons_and_ground_query(self):
        kb = R.KB("edge(a,b). edge(b,c). edge(c,d). blocked(c). node(a). node(b). node(c). node(d). path(X,Y):-edge(X,Y). path(X,Z):-edge(X,Y),path(Y,Z). open(X,Y):-path(X,Y),not blocked(Y). other(X,Y):-node(X),node(Y),X!=Y.")
        self.assertEqual(len(kb.query("path(X,Y)")), 6)
        self.assertEqual(len(kb.query("other(X,Y)")), 12)
        self.assertEqual(kb.query('path(a,"d")'), [{}])
        self.assertEqual(kb.query("open(a,c)"), [])
        self.assertEqual(kb.query("open(a,d)"), [{}])

    def test_anonymous_not_repeated_variable_strings_integers_and_zero_arity(self):
        kb = R.KB('p("a",1). p(a,-2). p(a,a). zero. no(X):-p(X,_),not p(X,9). equal(X):-p(X,X).')
        self.assertEqual(kb.query("p(a,_)"), [{}])
        self.assertEqual(kb.query("equal(X)"), [{"X": "a"}])
        self.assertEqual(kb.query("no(X)"), [{"X": "a"}])
        self.assertEqual(kb.query("zero()"), [{}])
        self.assertEqual(R.KB('p("\\uD834\\uDD1E").').query("p(X)"), [{"X": "𝄞"}])

    def test_positive_body_is_solved_before_negation_or_comparison(self):
        self.assertEqual(R.KB("q(a). q(b). blocked(b). p(X):-not blocked(X),X!=b,q(X).").query("p(X)"), [{"X": "a"}])

    def test_unsafe_negation_cycles_arity_and_syntax_are_refused(self):
        for text in ("p(X).", "p(X):-not q(X).", "q(a). p(X):-q(X),not r(X). r(X):-q(X),not p(X).", "p(a). p(a,b).", "p(_):-q(a).", "p(a):-q(a),_=a.", 'p("\\uD800").', "p(9223372036854775808)."):
            with self.subTest(text=text), self.assertRaises(ValueError):
                R.KB(text)
        kb = R.KB("p(a).")
        for query in ("p(X). garbage", "p(X) q(X)", "p(X,Y)", "p(X).."):
            with self.subTest(query=query), self.assertRaises(ValueError):
                kb.query(query)

    def test_public_example_has_public_interfaces_and_small_usage_vocabulary(self):
        facts, rules = R.Parser(R.EXAMPLE.read_text()).program()
        self.assertEqual({pred for pred, _args in facts}, set(R.expected())|{"lesson","scope","strength","effect","evidence","isa","limit","blocker","cost","prefer","supersedes","conflict","record"})
        self.assertEqual({pred for (pred, _args), _body in rules}, {"feeds","conflict"})
        self.assertEqual(len(R.KB(R.EXAMPLE.read_text()).query("strategy(S)")), 3)
        self.assertEqual(R.check_code(R.KB(R.EXAMPLE.read_text())), [])
        # No external load exists in the engine; only this text is passed.
        with patch.object(R.Path, "read_text", side_effect=AssertionError("unexpected file")):
            kb = R.KB("loop(a).")
            self.assertEqual(kb.query("loop(X)"), [{"X": "a"}])

    def test_each_reserved_relation_is_load_bearing(self):
        public = R.EXAMPLE.read_text()
        changes = [("loop(sample).", ""), ("consumes(sample,original).", "consumes(sample,model)."),
                   ("produces(sample,examples).", "produces(sample,answer)."),
                   ("needs(diagnose,model).", "needs(diagnose,proposal)."),
                   ("goal(diagnose,answer).", "goal(diagnose,qualified)."),
                   ("uses(diagnose,isolate).", ""), ("strategy(diagnose).", ""),
                   ('artifact(model,"model").', 'artifact(model,1).')]
        for before, after in changes:
            with self.subTest(before=before):
                self.assertTrue(R.check_code(R.KB(public.replace(before, after))))
        with patch.object(R, "compare", return_value=True), self.assertRaises(AssertionError):
            R.self_test()

    def test_named_strategy_selection_and_shortest_public_type_path(self):
        kb = R.KB(R.EXAMPLE.read_text())
        cases = [({"model"}, {"answer"}, "diagnose", ["observe", "explain", "isolate"]),
                 ({"proposal", "model", "owner-gate"}, {"answer"}, "ask-when-it-matters", ["formalize-alternatives", "solve-every-reading", "decide"]),
                 ({"original", "vocabulary"}, {"qualified"}, "split-by-cases", ["sample", "learn-terms", "cover", "learn-split", "assemble", "check-program"])]
        for have, want, selected, steps in cases:
            p = R.plan(kb, have, want)
            self.assertEqual(p["selected_strategy"], selected)
            self.assertEqual(p["steps"], steps)
        self.assertEqual(R.plan(kb, {"qualified"}, {"qualified"})["steps"], [])
        self.assertIsNone(R.plan(kb, {"original", "proposal"}, {"qualified"})["selected_strategy"])

    def test_bounds_and_no_path_never_become_an_impossibility_claim(self):
        kb = R.KB(R.EXAMPLE.read_text())
        for have, want, steps, states in [({"original", "vocabulary"}, {"qualified"}, 1, 4096),
                                         ({"original", "vocabulary"}, {"qualified"}, 8, 1),
                                         ({"question"}, {"qualified"}, 8, 4096)]:
            with self.assertRaises(ValueError):
                R.plan(kb, have, want, steps, states)
        with self.assertRaises(ValueError):
            R.KB("a(x). b(X):-a(X).", work=0)
        with self.assertRaises(ValueError):
            R.KB("a(x). a(y).", rows=1)

    def test_owned_results_and_planted_query_or_plan_faults_are_rejected(self):
        from usage_knowledge_reference import Usage,validation_inputs
        kb = Usage([R.EXAMPLE.read_text()],validations=validation_inputs(R.EXAMPLE.read_text())).kb
        before = kb.query("strategy(S)")
        copy = deepcopy(before)
        copy[0]["S"] = "altered"
        self.assertNotEqual(before, copy)
        source = R.EXAMPLE.read_text()
        report = {"authority": "none", "status": "proposed-query",
                  "detail": {"rows": before, "attempts_spent": 0,
                             "derived_rows": sum(map(len, kb.db.values())),
                             "binding": {"knowledge_sha256": R.hashlib.sha256(source.encode()).hexdigest()}}}
        R.verify(R.EXAMPLE.read_text(), "query", report, "strategy(S)")
        report["detail"]["rows"] = copy
        with self.assertRaises(AssertionError):
            R.verify(R.EXAMPLE.read_text(), "query", report, "strategy(S)")
        R.self_test()


if __name__ == "__main__":
    unittest.main()
