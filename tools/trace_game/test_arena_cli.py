"""Evidence binding, entrypoint and model-boundary checks."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from arena_spec import parse
from test_knowledge import memory_game, game

ROOT=Path(__file__).resolve().parents[2]


def run(*args):
    return subprocess.run(['nice','-n','19','ionice','-c3',sys.executable,'-B',*map(str,args)],
                          text=True,capture_output=True,timeout=30)


class ArenaCLI(unittest.TestCase):
    def test_bound_cli_and_common_checker(self):
        with tempfile.TemporaryDirectory() as folder:
            base=Path(folder); source=base/'game.json'; out=base/'evidence'
            source.write_text(json.dumps(memory_game().wire()))
            built=run(ROOT/'tools/trace_game/arena.py','solve','--game',source,'--out',out)
            self.assertEqual(built.returncode,0,built.stdout+built.stderr)
            for command in ((ROOT/'tools/trace_game/arena.py','check','--out',out),
                (ROOT/'tools/trace_game/certificate_check.py',out,'certificate.json')):
                checked=run(*command)
                self.assertEqual(checked.returncode,0,checked.stdout+checked.stderr)
                self.assertEqual(json.loads(checked.stdout)['evidence'],'certified winning strategy')
            before={p.name:p.read_bytes() for p in out.iterdir()}
            repeated=run(ROOT/'tools/trace_game/arena.py','solve','--game',source,'--out',out)
            self.assertEqual(repeated.returncode,2)
            self.assertEqual(before,{p.name:p.read_bytes() for p in out.iterdir()})
            packet=json.loads((out/'certificate.json').read_text()); packet['knowledge']['nodes'].pop()
            (out/'certificate.json').write_text(json.dumps(packet))
            refused=run(ROOT/'tools/trace_game/certificate_check.py',out,'certificate.json')
            self.assertEqual(refused.returncode,2)
            self.assertIn('artifact drift',refused.stderr)

    def test_unknown_budget_has_no_binding(self):
        with tempfile.TemporaryDirectory() as folder:
            base=Path(folder); source=base/'game.json'; out=base/'evidence'
            source.write_text(json.dumps(memory_game().wire()))
            result=run(ROOT/'tools/trace_game/arena.py','solve','--game',source,'--out',out,'--max-states',1)
            self.assertEqual(result.returncode,2,result.stdout+result.stderr)
            self.assertEqual(json.loads(result.stdout)['status'],'UNKNOWN')
            self.assertFalse((out/'arena-binding.json').exists())

    def test_zero_seconds_all_target_reachability_has_no_binding(self):
        value=game([0],[[('stay',0)]],roots=[0],partial=False).wire()
        value['objective']={'kind':'reachability','states':[0]}
        with tempfile.TemporaryDirectory() as folder:
            base=Path(folder); source=base/'game.json'; out=base/'evidence'
            source.write_text(json.dumps(value))
            result=run(ROOT/'tools/trace_game/arena.py','solve','--game',source,'--out',out,'--seconds',0)
            self.assertEqual(result.returncode,2,result.stdout+result.stderr)
            self.assertEqual(json.loads(result.stdout),{'status':'UNKNOWN','stop':'solver time bound','work':0})
            self.assertEqual(json.loads((out/'certificate.json').read_text()),json.loads(result.stdout))
            self.assertFalse((out/'arena-binding.json').exists())
            self.assertFalse((out/'verdict.json').exists())

    def test_root_namespaces_in_bound_verdict(self):
        with tempfile.TemporaryDirectory() as folder:
            base=Path(folder); source=base/'game.json'
            for partial in (False,True):
                arena=game([1,1],[[('stay',0)],[('stay',1)]],[0],['bad','safe'],[1],partial)
                source.write_text(json.dumps(arena.wire())); out=base/str(partial)
                built=run(ROOT/'tools/trace_game/arena.py','solve','--game',source,'--out',out)
                self.assertEqual(built.returncode,0,built.stdout+built.stderr)
                verdict=json.loads(built.stdout)
                self.assertEqual(verdict['winning_concrete_roots'],[1])
                self.assertEqual(verdict['winning_knowledge_roots'],[0] if partial else [])
                self.assertEqual(verdict['root_mapping'],
                                 [dict(knowledge_root=0,concrete_roots=[1])] if partial else [])
                self.assertEqual(json.loads((out/'verdict.json').read_text()),verdict)
                checked=run(ROOT/'tools/trace_game/certificate_check.py',out,'certificate.json')
                self.assertEqual(checked.returncode,0,checked.stdout+checked.stderr)
                self.assertEqual(json.loads(checked.stdout),verdict)

    def test_partial_visibility_and_totality(self):
        value=game([0,0],[[('a',0)],[('b',1)]],partial=False).wire(); value['observation']='partial'
        for node in value['nodes']: node['observation']='same'
        with self.assertRaisesRegex(ValueError,'enabled actions'): parse(value)
        value=memory_game().wire(); value['nodes'][0]['edges']=[]
        with self.assertRaisesRegex(ValueError,'total arena'): parse(value)


if __name__=='__main__': unittest.main()
