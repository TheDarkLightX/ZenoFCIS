"""Withdrawal temporal pilot checks against the real contract; authority: none."""
import json
import os
from pathlib import Path
import subprocess
import unittest

import ladder as l
import withdrawal_pilot as w

EXAMPLES=Path(__file__).with_name('examples')/'temporal'
# Unit runs replay every word through length 8, the shortest that holds a
# complete eight-tick window; check_withdrawal_pilot.py run covers length 12.
LENGTH=int(os.environ.get('ZAL_WITHDRAWAL_LENGTH','8'))


def binary(name,variable):
    target=Path(os.environ.get('CARGO_TARGET_DIR','target'))/'debug/examples'/name
    executable=Path(os.environ.get(variable,str(target)))
    if not executable.is_file():raise AssertionError('Build the pilot harnesses with check_withdrawal_pilot.py native-build')
    return executable.resolve()


_GRAPH=[]


def graph():
    if not _GRAPH:
        result=subprocess.run([str(binary('withdrawal_queue_decisions','ZAL_WITHDRAWAL_GRAPH_CLI'))],capture_output=True,text=True,timeout=900)
        if result.returncode:raise AssertionError(result.stderr)
        _GRAPH.append(w.load_graph(result.stdout))
    return _GRAPH[0]


def pilot():return binary('withdrawal_temporal_pilot','ZAL_WITHDRAWAL_PILOT_CLI')


class WithdrawalPilotChecks(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.graph=graph()
        cls.bounded=l.load(EXAMPLES/'alarm_withdrawal_bounded.json')
        cls.unbounded=l.load(EXAMPLES/'alarm_withdrawal.json')
        cls.paths=w.statement_paths(cls.bounded)
        cls.automaton,cls.info=w.tick_automaton(cls.graph,cls.paths)
        owners=[owner for _,owner in cls.paths]
        cls.columns=[(owners.index(('R',(lane,))),owners.index(('C',(lane,)))) for lane in w.SIDES]
        cls.words=list(w.words(cls.automaton,LENGTH))

    def test_statements_bind_the_contract(self):
        identity=w.contract_identity()
        for ctx in (self.bounded,self.unbounded):
            self.assertIn(identity,ctx.data['trace']['source_revision'])
            self.assertEqual(ctx.data['authority']['status'],'DECIDED')
            for meta in ctx.data['symbols'].values():self.assertNotIn('PLACEHOLDER',meta['definition'])
            self.assertEqual(list(ctx.domain('Requests')['values']),list(w.SIDES))
        self.assertEqual((self.bounded.data['trace']['mode'],self.bounded.data['trace']['horizon']),('finite',w.HORIZON))
        self.assertIn('within(%d, C(r))'%w.K,self.bounded.data['formula'])
        ahead=self.bounded.definition('Ahead')[0]['body']
        self.assertEqual(ahead.count('next('),w.K)
        self.assertEqual(self.unbounded.data['trace']['mode'],'unbounded')

    def test_adapter_obligations(self):
        try:counts=w.check_adapter(self.graph)
        except w.AdapterRefusal as exc:self.fail(str(exc))
        # The contract's own conformance grid: 420 reachable vaults, and
        # every one decided over the 96-tuple declared command and context domain.
        self.assertEqual((counts['vaults'],counts['decisions']),(420,420*96))
        # A tick ignores lane and amount: two of each, times both alarm values.
        self.assertEqual(counts['ticks'],420*8)
        self.assertGreater(counts['alarm_ticks_with_request'],0)
        print('PILOT adapter: %(vaults)d vaults, %(decisions)d decisions, %(requests)d accepted requests, %(payouts)d payouts'%counts)

    def test_tick_quotient(self):
        # The controller check's 26 reachable product states, independently.
        self.assertEqual(self.info['classes'],26)
        self.assertEqual(len(self.automaton['initial']),1)
        # The horizon: every class is reached within 4 ticks, then a full window.
        self.assertEqual(w.reach_depth(self.automaton),4)
        self.assertEqual(w.HORIZON,w.reach_depth(self.automaton)+w.K+1)
        print('PILOT tick projection: %(boundary_vaults)d boundary vaults, %(classes)d classes, %(edges)d labelled edges'%self.info)

    def test_fixture_histories(self):
        for fixture in self.bounded.data['fixtures']:
            real=w.in_language(self.automaton,fixture['events'])
            self.assertEqual(real,'Not a contract history' not in fixture['note'],fixture['id'])
            self.assertEqual(w.window_oracle(fixture['events'],w.K,True,self.columns),fixture['expected'],fixture['id'])

    def test_bounded_claim_finite_checked(self):
        report=w.finite_check(pilot(),self.bounded,self.automaton,LENGTH)
        self.assertEqual((report['counterexamples'],report['indeterminate']),(0,0),report['shown'])
        self.assertEqual(report['words'],len(self.words))
        self.assertEqual(report['words_by_length'],[sum(len(x)==n for x in self.words) for n in range(1,LENGTH+1)])
        print('PILOT bounded k=%d: %d words of lengths 1..%d satisfied'%(w.K,report['words'],LENGTH))

    def test_bound_is_tight(self):
        k=w.K-1
        formula=self.bounded.data['formula'].replace('within(%d,'%w.K,'within(%d,'%k)
        definition=dict(l.plain_data(self.bounded.data['definitions'])[0],body='next('*k+'Tick()'+')'*k)
        report=w.finite_check(pilot(),w.variant(self.bounded,formula,[definition]),self.automaton,LENGTH)
        expected=sum(not w.window_oracle(word,k,True,self.columns) for word in self.words)
        self.assertGreater(report['counterexamples'],0)
        self.assertEqual(report['counterexamples'],expected)
        print('PILOT tightness k=%d: %d counterexamples, as the window oracle predicts'%(k,expected))

    def test_strong_endpoint(self):
        # Complete before the word ends: a request presented near the end
        # must still be paid; weak next would let it pass unpaid.
        formula='forall sigma in Traces: forall r in Requests: always(R(r) => within(%d, C(r)))'%w.K
        report=w.finite_check(pilot(),w.variant(self.bounded,formula),self.automaton,LENGTH)
        expected=sum(not w.window_oracle(word,w.K,False,self.columns) for word in self.words)
        self.assertGreater(expected,0)
        self.assertEqual(report['counterexamples'],expected)
        print('PILOT endpoint: %d words end with an unpaid presented request'%expected)

    def test_independent_window_oracle(self):
        self.assertTrue(all(w.window_oracle(word,w.K,True,self.columns) for word in self.words))
        alarmed=sum(any(word[p][self.columns[0][0]] or word[p][self.columns[1][0]] for p in range(len(word)-w.K)) for word in self.words)
        self.assertGreater(alarmed,0)

    def test_unbounded_split_and_rows(self):
        name,obligation=w.run_assumption_split(self.unbounded)
        self.assertEqual(name,'A')
        self.assertIn('lean unbounded',obligation.source)
        rows=w.controller_rows(self.graph);reach=sorted({w.ctrl(v) for v in self.graph['vaults']})
        bounds=[]
        for side in w.SIDES:
            table,converged=w.potentials(rows,reach,side)
            self.assertTrue(converged)
            bounds.append(max(table.values()))
        # The contract's response bound: paid within 8 ticks of being recorded.
        self.assertEqual(bounds,[8,8])
        print('PILOT Lean rows: %d controller rows over %d controller states; potential bound 8'%(len(rows),len(reach)))

    def test_unbounded_kernel_checked(self):
        lean=os.environ.get('ZENO_FCIS_LEAN');exported=os.environ.get('ZAL_WITHDRAWAL_LEAN_EXPORT')
        if not lean or not exported:self.skipTest('pinned Lean and the exported claim are supplied by check_withdrawal_pilot.py run')
        source,info=w.lean_source(self.graph,Path(exported).read_text())
        result=w.run_lean(source,lean,'withdrawal_never_freezes')
        self.assertEqual(result['status'],'kernel-checked',result)
        self.assertEqual(result['axioms'],w.ALLOWED_AXIOMS)
        print('PILOT unbounded: kernel checked; axioms %s'%', '.join(result['axioms']))


if __name__=='__main__':unittest.main()
