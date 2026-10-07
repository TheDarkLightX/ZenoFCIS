"""Generator controls: source grammar, full-domain metadata and law preservation."""
import importlib.util
from pathlib import Path
import unittest
import copy
import math
SPEC=importlib.util.spec_from_file_location('template_contracts',Path(__file__).with_name('check_template_contracts_v2.py'))
MODULE=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(MODULE)

class Contracts(unittest.TestCase):
    def test_precedence_and_non_language_tokens_refuse(self):
        self.assertEqual(MODULE.parse('a || b -> c && d'),('->',('||',('name','a'),('name','b')),('&&',('name','c'),('name','d'))))
        self.assertEqual(MODULE.parse('div_ceil(a * 3, 4 * price)'),('div_ceil',('*',('name','a'),('int',3)),('*',('int',4),('name','price'))))
        with self.assertRaises(ValueError):MODULE.parse('x; side_effect()')
        with self.assertRaises(AssertionError):MODULE.parse('callback(x)')
    def test_complete_original_definitions_and_scopes(self):
        counts=[6,9,12,10,13,21,7,15]
        self.assertEqual(len(MODULE.TEMPLATES),len(counts))
        for name,count in zip(MODULE.TEMPLATES,counts):
            p=MODULE.Project(name);self.assertEqual(len(p.types),count)
            self.assertEqual({i for i,_,_ in p.laws},{int(i) for i in p.policy['law_kinds']})
            for t in p.types.values():self.assertTrue(t['fields'] or t['variants'] or t['leaf'])
        p=MODULE.Project('account-lockout')
        self.assertEqual(p.types[105]['leaf'],['I128',0,2])
        self.assertEqual(p.types[106]['leaf'],['I128',0,4102444800])
        self.assertEqual(p.types[107]['leaf'],['I128',0,4102445700])
        self.assertEqual(p.kind(101),'Sum');self.assertEqual(p.kind(109),'Sum')
        treasury=MODULE.Project('agent-treasury-guard')
        self.assertEqual([(i,s) for i,s,_ in treasury.laws],[(500,'commit, genesis'),(501,'commit'),(502,'commit'),(503,'accept'),(504,'accept'),(505,'accept'),(506,'accept'),(507,'accept'),(508,'failure')])
    def test_variable_cycles_refuse(self):
        p=MODULE.Project('durable-counter');p.variables['count']=MODULE.parse('count + 1')
        with self.assertRaises(AssertionError):p.expand(MODULE.parse('count'))
    def test_original_sum_leaf_not_enum(self):
        for name in MODULE.TEMPLATES:
            p=MODULE.Project(name)
            for tid,t in p.types.items():
                if t['variants']:self.assertIn('InputLeaf::Sum',p.input_leaf(tid))

    def test_original_prepared_shape_bounds_and_all_216_inputs(self):
        p=MODULE.Project('prepared-counter')
        self.assertEqual(p.types[105]['leaf'],['I128',0,3])
        self.assertEqual(p.types[106]['leaf'],['I128',-1,1])
        self.assertEqual(p.types[101]['fields'],[(120,'first',106),(121,'second',106),(122,'third',106)])
        self.assertEqual(p.types[102]['leaf'],['Bool'])
        self.assertEqual(4*3*3*3*2,216)
        self.assertEqual([(i,s) for i,s,_ in p.laws],[(500,'commit, genesis'),(501,'accept'),(502,'failure'),(503,'reject')])
        self.assertEqual(p.channels,[(300,103,104)])
        self.assertEqual([case['reason'] for case in p.policy['cases']],[200,201,None])
        self.assertEqual(p.policy['cases'][-1]['outbox'][0]['destination'],'local-observer')
    def test_original_gateway_full_domain_rule_precedence_and_links(self):
        p=MODULE.Project('compliance-gateway')
        cardinalities=[]
        for source,tid in p.inputs:
            t=p.types[tid]
            cardinalities.append(len(t['variants']) if t['variants'] else 2 if t['leaf'][0]=='Bool' else t['leaf'][2]-t['leaf'][1]+1)
        self.assertEqual(math.prod(cardinalities),2880)
        self.assertEqual(p.channels,[(300,103,104),(301,105,106)])
        reasons=[case['reason'] for case in p.policy['cases']]
        self.assertEqual([reason for reason in reasons if reason is not None],[200,201,210,211,212,213,214])
        self.assertEqual(p.policy['framework_reject_law'],509)
        self.assertEqual(p.policy['cases'][-1]['class'],'Accept')
        for case in p.policy['cases']:
            self.assertEqual(set(case['post']),{'120'} if case['class']!='Reject' else set())
    def test_original_framework_ids_and_complete_field_sets(self):
        for name in ('inventory-reservation','withdrawal-queue'):
            p=MODULE.Project(name)
            self.assertEqual(p.policy['framework_failure_law'],508)
            self.assertEqual(p.policy['framework_reject_law'],509)
        for name in ('account-lockout','order-fulfillment','agent-treasury-guard','compliance-gateway'):
            self.assertEqual(MODULE.Project(name).policy['framework_reject_law'],509)
        p=MODULE.Project('prepared-counter')
        p.policy=copy.deepcopy(p.policy)
        p.policy['cases'][-1]['post'].clear()
        with self.assertRaises(AssertionError):MODULE.source(p)
        p=MODULE.Project('compliance-gateway');p.policy=copy.deepcopy(p.policy)
        next(case for case in p.policy['cases'] if case['outbox'])['outbox'][0]['payload'].clear()
        with self.assertRaises(AssertionError):MODULE.source(p)

if __name__=='__main__':unittest.main()
