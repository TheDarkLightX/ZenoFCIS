"""Observation-domain and exact-integer certificate admission controls."""
import copy
import unittest
from certificate_check import check, validate_tables
from product import description
from solve import fixed_guard, reactive, unrealizable
import test_certificate


class Admission(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        test_certificate.Certificate.setUpClass()
        cls.tables=test_certificate.Certificate.tables
        cls.native=test_certificate.Certificate.packet
        cls.quantifier=reactive(description('quantifier'),{})
        cls.losing=unrealizable(description('oracle-withholding'),{})
        cls.counterexample=fixed_guard(description('quantifier'),{})['attempts'][2]
        for baseline in (cls.native,cls.quantifier,cls.losing,cls.counterexample):
            check(baseline,cls.tables)

    def refused(self,baseline,change,message):
        packet=copy.deepcopy(baseline); change(packet)
        with self.assertRaisesRegex(ValueError,message): check(packet,self.tables)

    def test_unobserved_actor_value(self):
        self.refused(self.quantifier,lambda p:p['certificate']['strategy']['rules'].append(
            dict(actor=99,request=[0],guard='true')),'observation outside declared domain')

    def test_unobserved_request_value(self):
        self.refused(self.quantifier,lambda p:p['certificate']['strategy']['rules'].append(
            dict(actor=0,request=[99],guard='true')),'observation outside declared domain')

    def test_strategy_totality_is_retained(self):
        self.refused(self.quantifier,lambda p:p['certificate']['strategy']['rules'].pop(),
                     'strategy is not total on W')

    def test_root_bool_index(self):
        self.refused(self.native,lambda p:p['graph']['roots'].__setitem__(0,False),'root index encoding')

    def test_edge_bool_event(self):
        self.refused(self.native,lambda p:p['graph']['edges'][0][0].__setitem__(0,False),
                     'edge event index encoding')

    def test_edge_bool_target(self):
        # Replace a target zero by False, so Python equality alone cannot detect it.
        edge=next(i for i,e in enumerate(self.native['graph']['edges'][0]) if e[2]==0)
        self.refused(self.native,lambda p:p['graph']['edges'][0][edge].__setitem__(2,False),
                     'edge target index encoding')

    def test_graph_indexes_require_exact_type_and_bounds(self):
        n=len(self.native['graph']['states'])
        for value in (True,False,0.0,-1,n):
            with self.subTest(root=value):
                self.refused(self.native,lambda p:p['graph']['roots'].__setitem__(0,value),'root index encoding')
            with self.subTest(target=value):
                self.refused(self.native,lambda p:p['graph']['edges'][0][0].__setitem__(2,value),'edge target index encoding')
        for value in (True,False,0.0,-1,len(self.native['graph']['edges'][0])):
            with self.subTest(event=value):
                self.refused(self.native,lambda p:p['graph']['edges'][0][0].__setitem__(0,value),'edge event index encoding')

    def test_certificate_roots_require_exact_type_and_bounds(self):
        for baseline,kind in ((self.losing,'unrealizability'),(self.counterexample,'counterexample')):
            for value in (True,False,0.0,-1,len(baseline['graph']['states'])):
                with self.subTest(kind=kind,value=value):
                    self.refused(baseline,lambda p:p['certificate'].__setitem__('root',value),
                                 f'{kind} root index encoding')

    def test_counterexample_events_require_exact_type_and_bounds(self):
        for value in (True,False,0.0,-1,2):
            with self.subTest(value=value):
                self.refused(self.counterexample,lambda p:p['certificate']['moves'][0].__setitem__(0,value),
                             'counterexample event index encoding')

    def test_other_integer_fields_refuse_bools(self):
        mutations=(
            (self.native,lambda p:p['certificate']['W'].__setitem__(0,False),'winning set encoding'),
            (self.native,lambda p:p['graph']['states'][0][0].__setitem__(0,True),'product integer encoding'),
            (self.native,lambda p:p['spec']['powers'].__setitem__('signers',True),'frozen capabilities'),
            (self.losing,lambda p:p['certificate']['ranks'][0].__setitem__(0,False),'ranked state encoding'),
            (self.losing,lambda p:p['certificate']['ranks'][0].__setitem__(1,False),'rank encoding'),
        )
        for baseline,change,message in mutations:
            with self.subTest(message=message): self.refused(baseline,change,message)
        ranked=next(i for i,r in enumerate(self.losing['certificate']['ranks']) if r[1]>0)
        self.refused(self.losing,lambda p:p['certificate']['ranks'][ranked].__setitem__(2,True),
                     'permitted environment strategy')
        self.refused(self.quantifier,lambda p:p['certificate']['strategy']['rules'][0].__setitem__('actor',False),
                     'strategy observation encoding')
        self.refused(self.quantifier,lambda p:p['certificate']['strategy']['rules'][0]['request'].__setitem__(0,False),
                     'strategy observation encoding')

    def test_native_post_refuses_bool_integer_alias(self):
        values,actual=next(iter(self.tables['lawful'].items()))
        row=dict(state=list(values[:4]),request=list(values[4:]),outcome=copy.deepcopy(actual),
                 complete_native='test specimen',publication_replayed=True,refusal_repeated=False)
        row['outcome']['post'][0]=False
        with self.assertRaisesRegex(ValueError,'native integer encoding'):
            validate_tables({'lawful':dict(table=dict(rows=[row]))})


if __name__=='__main__': unittest.main()
