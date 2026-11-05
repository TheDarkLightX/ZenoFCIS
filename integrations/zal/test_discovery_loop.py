"""Independent finite oracles and negative controls for discovery_loop."""
import unittest
from dataclasses import replace
from itertools import product

import discovery_loop as d


class DiscoveryLoopTests(unittest.TestCase):
    def test_counterexample_loop_finds_exact_two_quantifier_laws(self):
        for mode, expected in (("exists", 14), ("forall", 8)):
            with self.subTest(mode=mode):
                p = d.discover(mode)
                self.assertEqual(p.mask, expected)
                self.assertTrue(d.check_procedure(p))
                self.assertIsNone(p.attempts[-1].counterexample)
                self.assertGreater(len(p.attempts), 1)
                for trial in p.attempts[:-1]:
                    a, b = trial.counterexample
                    self.assertNotEqual(d._binary(trial.mask, a, b),
                                        any((a, b)) if mode == "exists" else all((a, b)))

    def test_exhaustive_projection_of_every_three_variable_boolean_function(self):
        # 2^(2^3) = 256 total relations, 2 modes, 3 hidden variables.
        ps = {mode: d.discover(mode) for mode in d.MODES}
        checks = 0
        for bits in range(256):
            table = d.Table(3, bits)
            for mode, variable in product(d.MODES, range(3)):
                receipt = d.project(table, variable, ps[mode])
                self.assertTrue(d.replay_projection(table, receipt))
                for row in range(4):
                    def inserted(bit):
                        lo = row & ((1 << variable) - 1)
                        hi = row >> variable
                        return lo | (bit << variable) | (hi << (variable + 1))
                    samples = [bool(bits & (1 << inserted(bit))) for bit in (0, 1)]
                    wanted = any(samples) if mode == "exists" else all(samples)
                    self.assertEqual(receipt.output.at(row), wanted)
                    checks += 1
        self.assertEqual(checks, 6144)

    def test_searches_and_replays_smaller_representation_of_multiplexer(self):
        mux = d.Table.from_predicate(6, lambda row: (row >> ((row >> 4) & 3)) & 1)
        baseline = d.make_bdd(mux, tuple(range(6)))
        best, orders = d.search_representation(mux)
        self.assertEqual(orders, 720)
        self.assertTrue(d.check_bdd(mux, best))
        self.assertLess(len(best.nodes), len(baseline.nodes))
        self.assertEqual(len([best.at(x) for x in range(64)]), 64)
        self.assertNotEqual(best.order, tuple(range(6)))
        self.assertFalse(d.check_bdd(mux, replace(best, nodes=((0, 2, 1),), root=2)))

    def test_replay_rejects_bogus_transforms_tampered_outputs_and_stale_input(self):
        p = d.discover("exists")
        self.assertFalse(d.check_procedure(replace(p, mask=8)))
        source = d.Table(3, 0b10110011)
        receipt = d.project(source, 1, p)
        self.assertTrue(d.replay_projection(source, receipt))
        self.assertFalse(d.replay_projection(d.Table(3, source.bits ^ 1), receipt))
        self.assertFalse(d.replay_projection(source, replace(receipt, mask=8)))
        self.assertFalse(d.replay_projection(source, replace(receipt, output=d.Table(2, receipt.output.bits ^ 1))))
        self.assertFalse(d.replay_projection(source, replace(receipt, variable=3)))
        with self.assertRaises(ValueError):
            d.project(source, 1, replace(p, mask=8))

    def test_boundaries_refuse_incorrect_types_sizes_orders_and_modes(self):
        for bad in ((7, 0), (1, -1), (2, 1 << 4), (True, 0), (2, True)):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                d.Table(*bad)
        with self.assertRaises(ValueError):
            d.discover("maybe")
        with self.assertRaises(ValueError):
            d.project(d.Table(0, 1), 0, d.discover("exists"))
        with self.assertRaises(ValueError):
            d.make_bdd(d.Table(3, 1), (0, 0, 1))
        with self.assertRaises(ValueError):
            d.Table(2, 3).at(-1)

    def test_zero_variable_and_constant_relations(self):
        for b in (0, 1):
            table = d.Table(0, b)
            bdd, attempts = d.search_representation(table)
            self.assertEqual(attempts, 1)
            self.assertEqual(len(bdd.nodes), 0)
            self.assertTrue(d.check_bdd(table, bdd))
        for b in (0, (1 << 8) - 1):
            source = d.Table(3, b)
            for mode in d.MODES:
                result = d.project(source, 2, d.discover(mode))
                self.assertEqual(result.output.bits, 0 if not b else (1 << 4) - 1)

    def test_demo_report_has_no_proof_or_deployment_authority(self):
        report = d.demo()
        self.assertEqual(report["authority"], "none")
        self.assertGreaterEqual(report["projected_cases_replayed"], 1)
        self.assertLess(report["representation"]["chosen_nodes"],
                        report["representation"]["baseline_nodes"])


if __name__ == "__main__":
    unittest.main()
