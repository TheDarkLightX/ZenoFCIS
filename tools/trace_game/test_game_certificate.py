"""Independent checks for caller games, including adversarial certificate edits."""
import copy
import unittest
from unittest.mock import patch
from certificate_check import check
from game_spec import description
from solve import certify
from test_game_spec import specimen


def subject():
    game,_,table=specimen()
    native=dict(positions=[dict(name='pre.100.110'),dict(name='command.101.120'),dict(name='context.102.130')],genesis=[0])
    return description(game,native),table


class GameCertificate(unittest.TestCase):
    def test_safety_counterexample_replays(self):
        spec,table=subject(); packet=certify(spec,table)
        verdict=check(packet,table)
        self.assertEqual(verdict['evidence'],'counterexample')
        self.assertEqual(verdict['history'][0]['post'],[1])

    def test_reachable_state_hole(self):
        spec,table=subject(); table[(0,0)]['holds']=False
        packet=certify(spec,table)
        self.assertEqual(check(packet,table)['evidence'],'certified safety')
        packet['certificate']['W'].pop()
        with self.assertRaisesRegex(ValueError,'closure'): check(packet,table)

    def test_reachable_graph_hole(self):
        spec,table=subject(); table[(0,0)]['holds']=False
        packet=certify(spec,table); packet['graph']['states'].pop(); packet['graph']['edges'].pop()
        with self.assertRaisesRegex(ValueError,'missing reachable'): check(packet,table)

    def test_bounded_has_no_unbounded_claim(self):
        spec,table=subject(); spec['horizon']=0
        verdict=check(certify(spec,table),table)
        self.assertEqual(verdict['evidence'],'checked up to K')
        self.assertEqual(verdict['K'],0)

    def test_reachability_and_nonreaching_cycle(self):
        spec,table=subject(); spec['game']['objective']['kind']='reachability'
        self.assertEqual(check(certify(spec,table),table)['evidence'],'certified reachability')
        table[(0,0)]['holds']=False
        packet=certify(spec,table); verdict=check(packet,table)
        self.assertEqual(verdict['evidence'],'counterexample')
        self.assertEqual(verdict['repeat_at'],1)
        packet['certificate']['repeat_at']=0
        with self.assertRaisesRegex(ValueError,'cycle replay'): check(packet,table)

    def test_solver_limit(self):
        spec,table=subject()
        with patch('solve.SOLVER_WORK',0):
            self.assertEqual(certify(spec,table)['status'],'UNKNOWN')
            spec['game']['objective']['kind']='reachability'
            self.assertEqual(certify(spec,table)['status'],'UNKNOWN')

    def test_native_subject_cannot_be_relabelled(self):
        spec,table=subject(); table['__subject__']=copy.deepcopy(spec)
        packet=certify(spec,table); packet['spec']['game']['objective']['formula']='true'
        with self.assertRaisesRegex(ValueError,'subject mismatch'): check(packet,table)


if __name__=='__main__': unittest.main()
