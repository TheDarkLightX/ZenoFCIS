"""Input and arena obligations using an inert finite transition specimen."""
import copy
import unittest
from game_spec import decode, validate, coalitions, expand
from product import build


def specimen():
    game = dict(schema='zeno-fcis/game/1', contract='contract',
                actors=[dict(id='caller', compromised=False,
                             command={'command.101.120':[0]}, context={'context.102.130':[1]})],
                coalition_bound=1, compromised_actor_bound=0,
                objective=dict(kind='safety', formula='post.100.110 > pre.100.110'))
    spec = dict(model='contract', genesis=[0], coalitions=[1], empty=False, guard=0,
                events=[dict(actor=0,request=[0,1])], objective=game['objective'])
    table = {(0,0):dict(post=[1], holds=True, blocked_holds=False, guards=[True], **{'class':'Accept'}),
             (1,0):dict(post=[1], holds=False, blocked_holds=False, guards=[True], **{'class':'Reject'})}
    return game,spec,table


class GameSpec(unittest.TestCase):
    def test_strict_json(self):
        for raw in (b'{"a":1,"a":2}', b'{"a":1.0}', b'{"a":NaN}', b' '*65537):
            with self.assertRaises(ValueError): decode(raw)

    def test_scope_refusals(self):
        game,_,_ = specimen()
        self.assertEqual(validate(game), game)
        for change in (dict(observation='partial'), dict(objective={'kind':'buchi','formula':'true'}),
                       dict(K=True), dict(unknown=1), dict(compromised_actor_bound=2)):
            with self.assertRaises(ValueError): validate(dict(game, **change))

    def test_capabilities_and_compromises(self):
        game,_,_ = specimen()
        self.assertEqual(coalitions(game), (1,))
        positions=[dict(name='command.101.120'),dict(name='context.102.130')]
        self.assertEqual(expand(game,positions), [dict(actor=0,request=[0,1])])
        game['actors'][0]['compromised']=True
        self.assertEqual(coalitions(game), ())

    def test_product_and_refusal(self):
        _,spec,table=specimen(); before=copy.deepcopy((spec,table))
        graph=build(spec,table)
        self.assertEqual(graph.status,'complete')
        self.assertTrue(graph.states[-1].bad)
        self.assertEqual((spec,table),before)
        table[(0,0)]['holds']=False
        graph=build(spec,table)
        self.assertEqual(len(graph.states),2)
        self.assertEqual(graph.edges[-1],((0,0,1),))

    def test_product_limit(self):
        _,spec,table=specimen()
        self.assertEqual(build(spec,table,max_states=1).status,'UNKNOWN')


if __name__=='__main__': unittest.main()
