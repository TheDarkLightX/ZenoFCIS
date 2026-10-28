"""Independent pairwise finite-function oracle across small machine carriers."""
from copy import deepcopy
from itertools import product
import unittest

from zal_notation import lenses as L


def machine(n, *, reset=True, observation=False, two_auth=False):
    states = tuple(sorted(['unseen', *('s' + str(i) for i in range(1, n))]))
    ordinal = ['unseen', *('s' + str(i) for i in range(1, n))]
    events = ('inc', 'reset') if reset else ('inc',)
    auths = ('no', 'yes') if two_auth else ('yes',)
    inputs = (L.finite.Port('state', 'counter', 'mod_n', states),
              L.finite.Port('event', 'command', 'control', events),
              L.finite.Port('authorized', 'environment', 'control', auths))
    outputs = (L.finite.Port('outcome', 'observation', 'control', ('marked','ok') if observation else ('ok',)),
               L.finite.Port('next_state', 'counter', 'mod_n', states),
               L.finite.Port('reason', 'observation', 'control', ('done',)))
    table = {}
    for s,e,a in product(states, events, auths):
        target = s if a == 'no' else ('unseen' if e == 'reset' else ordinal[(ordinal.index(s)+1)%n])
        table[(s,e,a)] = ((('marked' if observation and s == 'unseen' else 'ok'), target, 'done'),)
    return L.finite.build(inputs, outputs, table, (('state','next_state','unseen'),))


def independent_closure(contract):
    states = tuple(contract.inputs[0].values)
    row_lookup = dict(contract.rows)
    generators = {tuple(states.index(row_lookup[(s,e,a)][0][1]) for s in states)
                  for e,a in product(contract.inputs[1].values, contract.inputs[2].values)}
    result = generators | {tuple(range(len(states)))}
    while True:
        enlarged = result | {tuple(x[i] for i in y) for x,y in product(result,result)}
        if enlarged == result:
            return result
        result = enlarged


class IndependentIntegrationTests(unittest.TestCase):
    def test_independent_closures_and_laws_across_carriers(self):
        for n,reset,two_auth in product(range(1,6), (False,True), (False,True)):
            with self.subTest(n=n,reset=reset,two_auth=two_auth):
                contract=machine(n,reset=reset,two_auth=two_auth)
                source=L.make_source(contract)
                app=L.apply(source,'transformation-monoid')
                algebra=app['payload']['algebra']
                effects=list(map(tuple,algebra['elements']))
                self.assertEqual(set(effects),independent_closure(contract))
                self.assertEqual(len(effects), n if not reset or n==1 else 2*n)
                identity=effects.index(tuple(range(n)))
                self.assertEqual(algebra['identity'], identity)
                for a,b in product(range(len(effects)), repeat=2):
                    self.assertEqual(effects[algebra['multiplication'][a][b]],tuple(effects[a][i] for i in effects[b]))
                self.assertEqual(algebra['units'],[i for i,v in enumerate(effects) if sorted(v)==list(range(n))])
                for lid in ('transformation-monoid','unit-group','one-object-category','all-effects-group'):
                    application=L.apply(source,lid)
                    receipt=L.check(source,application)
                    self.assertEqual(receipt['status'],'refuted' if lid=='all-effects-group' and reset and n>1 else 'checked')
                    self.assertTrue(L.verify_receipt(source,application,receipt))

    def test_observable_outputs_can_refute_otherwise_valid_state_quotient(self):
        mapping={'unseen':'even','s1':'odd','s2':'even','s3':'odd'}
        for observation in (False,True):
            source=L.make_source(machine(4, observation=observation,two_auth=True))
            app=L.apply(source,'parity-quotient',{'mapping':mapping})
            result=L.check(source,app)
            self.assertEqual(result['status'],'refuted' if observation else 'checked')

    def test_unreachable_declared_rows_are_not_ignored(self):
        contract=machine(4)
        rows=contract.table()
        for s,e,a in rows:
            rows[(s,e,a)]=( ('ok','unseen','done'), )
        rows[('s2','inc','yes')]=( ('ok','s1','done'), )
        source=L.make_source(L.finite.build(contract.inputs,contract.outputs,rows,contract.state))
        mapping={'unseen':'all','s1':'other','s2':'all','s3':'all'}
        result=L.check(source,L.apply(source,'parity-quotient',{'mapping':mapping}))
        self.assertEqual(result['status'],'refuted')

    def test_explicit_source_retention_and_all_budget_statuses(self):
        source=L.make_source(machine(3,two_auth=True))
        before=deepcopy(source)
        for lid in ('definition-expand','definition-fold','finite-table','state-machine','transformation-monoid','unit-group','one-object-category','all-effects-group'):
            app=L.apply(source,lid)
            self.assertEqual(L.check(source,app,0)['status'],'incomplete')
            self.assertEqual(L.zoom_back(source,app),before)
            self.assertEqual(source,before)
            if lid not in ('definition-expand','definition-fold','finite-table'):
                with self.assertRaises((ValueError, L.Unsupported)):
                    L.zoom_back(app['target_context'],app)


if __name__=='__main__':
    unittest.main(verbosity=2)
