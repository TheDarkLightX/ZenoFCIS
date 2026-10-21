"""Independent certificate controls: baseline first, then one-defect mutants."""
import copy
import unittest
from certificate_check import check
from check import core
from dataclasses import replace
from product import build
from solve import safety
from product import description
from solve import certify
import test_product


class Certificate(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        test_product.Product.setUpClass(); cls.tables=test_product.Product.tables
        for name,fault in (('guard-true','unauthorized'),('guard-false','deny-withdraw')):
            cls.tables[name]={key:core(list(key[:4]),list(key[4:]),fault) for key in cls.tables['lawful']}
        cls.packet=certify(description('withdrawal'),cls.tables)
        check(cls.packet,cls.tables)

    def rejected(self,change,message):
        p=copy.deepcopy(self.packet); change(p)
        with self.assertRaisesRegex(ValueError,message): check(p,self.tables)

    def leaky(self):
        spec=description('withdrawal','true'); graph=build(spec,self.tables)
        W=[i for i,s in enumerate(graph.states) if not s.bad]
        packet=dict(schema='zeno-fcis/trace-game-certificate/1.1',spec=spec,
                    graph=graph.wire(),certificate=dict(kind='safety',W=W))
        return graph,packet

    def test_leaky_certificate_wrong_solver(self):
        graph,p=self.leaky()
        self.assertFalse(set(graph.roots)<=set(safety(graph)))
        with self.assertRaisesRegex(ValueError,'closure'): check(p,self.tables)
        # Planted wrong solver drops adversarial transitions into Bad.
        leaking=replace(graph,edges=tuple(tuple(e for e in row if not graph.states[e[2]].bad) for row in graph.edges))
        self.assertTrue(set(leaking.roots)<=set(safety(leaking)))
        p['graph']=leaking.wire(); p['certificate']['W']=safety(leaking)
        with self.assertRaisesRegex(ValueError,'move coverage'): check(p,self.tables)

    def test_unsafe_state_in_W(self):
        graph,p=self.leaky(); p['certificate']['W']=list(range(len(graph.states)))
        with self.assertRaisesRegex(ValueError,'unsafe state'): check(p,self.tables)

    def test_empty_environment_is_unknown(self):
        p=copy.deepcopy(self.packet); p['spec']['empty']=True
        p['graph']['states']=p['graph']['states'][:6]; p['graph']['edges']=[[] for _ in range(6)]
        p['certificate']['W']=list(range(6))
        with self.assertRaisesRegex(ValueError,'non-vacuity.*UNKNOWN'): check(p,self.tables)

    def test_checker_core_has_only_standard_library_imports(self):
        import ast
        from pathlib import Path
        tree=ast.parse(Path(__file__).with_name('certificate_check.py').read_text())
        # Optional help and telemetry belong only to the command-line shell.
        core=ast.Module(body=[node for node in tree.body
            if not (isinstance(node,ast.FunctionDef) and node.name=='main')
            and not isinstance(node,ast.If)], type_ignores=[])
        imports={node.names[0].name.split('.')[0] if isinstance(node,ast.Import) else node.module.split('.')[0]
                 for node in ast.walk(core) if isinstance(node,(ast.Import,ast.ImportFrom))}
        self.assertLessEqual(imports,{'argparse','hashlib','itertools','json','pathlib','subprocess'})

    def test_standalone_checker_does_not_require_optional_helpers(self):
        import os
        from pathlib import Path
        import shutil
        import subprocess
        import sys
        import tempfile
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'certificate_check.py'
            shutil.copyfile(Path(__file__).with_name('certificate_check.py'),path)
            env={**os.environ, 'PYTHONPATH':'', 'ZENO_FCIS_TELEMETRY':temporary}
            help_result=subprocess.run([sys.executable,'-I',str(path),'--help'],env=env,capture_output=True)
            self.assertEqual(help_result.returncode,0,help_result.stderr)
            refusal=subprocess.run([sys.executable,'-I',str(path),temporary,'absent.json'],env=env,capture_output=True)
            self.assertEqual(refusal.returncode,2,refusal.stderr)
            self.assertIn(b'certificate refused:',refusal.stderr)
            self.assertNotIn(b'ModuleNotFoundError',refusal.stderr)

    def test_solver_budgets_are_unknown(self):
        from unittest.mock import patch
        from solve import fixed_guard, unrealizable, reactive
        with patch('solve.SOLVER_WORK',0):
            self.assertEqual(certify(description('withdrawal'),self.tables)['status'],'UNKNOWN')
            self.assertEqual(fixed_guard(description('withdrawal'),self.tables)['status'],'UNKNOWN')
            self.assertEqual(reactive(description('withdrawal'),self.tables)['status'],'UNKNOWN')
            self.assertEqual(unrealizable(description('oracle-withholding'),{})['status'],'UNKNOWN')

    def test_baseline(self): self.assertEqual(check(self.packet,self.tables)['evidence'],'certified safety')
    def test_reachable_hole(self): self.rejected(lambda p:p['certificate']['W'].pop(),'closure')
    def test_product_move_omitted(self): self.rejected(lambda p:p['graph']['edges'][0].pop(),'move coverage')
    def test_initial_hole(self): self.rejected(lambda p:p['certificate']['W'].remove(0),'initial membership')
    def test_incomplete_product(self): self.rejected(lambda p:p['graph'].__setitem__('status','UNKNOWN'),'incomplete')


if __name__=='__main__': unittest.main()
