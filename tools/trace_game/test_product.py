"""Product closure and exhaustion controls; specimens carry no native credit."""
import unittest
from check import initial, moves, advance, core
from product import build, description, State, snapshot
from test_check import fixture


class Product(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tables = {'lawful':{tuple(r['state']+r['request']):r['outcome']
                                for r in fixture()['table']['rows']}}

    def test_finite_product_and_relative_time(self):
        spec = description('withdrawal'); graph = build(spec, self.tables)
        self.assertEqual(graph.status, 'complete')
        self.assertTrue(all(not s.bad for s in graph.states))
        # A long tick prefix changes absolute v1 time, but no future behavior.
        before = initial([2,0,0,0]); before['time'] = 200
        request = dict(actor=0, request=[0,0,0,1])
        after, _, bad, _ = advance(spec, before, request, core(before['state'],request['request']))
        finite = State(tuple(after['state']), tuple(after['reference']), 3, 1, 2)
        self.assertFalse(bad)
        self.assertEqual(moves(spec, after, 3), moves(spec, snapshot(finite), 3))
        self.assertLess(len(graph.states), 300)

    def test_budgets_are_unknown(self):
        for limits in (dict(max_states=6), dict(max_edges=1), dict(seconds=0)):
            self.assertEqual(build(description('withdrawal'), self.tables, **limits).status,'UNKNOWN')

    def test_missing_native_move_is_unknown(self):
        self.assertEqual(build(description('withdrawal'), {'lawful':{}}).status, 'UNKNOWN')

    def test_empty_environment_is_unknown(self):
        self.assertEqual(build(description('withdrawal',empty=True), self.tables).status,'UNKNOWN')


if __name__ == '__main__': unittest.main()
