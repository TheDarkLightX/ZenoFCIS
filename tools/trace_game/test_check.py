"""Mutation controls for the independent evidence checker; no native claim."""
import copy
import itertools
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from check import core, starts, initial, moves, advance, check_history, check_packet, regression_inventory
from run import check_monitor, verify_binding


def fixture():
    metadata=dict(model='withdrawal',fault='lawful',horizon=2,powers=dict(accounts=3,signers=1,oracle_choices=3,approval_ages=3,arbitrary_order=True),
                  only_objective=None,disabled_objective=None,unfair_delay=False)
    rows=[];table={}
    for raw in itertools.product(range(3),range(2),range(3),range(2),range(6),range(3),range(3),range(2)):
        state,request=list(raw[:4]),list(raw[4:]);outcome=core(state,request)
        rows.append(dict(state=state,request=request,outcome=outcome,complete_native='test specimen',publication_replayed=True))
        table[raw]=outcome
    nodes=[dict(parent=None,action=None,start=i,coalition=c,snapshot=initial(state),outcome=None,violations=[])
           for i,state in enumerate(starts('withdrawal')) for c in (1,2,3)]
    frontier=list(range(6));completed=0
    for _ in range(2):
        following=[]
        for parent in frontier:
            n=nodes[parent]
            for action in moves(metadata,n['snapshot'],n['coalition']):
                snapshot,outcome,violations,count=advance(metadata,n['snapshot'],action,table[tuple(n['snapshot']['state']+action['request'])])
                following.append(len(nodes));completed+=count
                nodes.append(dict(parent=parent,action=action,start=n['start'],coalition=n['coalition'],snapshot=snapshot,outcome=outcome,violations=violations))
        frontier=following
    pending=[nodes[i]['snapshot']['pending'] for i in frontier if nodes[i]['snapshot']['pending']]
    run=dict(nodes=nodes,frontier=frontier,status='checked-through-K',stop='exhausted-declared-prefixes',completed_depth=2,
             transitions=len(nodes)-6,witnesses={},completed_requests=completed,open_requests=len(pending),late_requests=len(pending),maximum_loss=0,maximum_loss_node=None)
    return dict(schema='zeno-fcis/trace-game/1',metadata=metadata,limits=dict(horizon=2),table=dict(rows=rows),run=run,regressions=[])


class Check(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.packet=fixture()
    def test_specimen_is_not_native_evidence(self):
        with self.assertRaisesRegex(ValueError,'source binding'):check_packet(self.packet)
    def test_complete_fixture(self):self.assertEqual(check_history(self.packet)['status'],'checked-through-K')
    def mutated(self,change,message):
        packet=copy.deepcopy(self.packet);change(packet)
        with self.assertRaisesRegex(ValueError,message):check_history(packet)
    def test_initial_coalitions_cannot_be_omitted(self):self.mutated(lambda p:p['run']['nodes'][0].__setitem__('coalition',2),'initial state/coalition')
    def test_outcome_bytes_cannot_be_changed(self):self.mutated(lambda p:p['table']['rows'][0]['outcome'].__setitem__('kind','Accept'),'independent native mismatch')
    def test_missing_native_row_is_incomplete(self):self.mutated(lambda p:p['table']['rows'].pop(),'incomplete independent')
    def test_reference_is_fixed_before_candidate(self):self.mutated(lambda p:p['run']['nodes'][6]['snapshot']['reference'].__setitem__(0,0),'edge replay')
    def test_reported_completion_is_not_an_oracle(self):self.mutated(lambda p:p['run'].__setitem__('completed_depth',1),'incorrect completed depth')
    def test_open_request_cannot_be_success(self):self.mutated(lambda p:p['run'].__setitem__('open_requests',0),'truncated request')
    def test_fake_witness_is_rejected(self):self.mutated(lambda p:p['run']['witnesses'].__setitem__('drain',6),'false/missing witness')
    def test_duplicate_child_is_not_coverage(self):self.mutated(lambda p:p['run']['nodes'][7].__setitem__('action',p['run']['nodes'][6]['action']),'edge replay|noncanonical')
    def test_pending_monitor_history_is_retained(self):self.mutated(lambda p:p['run']['nodes'][6]['snapshot'].__setitem__('monitor',2),'edge replay')
    def test_loss_cannot_be_self_reported(self):self.mutated(lambda p:p['run'].__setitem__('maximum_loss',9),'incorrect worst loss')
    def test_empty_alphabet_is_never_a_complete_check(self):
        packet=copy.deepcopy(self.packet);packet['metadata']['powers']['approval_ages']=0
        packet['run'].update(nodes=packet['run']['nodes'][:6],witnesses={},transitions=0)
        with self.assertRaisesRegex(ValueError,'empty alphabet'):check_history(packet)
    def test_node_budget_preserves_partial_parent_and_children(self):
        packet=copy.deepcopy(self.packet);packet['limits']['nodes']=7
        packet['run'].update(nodes=packet['run']['nodes'][:7],frontier=list(range(7)),status='UNKNOWN',stop='node-budget:parent=0:move=1',completed_depth=0,transitions=1,completed_requests=0,open_requests=0,late_requests=0)
        self.assertEqual(check_history(packet)['status'],'UNKNOWN')
        packet['run']['frontier'].pop(0)
        with self.assertRaisesRegex(ValueError,'frontier lost'):check_history(packet)
    def test_missing_regression_inventory(self):
        packet=dict(run=dict(nodes=self.packet['run']['nodes'],witnesses={'fault':6}),regressions=[])
        with self.assertRaisesRegex(ValueError,'regression inventory'):regression_inventory(packet,lambda r,a:True)
    def test_source_tampering_invalidates_binding(self):
        from unittest.mock import patch
        binding=dict(source_files={'model.rs':'old'},binaries={})
        with patch('run.source_files',return_value={'model.rs':'new'}),self.assertRaisesRegex(ValueError,'drift'):verify_binding(binding)


class Standalone(unittest.TestCase):
    checker=Path(__file__).with_name('check.py').resolve()

    def test_standalone_refuses_specimen(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory=Path(temporary)
            (directory/'specimen.json').write_text(json.dumps(fixture()))
            result=subprocess.run([sys.executable,'-B',str(self.checker),str(directory)],capture_output=True,text=True,timeout=30)
            self.assertNotEqual(result.returncode,0)
            self.assertIn('native source binding required',result.stderr)
            self.assertFalse((directory/'replay.json').exists())

    @unittest.skipUnless(os.environ.get('TRACE_GAME_CAMPAIGN'),'set TRACE_GAME_CAMPAIGN for the real-campaign CLI check')
    def test_standalone_replays_real_campaign(self):
        campaign=Path(os.environ['TRACE_GAME_CAMPAIGN']).resolve()
        self.assertTrue((campaign/'binding.json').is_file())
        # Link immutable inputs; keep all derived output in this test's directory.
        with tempfile.TemporaryDirectory() as temporary:
            directory=Path(temporary)
            for path in campaign.iterdir():
                if path.name not in ('checks','replay.json'):
                    (directory/path.name).symlink_to(path,target_is_directory=path.is_dir())
            result=subprocess.run([sys.executable,'-B',str(self.checker),str(directory)],capture_output=True,text=True,timeout=600)
            self.assertEqual(result.returncode,0,result.stderr+result.stdout)
            records=json.loads((directory/'replay.json').read_text())
            self.assertEqual(len(records),46)
            self.assertEqual(records['core-lawful.json']['status'],'checked-through-K')
            self.assertEqual(records,json.loads((directory/'checks/independent-replay.json').read_text()))


if __name__=='__main__':unittest.main()
