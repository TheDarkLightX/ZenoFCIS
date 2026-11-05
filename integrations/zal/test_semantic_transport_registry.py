"""Negative and transfer controls for first-class finite semantic transports."""
from dataclasses import replace
from itertools import product
import unittest

import semantic_transport_registry as s


def goal(name):
    if name in ("xor", "xnor"):
        rows = tuple(product((0, 1), repeat=2))
        labels = tuple((x != y) if name == "xor" else (x == y) for x, y in rows)
    elif name in ("radial", "radial_new", "oblique", "oblique_new"):
        wide = name.endswith("_new")
        rows = tuple(product(range(-3 if wide else -2, 4 if wide else 3), repeat=2))
        cutoff = 9 if wide else 4
        labels = tuple((x*x + y*y >= cutoff) if name.startswith("radial")
                       else ((x+y)*(x+y) >= cutoff) for x, y in rows)
    else:
        raise ValueError("test fixture")
    return s.Goal(s.Domain(rows), labels)


class FiniteSemanticTransportTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.examples = {name: (goal(name), s.discover(goal(name), max_steps=2))
                        for name in ("xor", "radial", "oblique")}

    def test_discovers_three_distinct_compositions_with_independent_replay(self):
        expected = {
            "xor": (("append-mul", (0, 1)),),
            "radial": (("append-mul", (0, 0)), ("append-mul", (1, 1))),
            "oblique": (("append-add", (0, 1)), ("append-mul", (2, 2))),
        }
        for name, (task, result) in self.examples.items():
            with self.subTest(problem=name):
                self.assertEqual(result.status, "finite-replayed")
                self.assertFalse(result.complete)  # a witness is not global optimality
                self.assertEqual(tuple((edge.rule.op, edge.rule.args) for edge in result.path), expected[name])
                self.assertEqual(result.certificate.checked_rows, len(task.source.rows))
                self.assertTrue(s.check_certificate(task, result.path, result.certificate))
                self.assertGreater(result.visited, 1)
                self.assertGreater(result.edges_checked, 1)
                self.assertTrue(all(s.check_edge(edge) for edge in result.path))

    def test_embedding_vs_property_quotient_have_distinct_assurance(self):
        source = s.Domain(((0, 0), (0, 1), (1, 0), (1, 1)))
        lift = s.propose_edge("lift", source, s.Rule("append-mul", (0, 1)))
        self.assertEqual(s.edge_kind(lift), "finite-bijection")
        extended = s.propose_edge("emb", source, s.Rule("append-mul", (0, 1)), ((1, 1, 0),))
        self.assertEqual(s.edge_kind(extended), "finite-embedding")
        quotient = s.propose_edge("drop_y", source, s.Rule("project", (0,)))
        self.assertEqual(s.edge_kind(quotient), "finite-quotient")
        self.assertTrue(s.check_edge(quotient))
        self.assertTrue(s.check_edge(extended))

    def test_quotient_can_be_exact_for_one_property_but_not_another(self):
        rows = ((0, 0), (0, 1), (1, 0), (1, 1))
        source = s.Domain(rows)
        to_x = s.propose_edge("to-x", source, s.Rule("project", (0,)))
        x_goal = s.Goal(source, tuple(bool(x) for x, _ in rows))
        carried, loss = s.transport_labels(x_goal, (to_x,))
        self.assertIsNone(loss)
        self.assertEqual(len(carried[2]), 2)
        xor, _ = self.examples["xor"]
        carried, loss = s.transport_labels(xor, (to_x,))
        self.assertIsNone(carried)
        self.assertEqual(loss.reason, "property-lost-under-quotient")
        self.assertEqual(xor.labels[loss.source_rows[0]], False)
        self.assertEqual(xor.labels[loss.source_rows[1]], True)

    def test_semantics_change_integer_to_modular_equality_refuted(self):
        # 0 != 97 over Z, but they are identical residues over F_97.
        source = s.Domain(((0,), (97,)))
        eq_zero = s.Goal(source, (True, False))
        to_mod = s.propose_edge("reduce", source, s.Rule("mod-reduce", (97,)))
        self.assertEqual(s.edge_kind(to_mod), "finite-quotient")
        carried, loss = s.transport_labels(eq_zero, (to_mod,))
        self.assertIsNone(carried)
        self.assertEqual(loss.source_rows, (0, 1))
        with self.assertRaises(ValueError):
            s.certify(eq_zero, (to_mod,), s.Separator((1,), 0))

    def test_composing_and_replaying_explicit_projection_after_lift(self):
        target, _ = self.examples["oblique"]
        raw = target.source
        add = s.propose_edge("sum", raw, s.Rule("append-add", (0, 1)))
        square = s.propose_edge("square", add.target, s.Rule("append-mul", (2, 2)))
        project_square = s.propose_edge("project", square.target, s.Rule("project", (3,)))
        self.assertEqual(project_square.target.arity, 1)
        self.assertEqual(s.edge_kind(project_square), "finite-quotient")
        path = (add, square, project_square)
        transported, loss = s.transport_labels(target, path)
        self.assertIsNone(loss)
        domain, _, reached = transported
        fitted = s.fit_separator(domain, reached)
        self.assertIsNotNone(fitted)
        certificate = s.certify(target, path, fitted)
        self.assertTrue(s.check_certificate(target, path, certificate))
        self.assertEqual(certificate.checked_rows, 25)

    def test_registry_is_immutable_and_revision_bound(self):
        task, search = self.examples["radial"]
        empty = s.Registry()
        registry = empty
        for edge in search.path:
            registry = registry.with_edge(edge)
        self.assertEqual(len(empty.edges), 0)
        self.assertEqual(len(registry.edges), 2)
        self.assertEqual(registry.resolve(search.certificate.edge_hashes), search.path)
        evidence = s.register_certificate(registry, search.certificate, task)
        self.assertTrue(s.replay_registered(task, registry, evidence))
        self.assertFalse(s.replay_registered(task, empty, evidence))
        self.assertFalse(s.replay_registered(task, registry, replace(evidence, registry_hash="0"*64)))
        self.assertFalse(s.replay_registered(task, registry,
                         replace(evidence, certificate=replace(search.certificate, goal_hash="0"*64))))
        with self.assertRaises(ValueError):
            registry.with_edge(search.path[0])
        with self.assertRaises(ValueError):
            registry.resolve((search.path[-1].identity(), search.path[0].identity()))

    def test_registry_refuses_forged_semantics_and_duplicate_labels(self):
        task, search = self.examples["xor"]
        original = search.path[0]
        bad_map = replace(original, mapping=tuple(reversed(original.mapping)))
        self.assertFalse(s.check_edge(bad_map))
        with self.assertRaises(ValueError):
            s.Registry((bad_map,))
        bad_target = replace(original, target=s.Domain(((0,0,0),)))
        self.assertFalse(s.check_edge(bad_target))
        with self.assertRaises(ValueError):
            s.Registry((original, replace(original, rule=s.Rule("project", (0,)))))
        with self.assertRaises(ValueError):
            s.Registry((original, original))

    def test_goal_changes_stale_out_certificate(self):
        task, search = self.examples["xor"]
        opposite = goal("xnor")
        self.assertFalse(s.check_certificate(opposite, search.path, search.certificate))
        broken = replace(search.certificate, separator=replace(search.certificate.separator, bias=99))
        self.assertFalse(s.check_certificate(task, search.path, broken))
        broken = replace(search.certificate, edge_hashes=("unknown",))
        self.assertFalse(s.check_certificate(task, search.path, broken))
        broken = replace(search.certificate, authority="production")
        self.assertFalse(s.check_certificate(task, search.path, broken))
        mutated_edge = replace(search.path[0], label="different-label")
        self.assertFalse(s.check_certificate(task, (mutated_edge,), search.certificate))

    def test_frozen_rule_transfer_and_refitting_on_new_domains(self):
        scenarios = (("xor", "xnor", 4), ("radial", "radial_new", 49),
                     ("oblique", "oblique_new", 49))
        for original, updated, count in scenarios:
            with self.subTest(transfer=updated):
                _, discovered = self.examples[original]
                new_goal = goal(updated)
                transferred, loss = s.reuse_rules(discovered, new_goal)
                self.assertIsNone(loss)
                self.assertIsNotNone(transferred)
                path, cert = transferred
                self.assertEqual(tuple(e.rule for e in path), tuple(e.rule for e in discovered.path))
                self.assertTrue(s.check_certificate(new_goal, path, cert))
                self.assertEqual(cert.checked_rows, count)
                self.assertNotEqual(cert.goal_hash, discovered.certificate.goal_hash)

    def test_mismatched_transfer_and_nonqualified_source_refuse(self):
        _, original = self.examples["xor"]
        with self.assertRaises(ValueError):
            s.reuse_rules(replace(original, status="inconclusive-budget"), goal("xnor"))
        unrelated = s.Goal(s.Domain(((0,), (1,))), (False, True))
        with self.assertRaises(ValueError):
            s.reuse_rules(original, unrelated)

    def test_search_budget_and_grammar_depth_do_not_overclaim(self):
        task, _ = self.examples["xor"]
        stopped = s.discover(task, max_steps=2, max_visits=1)
        self.assertEqual(stopped.status, "inconclusive-budget")
        self.assertFalse(stopped.complete)
        self.assertGreater(stopped.edges_checked, 0)
        self.assertIsNone(stopped.certificate)
        incomplete = s.discover(task, max_steps=0)
        self.assertEqual(incomplete.status, "no-solution-within-grammar")
        self.assertTrue(incomplete.complete)
        self.assertIsNone(incomplete.certificate)

    def test_invalid_inputs_fail_closed(self):
        for rows in ((((0,), (0,)),), ((True,), (0,)), ((257,),), ((0,0,0,0,0),)):
            with self.subTest(rows=rows), self.assertRaises(ValueError):
                s.Domain(tuple(rows))
        with self.assertRaises(ValueError):
            s.Goal(s.Domain(((0,), (1,))), (0, 1))
        with self.assertRaises(ValueError):
            s.Rule("append-mul", (1, 0)).validate(2)
        with self.assertRaises(ValueError):
            s.Rule("axiom-change", ()).validate(2)
        with self.assertRaises(ValueError):
            s.propose_edge("X", s.Domain(((0,), (1,))), s.Rule("append-add", (0, 0)),
                           ((False, 1),))
        with self.assertRaises(ValueError):
            s.discover(goal("xor"), max_steps=4)

    def test_all_fourteen_nonconstant_two_input_boolean_functions(self):
        rows = tuple(product((0, 1), repeat=2))
        for truth_mask in range(1, 15):
            with self.subTest(truth_mask=truth_mask):
                g = s.Goal(s.Domain(rows), tuple(bool((truth_mask >> i) & 1)
                                                 for i in range(4)))
                result = s.discover(g, max_steps=1)
                self.assertEqual(result.status, "finite-replayed")
                self.assertTrue(s.check_certificate(g, result.path, result.certificate))
                if truth_mask in (6, 9):
                    self.assertEqual(len(result.path), 1)
                    self.assertEqual(result.path[0].rule, s.Rule("append-mul", (0, 1)))
                else:
                    self.assertEqual(len(result.path), 0)

    def test_all_property_quotient_fibers_against_independent_oracle(self):
        rows = tuple(product((0, 1), repeat=2))
        domain = s.Domain(rows)
        project_x = s.propose_edge("x", domain, s.Rule("project", (0,)))
        for mask in range(1, 15):
            g = s.Goal(domain, tuple(bool((mask >> i) & 1) for i in range(4)))
            transported, loss = s.transport_labels(g, (project_x,))
            # In this row order, x=0 pairs indices 0,1; x=1 indices 2,3.
            safe = g.labels[0] == g.labels[1] and g.labels[2] == g.labels[3]
            self.assertEqual(loss is None, safe)
            self.assertEqual(transported is not None, safe)

    def test_forged_nested_edge_must_refuse_not_crash(self):
        g, result = self.examples["xor"]
        bad = replace(result.path[0], rule="forgery")
        self.assertFalse(s.check_certificate(g, (bad,), result.certificate))
        self.assertFalse(s.check_edge(bad))

    def test_no_mutable_aliases_or_deployment_authority(self):
        task, result = self.examples["oblique"]
        self.assertEqual(result.certificate.authority, "none")
        self.assertIsInstance(result.path, tuple)
        self.assertIsInstance(task.source.rows, tuple)
        with self.assertRaises(AttributeError):
            result.path[0].mapping = (1,)
        self.assertFalse(s.check_certificate(task, ("forged",), result.certificate))


if __name__ == "__main__":
    unittest.main()
