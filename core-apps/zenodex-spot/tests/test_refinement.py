"""Fast controls for the native-to-contract refinement tooling.

The complete gate (Verus, Lean, finite domain, planted defects) is
tools/check_refinement.py; these tests keep its parts honest cheaply.
"""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
import gen_refinement_program
import native_program
import planted_defects
import refinement_contract as rc
from spot_reference import Command, Pool, World, step


class Refinement(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.text = native_program.SOURCE.read_text()
        cls.native = native_program.load_text(cls.text)

    def decide(self, world, command):
        return native_program.decide(self.native, rc.world_inputs(world, command))

    def test_program_shape(self):
        n = self.native
        self.assertEqual((len(n.nodes), len(n.roots), len(n.branches)), (1377, 57, 70))
        self.assertEqual([b.code for b in n.branches if b.accept], list(range(63, 70)))
        self.assertTrue(all(not (b.assignments or b.effects or b.outbox) for b in n.branches if not b.accept))

    def test_generated_sources_are_fresh(self):
        self.assertEqual(gen_refinement_program.stale(), [])

    def test_native_matches_reference_on_each_command(self):
        world = World(Pool(2000, 3000, 2449), (2000, 3000, 0, 0, 10 ** 9, 10 ** 9, 0, 0), (1449, 0, 1000))
        for command in (Command(1, a=1000), Command(2, direction=1, a=500, limit0=10 ** 9),
                        Command(3, a=1000, b=1500), Command(4, a=1000), Command(4, a=1000, actor=3)):
            accept, reason, post = self.decide(world, command)
            expected = step(world, command)
            self.assertEqual(accept, expected.reason is None, command)
            if accept:
                state = [post[f] for _, f in self.native.bindings[:40]]
                self.assertEqual(state, list(rc.state_values(expected.post)))
                self.assertEqual(rc.int_contract(rc.world_inputs(world, command)[:40], command.action,
                                                 command.actor, state), [])

    def test_contract_rendering_rejects_named_violations(self):
        world = World(Pool(2000, 3000, 2449), (2000, 3000, 0, 0, 10 ** 9, 10 ** 9, 0, 0), (1449, 0, 1000))
        command = Command(4, a=1000)
        _, _, post = self.decide(world, command)
        state = [post[f] for _, f in self.native.bindings[:40]]
        pre = rc.world_inputs(world, command)[:40]
        lost = list(state)
        lost[0] -= 1  # reserve falls without its balance
        self.assertIn('post_mirror', rc.int_contract(pre, 4, 0, lost))
        paid = list(state)
        paid[24] += 1  # the burner is paid one atom more than pro rata
        self.assertIn('conservation0', rc.int_contract(pre, 4, 0, paid))
        greedy = list(state)
        greedy[0] -= 500  # pays out more than pro rata, both sides moved together
        greedy[8] -= 500
        greedy[24] += 500
        failed = rc.int_contract(pre, 4, 0, greedy)
        self.assertIn('int_step_mask1', failed)
        self.assertIn('rat_step_mask1', failed)

    def test_every_planted_defect_changes_the_program(self):
        mutated = planted_defects.applied(self.text)
        self.assertEqual(set(mutated), set(planted_defects.DEFECTS))
        for name, text in mutated.items():
            other = native_program.load_text(text)
            self.assertNotEqual((other.nodes, other.roots), (self.native.nodes, self.native.roots), name)


if __name__ == '__main__':
    unittest.main()
