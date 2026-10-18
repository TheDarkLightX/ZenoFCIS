"""Preparation/binding regressions; synthetic artifacts are not native evidence."""
from dataclasses import FrozenInstanceError
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import v2_native_dependencies as deps
from qualification_controls import run_controls


class DependencyBinding(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        for name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo/config.toml'):
            path = self.root/name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('fixture')
        self.rows = []
        self.target = self.root/'target'
        self.target.mkdir()
        for package in ('value', 'codec', 'collections'):
            base = self.root/'crates'/('zeno-fcis-'+package)
            (base/'src').mkdir(parents=True)
            (base/'Cargo.toml').write_text('fixture manifest')
            (base/'src/lib.rs').write_text('fixture source')
            artifact = self.target/('libzeno_fcis_'+package+'.rlib')
            artifact.write_bytes(package.encode())
            self.rows.append({'reason':'compiler-artifact', 'target':{'name':'zeno_fcis_'+package},
                              'manifest_path':str(base/'Cargo.toml'), 'fresh':False,
                              'filenames':[str(artifact)]})
        transitive = self.target/'libtransitive.rlib'
        transitive.write_bytes(b'transitive')
        self.rows.append({'reason':'compiler-artifact', 'target':{'name':'transitive'},
                          'fresh':True, 'filenames':[str(transitive)]})
        self.env = {'CARGO_TARGET_DIR':str(self.target)}

    def prepare(self):
        result = subprocess.CompletedProcess([], 0, '\n'.join(map(json.dumps, self.rows)), '')
        with patch.object(deps.verifier, 'run', return_value=result) as run:
            prepared = deps.prepare_native_dependencies(self.root, self.root/'common', self.env)
        self.assertEqual(run.call_count, 1)
        return prepared

    def test_prepare_once_then_pool_binds_private_transitive_artifacts_without_builds(self):
        prepared = self.prepare()
        with self.assertRaises(FrozenInstanceError):
            prepared.root = Path('/other')
        for path in self.target.glob('*.rlib'):
            path.write_bytes(b'another shared build')
        mtimes = {p:p.stat().st_mtime_ns for p in self.root.glob('crates/*/src/*.rs')}
        for jobs in (1, 2):
            def worker(item):
                return {'name':item[0], 'args':deps.native_dependency_args(
                    self.root, self.root/f'jobs-{jobs}'/item[0], self.env, prepared=prepared)}
            with patch.dict(os.environ, {'ZENO_FCIS_CONTROL_JOBS':str(jobs)}), \
                    patch.object(deps.verifier, 'run', side_effect=AssertionError('worker rebuilt dependencies')):
                rows = run_controls(worker, [('A',), ('B',)])
            self.assertEqual(rows[0]['args'], rows[1]['args'])
            self.assertEqual(rows[0]['args'], list(prepared.arguments))
        self.assertEqual(mtimes, {p:p.stat().st_mtime_ns for p in mtimes})
        self.assertEqual(len(prepared.artifacts), 4)
        self.assertTrue(all(path.stat().st_mode & 0o222 == 0 for path, _ in prepared.artifacts))
        receipt = json.loads(prepared.receipt.read_text())
        self.assertTrue(all(row['fresh'] is False for row in receipt['libraries'].values()))
        self.assertEqual(len(receipt['artifacts']), 4)

    def test_fresh_true_oracle_witness_remains_refused(self):
        for i in (0, 1, 2):
            with self.subTest(package=i):
                self.rows[i]['fresh'] = True
                result = subprocess.CompletedProcess([], 0, '\n'.join(map(json.dumps, self.rows)), '')
                with patch.object(deps.verifier, 'run', return_value=result), \
                        self.assertRaisesRegex(ValueError, 'fresh current-source provenance'):
                    deps.prepare_native_dependencies(self.root, self.root/f'bad-{i}', self.env)
                self.rows[i]['fresh'] = False

    def test_cargo_top_level_library_retains_hash_matched_link_sibling(self):
        library = Path(self.rows[1]['filenames'][0])
        hashed = self.target/'deps/libzeno_fcis_codec-cargo_hash.rlib'
        hashed.parent.mkdir()
        hashed.write_bytes(library.read_bytes())
        metadata = hashed.with_suffix('.rmeta')
        metadata.write_bytes(b'synthetic metadata')
        self.rows[1]['filenames'].append(str(metadata))
        prepared = self.prepare()
        private = dict(prepared.artifacts)
        retained = prepared.receipt.parent/'libraries'/hashed.name
        self.assertIn(retained, private)
        self.assertEqual(retained.read_bytes(), library.read_bytes())
        hashed.write_bytes(b'later shared target mutation')
        self.assertEqual(retained.read_bytes(), b'codec')
        deps.native_dependency_args(self.root,self.root/'worker',self.env,prepared=prepared)

    def test_missing_or_wrong_hashed_link_sibling_refuses_preparation(self):
        hashed = self.target/'deps/libzeno_fcis_codec-cargo_hash.rlib'
        hashed.parent.mkdir()
        metadata = hashed.with_suffix('.rmeta')
        metadata.write_bytes(b'synthetic metadata')
        self.rows[1]['filenames'].append(str(metadata))
        for index, content in enumerate((None,b'wrong library')):
            if content is not None:
                hashed.write_bytes(content)
            result = subprocess.CompletedProcess([],0,'\n'.join(map(json.dumps,self.rows)),'')
            with patch.object(deps.verifier,'run',return_value=result), \
                    self.assertRaisesRegex(ValueError,'differs from Cargo output'):
                deps.prepare_native_dependencies(self.root,self.root/f'bad-link-{index}',self.env)

    def test_authority_creates_absent_target_before_unchanged_preparation_checks(self):
        import ast
        import check_authority_v2 as authority
        check = next(n for n in ast.parse(Path(authority.__file__).read_text()).body
                     if isinstance(n, ast.FunctionDef) and n.name == 'check')
        start = next(i for i, n in enumerate(check.body) if isinstance(n, ast.Assign)
                     and any(isinstance(t, ast.Name) and t.id == 'target' for t in n.targets))
        end = next(i for i, n in enumerate(check.body) if isinstance(n, ast.Assign)
                   and any(isinstance(t, ast.Name) and t.id == 'dependencies' for t in n.targets))
        code = compile(ast.Module(body=check.body[start:end + 1], type_ignores=[]),
                       authority.__file__, 'exec')
        # Replay the reviewer's actual authority preparation block. Cargo's
        # synthetic messages exercise guards; they are not native evidence.
        for stale in (None, 0, 1):
            with self.subTest(stale=stale):
                target = self.root/f'absent-{stale}'
                env = {**self.env, 'CARGO_TARGET_DIR': str(target)}
                scope = {**vars(authority), 'ROOT': self.root,
                         'out': self.root/f'authority-{stale}', 'env': env}
                rows = json.loads(json.dumps(self.rows))
                if stale is not None:
                    rows[stale]['fresh'] = True
                def cargo(command, cwd, environment, **kwargs):
                    self.assertTrue(target.is_dir())
                    self.assertEqual(environment['CARGO_TARGET_DIR'], str(target))
                    for row in rows:
                        source = Path(row['filenames'][0])
                        artifact = target/source.name
                        artifact.write_bytes(source.read_bytes())
                        row['filenames'] = [str(artifact)]
                    return subprocess.CompletedProcess(command, 0,
                        '\n'.join(map(json.dumps, rows)), '')
                self.assertFalse(target.exists())
                with patch.object(deps.verifier, 'run', side_effect=cargo) as run:
                    if stale is not None:
                        with self.assertRaisesRegex(ValueError, 'fresh current-source provenance'):
                            exec(code, scope)
                    else:
                        exec(code, scope)
                self.assertEqual(run.call_count, 1)
                if stale is None:
                    prepared = scope['dependencies']
                    for path in target.glob('*.rlib'):
                        path.write_bytes(b'known bad shared replacement')
                    args = deps.native_dependency_args(self.root, self.root/'fresh-consumer',
                                                       env, prepared=prepared)
                    self.assertEqual(args, list(prepared.arguments))
                    self.assertFalse(any(str(target) in arg for arg in args))
                    private = prepared.artifacts[0][0]
                    private.chmod(0o644)
                    private.write_bytes(b'known bad private substitution')
                    with self.assertRaisesRegex(ValueError, 'binding changed'):
                        deps.native_dependency_args(self.root, self.root/'bad-consumer',
                                                    env, prepared=prepared)

    def test_changed_source_refuses_binding(self):
        prepared = self.prepare()
        (self.root/'crates/zeno-fcis-value/src/lib.rs').write_text('known bad changed source')
        with self.assertRaisesRegex(ValueError, 'binding changed'):
            deps.native_dependency_args(self.root, self.root/'consumer', self.env, prepared=prepared)

    def test_changed_transitive_artifact_refuses_binding(self):
        prepared = self.prepare()
        artifact = next(p for p, _ in prepared.artifacts if p.name == 'libtransitive.rlib')
        artifact.chmod(0o644)
        artifact.write_bytes(b'known bad substitution')
        with self.assertRaisesRegex(ValueError, 'binding changed'):
            deps.native_dependency_args(self.root, self.root/'consumer', self.env, prepared=prepared)

    def test_changed_preparation_receipt_refuses_binding(self):
        prepared = self.prepare()
        prepared.receipt.write_text('{}')
        with self.assertRaisesRegex(ValueError, 'binding changed'):
            deps.native_dependency_args(self.root, self.root/'consumer', self.env, prepared=prepared)

    def test_five_native_helpers_bind_preparation_inside_pool_without_cargo(self):
        import check_catalog_v2 as catalog
        import check_composition_v2 as composition
        import check_decision_v2 as decision
        import check_guarded_laws_v2 as guarded
        import check_schema_binding_v2 as schema
        prepared = self.prepare()
        for gate in (catalog, composition, decision, guarded, schema):
            built = []
            def run(command, cwd, environment, **kwargs):
                self.assertEqual(environment['CARGO_MANIFEST_DIR'],
                    str(cwd.resolve()/'crates/zeno-fcis-synthesis'))
                if command[0] == 'rustc':
                    built.append(command)
                    Path(command[command.index('-o')+1]).write_bytes(b'synthetic executable')
                else:
                    self.assertNotEqual(command[0], 'cargo')
                return subprocess.CompletedProcess(command, 0, 'test result: ok.', '')
            def worker(item):
                directory = self.root/gate.__name__/item[0]
                if gate is not catalog:
                    directory.mkdir(parents=True)
                if gate is catalog:
                    gate.native_tests(self.root, directory, self.env, prepared=prepared)
                elif gate is composition:
                    gate.native(self.root, self.env, directory, prepared=prepared)
                elif gate is decision:
                    gate.native(directory, self.env, self.root, prepared=prepared)
                elif gate is guarded:
                    gate.native_commands(directory, self.env, self.root, item[0], prepared=prepared)
                else:
                    gate.native_check(directory, self.root, {'runtime_rust':'1.97.1'}, self.env, prepared=prepared)
                return {'name':item[0]}
            with self.subTest(gate=gate.__name__), patch.object(gate, 'ROOT', self.root), \
                    patch.dict(os.environ, {'ZENO_FCIS_CONTROL_JOBS':'2'}), \
                    patch.object(deps.verifier, 'run', side_effect=run):
                self.assertEqual(run_controls(worker, [('A',), ('B',)]), [{'name':'A'}, {'name':'B'}])
            self.assertEqual(len(built), 2)
            for command in built:
                self.assertIn(prepared.arguments[1], command)
                self.assertTrue(all(arg in command for arg in prepared.arguments))


    def test_native_environment_overrides_foreign_manifest_without_mutating_input(self):
        env = {'CARGO_MANIFEST_DIR':'/wrong', 'other':'preserved'}
        for source in (self.root, self.root/'specimen'):
            bound = deps.native_test_environment(source,env)
            self.assertEqual(bound['CARGO_MANIFEST_DIR'],str(source/'crates/zeno-fcis-synthesis'))
            self.assertEqual(bound['other'],'preserved')
        self.assertEqual(env,{'CARGO_MANIFEST_DIR':'/wrong', 'other':'preserved'})

    def test_other_direct_test_compilers_bind_the_source_manifest_directory(self):
        import check_finite_execution as finite
        import check_metered_execution as metered
        import check_protected_input as protected
        import check_canonical_bytes as canonical
        import check_envelope_frame_v2 as envelope
        import check_laws_v2 as laws
        import check_checker as checker
        for gate in (finite,metered,protected,canonical,envelope,laws,checker):
            directory = self.root/gate.__name__
            directory.mkdir()
            env = {'CARGO_MANIFEST_DIR':'/known-wrong'}
            observed = []
            def run(command,cwd,environment,**kwargs):
                self.assertEqual(environment['CARGO_MANIFEST_DIR'],str(self.root/'crates/zeno-fcis-synthesis'))
                if '--version' in command:
                    return subprocess.CompletedProcess(command,0,'release: 1.97.1\n','')
                observed.append(command)
                raise RuntimeError('known bad native compilation refused')
            helper = gate.native_tests if gate is finite else gate.native if gate is checker else gate.native_checks
            with self.subTest(gate=gate.__name__), patch.object(gate,'ROOT',self.root), \
                    patch.object(gate,'native_dependency_args',return_value=[]), \
                    patch.object(deps.verifier,'run',side_effect=run), \
                    self.assertRaisesRegex(RuntimeError,'known bad native compilation refused'):
                helper(directory,{'runtime_rust':'1.97.1'},env)
            self.assertEqual(len(observed),1)
            self.assertEqual(env,{'CARGO_MANIFEST_DIR':'/known-wrong'})

    def test_remaining_three_workers_ignore_changed_shared_artifacts(self):
        import ast
        import check_original_output_v2 as output
        import check_publication_v2 as publication
        import check_authority_v2 as authority
        prepared = self.prepare()
        for path in self.target.glob('*.rlib'):
            path.write_bytes(b'known bad shared artifact after preparation')
        tree = ast.parse(Path(authority.__file__).read_text())
        check = next(n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name=='check')
        nodes = [n for n in check.body if
            (isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='rust' for t in n.targets))
            or (isinstance(n,ast.FunctionDef) and n.name=='native')]
        self.assertEqual(len(nodes),2)
        for jobs in (1,2,4):
            directory = self.root/f'remaining-{jobs}'
            directory.mkdir()
            scope = {**vars(authority),'ROOT':self.root,'out':directory,'env':self.env,
                'pin':{'runtime_rust':'1.97.1'},'dependencies':prepared}
            exec(compile(ast.Module(body=nodes,type_ignores=[]),authority.__file__,'exec'),scope)
            commands = []
            def run(command,cwd,environment,**kwargs):
                if command[0]=='rustc':
                    self.assertEqual(command[command.index('-L')+1],prepared.arguments[1])
                    self.assertTrue(all(value in command for value in prepared.arguments))
                    self.assertEqual(environment['CARGO_MANIFEST_DIR'],str(cwd/'crates/zeno-fcis-synthesis'))
                    commands.append(command)
                return subprocess.CompletedProcess(command,0,'synthetic native result','')
            def worker(item):
                name = item[0]
                for module,builder in ((output,output.native_build_command),(publication,publication.native_command)):
                    binding = directory/(name+module.__name__)
                    arguments = deps.native_dependency_args(self.root,binding,self.env,prepared=prepared)
                    binary = binding/'tests'
                    command = builder(self.root,binary,arguments)
                    module.run(command,self.root,self.env,binding,'build')
                scope['native'](self.root,name)
                return {'name':name}
            with patch.dict(os.environ,{'ZENO_FCIS_CONTROL_JOBS':str(jobs)}), \
                    patch.object(deps.verifier,'run',side_effect=run):
                self.assertEqual([r['name'] for r in run_controls(worker,[(n,) for n in 'ABCD'])],list('ABCD'))
            self.assertEqual(len(commands),12)


if __name__ == '__main__':
    unittest.main()
