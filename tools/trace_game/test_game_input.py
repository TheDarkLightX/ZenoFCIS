"""Native caller-game examples and named mutation-control assertions."""
import copy
import json
import os
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest

import game_spec
from certificate_check import check
from game import native, run
from product import build
from solve import certify

ROOT=Path(__file__).resolve().parents[2]
OBSERVATIONS={}


def prepare(path,binary,change=None):
    game,contract=game_spec.load(path)
    if change: change(game)
    request=dict(project=(contract/'project.zeno').read_text(),policy=(contract/'v2/policy.json').read_text())
    metadata=native(binary,request)
    request.update(events=game_spec.expand(game,metadata['positions']),
                   formula=game['objective']['formula'],guards=game.get('guards',['true']))
    packet=native(binary,request)
    table={tuple(row['pre'])+(row['event'],):row for row in packet['rows']}
    return game_spec.description(game,packet),table,packet


@unittest.skipUnless(os.environ.get('ZENO_TRACE_GAME_NATIVE'),'set ZENO_TRACE_GAME_NATIVE to the built driver')
class NativeGame(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.binary=Path(os.environ['ZENO_TRACE_GAME_NATIVE'])
        cls.safe=prepare(ROOT/'test-data/trace-games/escrow-safe.json',cls.binary)
        cls.planted=prepare(ROOT/'test-data/trace-games/escrow-planted.json',cls.binary)

    def verdict(self,subject):
        spec,table,_=subject
        return check(certify(spec,table),table)

    def test_safety_observes_post(self):
        result=self.verdict(self.planted); OBSERVATIONS['post']=result
        self.assertEqual(result['evidence'],'counterexample')
        self.assertEqual([x['actor'] for x in result['history']],['buyer','seller','outsider'])
        self.assertEqual(result['history'][-1]['post'],[153,0,1,0])

    def test_capabilities_bound_events(self):
        game,contract=game_spec.load(ROOT/'test-data/trace-games/escrow-planted.json')
        game['actors'][-1]['command']['command.101.120']=[160,161,163]
        metadata=self.planted[2]
        events=game_spec.expand(game,metadata['positions'])
        outsider=[e['request'] for e in events if e['actor']==3]
        OBSERVATIONS['capabilities']=dict(events=len(events),outsider_sample=outsider[:8],
            action_domain=sorted({e[0] for e in outsider}),
            forbidden=next((e for e in outsider if e[0]==162 or e[2]!=173),None))
        self.assertEqual(sorted({e[0] for e in outsider}),[160,161,163])
        self.assertTrue(all(e[2]==173 for e in outsider))
        request=dict(project=(contract/'project.zeno').read_text(),policy=(contract/'v2/policy.json').read_text(),
                     events=events,formula=game['objective']['formula'],guards=['true'])
        packet=native(self.binary,request)
        table={tuple(r['pre'])+(r['event'],):r for r in packet['rows']}
        result=check(certify(game_spec.description(game,packet),table),table)
        OBSERVATIONS['capabilities']['verdict']=result
        self.assertEqual(result['evidence'],'certified safety')

    def test_certificate_contains_all_reachable_states(self):
        spec,table,_=self.safe
        packet=certify(spec,table)
        try: result=check(packet,table)
        except ValueError as error: result=dict(evidence='refused',reason=str(error))
        OBSERVATIONS['certificate']=result
        self.assertEqual(result['evidence'],'certified safety')
        self.assertEqual(set(packet['certificate']['W']),set(range(len(packet['graph']['states']))))

    def test_product_limit_enforced(self):
        spec,table,_=self.planted
        result=build(spec,table,max_states=len(spec['coalitions']))
        OBSERVATIONS['limit']=dict(status=result.status,stop=result.stop,states=len(result.states),
                                   limit=len(spec['coalitions']))
        self.assertEqual(result.status,'UNKNOWN')
        self.assertEqual(result.stop,'state budget')

    def test_native_replays_and_refusal_work(self):
        spec,table,packet=self.safe
        self.assertEqual(self.verdict(self.safe)['evidence'],'certified safety')
        self.assertTrue(packet['genesis_replayed'])
        refused=[r for r in table.values() if r['class']=='Refused']
        self.assertTrue(refused)
        self.assertTrue(all(r['refusal_repeated'] and r['usage_steps']>0 and r['usage_reads']>0 for r in refused))
        self.assertTrue(all(r['publication_replayed'] for r in table.values() if r['class']!='Refused'))

    def test_bounds_change_the_declared_environment(self):
        spec,table,_=self.planted
        game=copy.deepcopy(spec['game']); game['compromised_actor_bound']=0
        constrained=game_spec.description(game,self.planted[2])
        self.assertTrue(all(not c & 8 for c in constrained['coalitions']))
        self.assertEqual(check(certify(constrained,table),table)['evidence'],'certified safety')

    def test_native_reachability_withholding_and_forced_target(self):
        # The environment can repeat an unauthorized deposit forever, so the
        # eventual payout is not forced despite the existence of a payout path.
        path=ROOT/'test-data/trace-games/escrow-safe.json'
        def target(game): game['objective']=dict(kind='reachability',formula='post.100.112 > 0')
        spec,table,_=prepare(path,self.binary,target)
        result=check(certify(spec,table),table)
        self.assertEqual(result['evidence'],'counterexample')
        self.assertIn('repeat_at',result)
        def immediate(game): game['objective']=dict(kind='reachability',formula='true')
        spec,table,_=prepare(path,self.binary,immediate)
        self.assertEqual(check(certify(spec,table),table)['evidence'],'certified reachability')

    def test_full_width_scalar_values(self):
        large=1<<80
        project=f'''zeno 1; project 1 counter; namespace 2 counter;
type 100 state Counter;
type 101 command Command;
type 102 context Context;
type 103 int Count in {large}..={large+1};
type 104 int Input in 0..=0;
field 110 100 count 103;
field 120 101 action 104;
field 130 102 caller 104;
reason 200 at_limit precedence 0;
component 400 counter {{ owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 64; }}
merge [400];
law 500 bounded on commit, genesis = post.100.110 >= {large};
'''
        policy=dict(schema='zeno-fcis/template-declarative-policy/2',template='counter',
            roots=dict(state=100,command=101,context=102),leaf_bindings={},variables={'x':'pre.100.110'},
            cases=[dict(when=f'x == {large+1}',**{'class':'Reject'},reason=200,post={},outbox=[]),
                   dict(when='true',**{'class':'Accept'},reason=None,post={'110':'x + 1'},outbox=[])],
            genesis={'110':large},law_kinds={'500':'StateInvariant'})
        packet=native(self.binary,dict(project=project,policy=json.dumps(policy),
            events=[dict(actor=0,request=[0,0])],guards=['true'],formula='post.100.110 > pre.100.110'))
        self.assertEqual(packet['genesis'],[large])
        self.assertEqual(packet['rows'][0]['post'],[large+1])
        self.assertTrue(packet['rows'][0]['holds'])

    def test_unknown_diagnostic_and_existing_directory(self):
        with tempfile.TemporaryDirectory() as folder:
            base=Path(folder); bad=base/'bad.json'; bad.write_text('{"schema":1,"schema":2}')
            out=base/'unknown'
            command=['nice','-n','19','ionice','-c3','python3','-B',str(ROOT/'tools/trace_game/game.py'),
                     'certify','--game',str(bad),'--out',str(out),'--native',str(self.binary)]
            completed=subprocess.run(command,capture_output=True,text=True)
            self.assertEqual(completed.returncode,2,completed.stderr)
            self.assertEqual(json.loads(completed.stdout)['evidence'],'UNKNOWN')
            checked=subprocess.run(['nice','-n','19','ionice','-c3','python3','-B',
                str(ROOT/'tools/trace_game/certificate_check.py'),str(out),'certificate.json'],capture_output=True,text=True)
            self.assertEqual(checked.returncode,2,checked.stderr)
            self.assertEqual(json.loads(checked.stdout)['evidence'],'UNKNOWN')
            before={p.name:p.read_bytes() for p in out.iterdir()}
            completed=subprocess.run(command,capture_output=True,text=True)
            self.assertEqual(completed.returncode,2)
            self.assertEqual({p.name:p.read_bytes() for p in out.iterdir()},before)

    def test_entry_points_and_independent_replay(self):
        with tempfile.TemporaryDirectory() as folder:
            base=Path(folder)
            for label in ('safe','planted'):
                out=base/label
                verdict=run(SimpleNamespace(mode='certify',game=ROOT/f'test-data/trace-games/escrow-{label}.json',out=out,native=self.binary))
                completed=subprocess.run(['nice','-n','19','ionice','-c3','python3','-B',
                    str(ROOT/'tools/trace_game/certificate_check.py'),str(out),'certificate.json'],capture_output=True,text=True)
                self.assertEqual(completed.returncode,0 if label == 'safe' else 1,completed.stderr)
                self.assertEqual(json.loads(completed.stdout)['evidence'],verdict['evidence'])
            original=json.loads((ROOT/'test-data/trace-games/escrow-planted.json').read_text())
            original['contract']=str(ROOT/'test-data/trace-games/escrow-planted')
            original['guards']=['true','context.102.130 != 173']
            path=base/'game.json'; path.write_text(json.dumps(original))
            selected=run(SimpleNamespace(mode='select-guard',game=path,out=base/'selected',native=self.binary))
            self.assertEqual(selected['evidence'],'certified safety')
            self.assertEqual(selected['guard'],'context.102.130 != 173')
            original['guards']=['true']; path.write_text(json.dumps(original))
            exhausted=run(SimpleNamespace(mode='select-guard',game=path,out=base/'exhausted',native=self.binary))
            self.assertEqual(exhausted['evidence'],'no guard in class')
            original.pop('guards'); original['K']=2; path.write_text(json.dumps(original))
            bounded=run(SimpleNamespace(mode='check',game=path,out=base/'bounded',native=self.binary))
            self.assertEqual(bounded['evidence'],'checked up to K')


if __name__=='__main__': unittest.main()
