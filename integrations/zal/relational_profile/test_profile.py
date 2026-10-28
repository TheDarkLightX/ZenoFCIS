"""Focused bridge and transport controls; fake workers do not qualify Haskell."""
from __future__ import annotations

import copy
import importlib.util
import json
import os
import py_compile
import shutil
import subprocess
import sys
import tempfile
import types
import unittest
from pathlib import Path
from unittest.mock import patch

if not __package__:
    sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from integrations.zal.relational_profile import bridge, profile

_MANIFEST, _ = profile.verify_provider()
upstream, oracle = profile._load_vendor(_MANIFEST)

ROOT = Path(__file__).resolve().parent
CHECKOUT = ROOT.parents[2]
CONTEXT = ROOT / 'vendor/examples/owner_cars_context.json'
OPT = ['-O'] if sys.flags.optimize else []


def count_query():
    return {'schema': 'zal/relational-query/1', 'logic': 'zal/c2/1', 'signature': [],
            'formula': {'op': 'true'}, 'operation': 'count', 'domain_size': '1',
            'weights': {}, 'context_id': 'transport-fixture'}


def count_answer():
    return {'status': 'ExactCount', 'value': '1', 'domain_size': '1',
            'engine': 'population-definitions/1', 'work': '1',
            'peak_entries': '1', 'peak_bits': '1',
            'assurance': 'experimental; source-level proof open'}


def comparison():
    atom = {'op': 'atom', 'predicate': 'P', 'args': ['x']}
    return {'schema': 'zal/relational-comparison/1', 'logic': 'zal/c2/1',
            'signature': [{'id': 'P', 'arity': 1}], 'domain_size': '1',
            'context_id': 'transport-fixture', 'assumptions': {'op': 'true'},
            'left': {'op': 'exists', 'var': 'x', 'body': atom},
            'right': {'op': 'true'}, 'relation': 'equivalence'}


def refutation():
    return {'status': 'RefutedAt', 'holds': False, 'vacuous': False,
            'relation': 'equivalence', 'domain_size': '1', 'enumerated_models': '2',
            'admitted_models': '2', 'partition': {'both_true': '1', 'left_only': '0',
                                                   'right_only': '1', 'both_false': '0'},
            'counterexample': {'model': {'P': []}, 'assumptions': True,
                               'left': False, 'right': True},
            'work_bound': '10', 'engine': 'complete-structure-comparison/1',
            'scope': profile.COMPARISON_SCOPE}


class ProviderControls(unittest.TestCase):
    def test_exact_vendor_and_unavailable_runtime(self):
        manifest, sources = profile.verify_provider()
        self.assertEqual(len(manifest['files']), 32)
        self.assertEqual(len(sources), 8)
        with tempfile.TemporaryDirectory() as tmp:
            runtime = Path(tmp) / 'runtime'
            prepared = profile.prepare_runtime(runtime)
            self.assertFalse(prepared['worker_built'])
            self.assertEqual(profile.execute(count_query(), runtime)['status'], 'Unavailable')
            with self.assertRaises(profile.ProfileFailure):
                profile.prepare_runtime(runtime)

    def test_invalid_and_unsupported_are_not_success(self):
        missing = '/not-a-created-relational-runtime'
        for raw, expected in [
            (b'{"schema":"x","schem\\u0061":"y"}', 'Invalid'),
            (b'{}', 'Invalid'),
            (b'{"schema":"other"}', 'Unsupported'),
            (b'{"schema":[]}', 'Invalid'),
        ]:
            result = profile.execute(raw, missing)
            self.assertEqual(result['status'], expected)
            self.assertNotIn('result', result)
        unsupported = dict(count_query(), logic='zal/another-profile/1')
        self.assertEqual(profile.execute(unsupported, missing)['status'], 'Unsupported')
        self.assertEqual(profile.execute(count_query(), missing, seconds=0)['status'], 'Invalid')
        self.assertEqual(profile.execute(count_query(), missing, limits={'work': '999'})['status'], 'Invalid')
        too_large = dict(profile.DEFAULT_LIMITS, domain='10001')
        self.assertEqual(profile.execute(count_query(), missing, limits=too_large)['status'], 'Invalid')
        smaller = dict(profile.DEFAULT_LIMITS, domain='1')
        self.assertEqual(profile.execute(dict(count_query(), domain_size='2'), missing, limits=smaller)['status'], 'UNKNOWN')
        deep = {'op': 'true'}
        for _ in range(257):
            deep = {'op': 'not', 'body': deep}
        self.assertEqual(profile.execute(dict(count_query(), formula=deep), missing)['status'], 'UNKNOWN')

    def test_vendor_bytes_are_checked_before_their_code_executes(self):
        for name in ('run.py', 'oracle.py'):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as tmp:
                base = Path(tmp)
                package = base / 'integrations/zal/relational_profile'
                package.parent.mkdir(parents=True)
                shutil.copytree(ROOT, package)
                marker = base / 'must-not-execute'
                with (package / 'vendor' / name).open('ab') as handle:
                    handle.write(('\nopen(' + repr(str(marker)) + ', "w").write("executed")\n').encode())
                process = subprocess.run([sys.executable, *OPT, '-I', '-B', str(package / '__main__.py'),
                                          'run', str(base / 'absent-runtime'), '-'],
                                         input=json.dumps(count_query()).encode(),
                                         capture_output=True, timeout=10, check=False)
                self.assertEqual(process.returncode, 2, process.stderr.decode(errors='replace'))
                answer = json.loads(process.stdout)
                self.assertEqual(answer['status'], 'Error')
                self.assertIn('provider source changed', answer['detail'])
                self.assertFalse(marker.exists())

    def test_poisoned_bytecode_and_unlisted_package_initializer_never_execute(self):
        for name in ('run', 'oracle'):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as tmp:
                base = Path(tmp)
                package = base / 'integrations/zal/relational_profile'
                package.parent.mkdir(parents=True)
                shutil.copytree(ROOT, package)
                target = package / 'vendor' / (name + '.py')
                marker = base / 'poison-executed'
                initializer_marker = base / 'initializer-executed'
                initializer = package / 'vendor/__init__.py'
                initializer.write_text('open(' + repr(str(initializer_marker)) + ', "w").write("executed")\n')
                malicious = ('open(' + repr(str(marker)) + ', "w").write("executed")\n').encode()
                original = target.stat()
                self.assertLess(len(malicious), original.st_size)
                fake = base / 'fake.py'
                fake.write_bytes(malicious + b' ' * (original.st_size - len(malicious)))
                os.utime(fake, ns=(original.st_atime_ns, original.st_mtime_ns))
                cache = Path(importlib.util.cache_from_source(str(target)))
                cache.parent.mkdir(exist_ok=True)
                py_compile.compile(str(fake), cfile=str(cache), dfile=str(target), doraise=True,
                                   invalidation_mode=py_compile.PycInvalidationMode.TIMESTAMP)
                # A normal source loader really accepts this same-size, same-mtime
                # poisoned cache: prove the negative control is effective first.
                control = subprocess.run([sys.executable, *OPT, '-I', '-B', '-c',
                    ('import importlib.util,sys; s=importlib.util.spec_from_file_location("control",sys.argv[1]); '
                    'm=importlib.util.module_from_spec(s); s.loader.exec_module(m)'), str(target)],
                    capture_output=True, timeout=10, check=False)
                self.assertEqual(control.returncode, 0, control.stderr.decode(errors='replace'))
                self.assertTrue(marker.exists(), 'poisoned cache must be effective in the negative control')
                marker.unlink()
                process = subprocess.run([sys.executable, *OPT, '-I', '-B', str(package / '__main__.py'),
                                          'run', str(base / 'absent-runtime'), '-'],
                                         input=json.dumps(count_query()).encode(),
                                         capture_output=True, timeout=10, check=False)
                self.assertEqual(process.returncode, 2, process.stderr.decode(errors='replace'))
                self.assertEqual(json.loads(process.stdout)['status'], 'Unavailable')
                self.assertFalse(marker.exists())
                self.assertFalse(initializer_marker.exists())

    def test_standalone_ignores_ancestor_initializers_and_bound_module_caches(self):
        for poisoned in ('profile', 'bridge'):
            with self.subTest(poisoned=poisoned), tempfile.TemporaryDirectory() as tmp:
                base = Path(tmp)
                package = base / 'integrations/zal/relational_profile'
                package.parent.mkdir(parents=True)
                shutil.copytree(ROOT, package)
                for name in ('ladder.py', 'ladder_en.json'):
                    shutil.copyfile(ROOT.parent / name, package.parent / name)
                ancestor_markers = []
                for parent in (base / 'integrations', base / 'integrations/zal'):
                    marker = parent / 'initializer-executed'
                    ancestor_markers.append(marker)
                    (parent / '__init__.py').write_text('open(' + repr(str(marker)) + ', "w").write("executed")\n')
                target = package / (poisoned + '.py')
                marker = base / 'poison-executed'
                malicious = ('open(' + repr(str(marker)) + ', "w").write("executed")\n').encode()
                original = target.stat()
                self.assertLess(len(malicious), original.st_size)
                fake = base / 'fake.py'
                fake.write_bytes(malicious + b' ' * (original.st_size - len(malicious)))
                os.utime(fake, ns=(original.st_atime_ns, original.st_mtime_ns))
                cache = Path(importlib.util.cache_from_source(str(target)))
                cache.parent.mkdir(exist_ok=True)
                py_compile.compile(str(fake), cfile=str(cache), dfile=str(target), doraise=True,
                                   invalidation_mode=py_compile.PycInvalidationMode.TIMESTAMP)
                control = subprocess.run([sys.executable, *OPT, '-I', '-B', '-c',
                    ('import importlib.util,sys; s=importlib.util.spec_from_file_location("control",sys.argv[1]); '
                    'm=importlib.util.module_from_spec(s); s.loader.exec_module(m)'), str(target)],
                    capture_output=True, timeout=10, check=False)
                self.assertEqual(control.returncode, 0, control.stderr.decode(errors='replace'))
                self.assertTrue(marker.exists(), 'negative control must execute the poisoned cache')
                marker.unlink()
                process = subprocess.run([sys.executable, *OPT, '-I', '-B', str(package / '__main__.py'),
                    'lower', '--context', str(package / 'vendor/examples/owner_cars_context.json'),
                    '--text', 'every owner has a car.', '--rung', 'intuitive', '--domain-size', '2'],
                    capture_output=True, timeout=10, check=False)
                self.assertEqual(process.returncode, 0, process.stderr.decode(errors='replace'))
                answer = json.loads(process.stdout)
                self.assertEqual(answer['status'], 'Lowered')
                self.assertFalse(marker.exists())
                self.assertTrue(all(not path.exists() for path in ancestor_markers))

    def test_standalone_rejects_modified_local_modules_before_execution(self):
        for name in ('profile', 'bridge'):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as tmp:
                base = Path(tmp)
                package = base / 'integrations/zal/relational_profile'
                package.parent.mkdir(parents=True)
                shutil.copytree(ROOT, package)
                marker = base / 'must-not-execute'
                with (package / (name + '.py')).open('ab') as handle:
                    handle.write(('\nopen(' + repr(str(marker)) + ', "w").write("executed")\n').encode())
                process = subprocess.run([sys.executable, *OPT, '-I', '-B', str(package / '__main__.py'),
                                          'run', str(base / 'absent'), '-'], input=b'{}',
                                         capture_output=True, timeout=10, check=False)
                self.assertEqual(process.returncode, 2, process.stderr.decode(errors='replace'))
                answer = json.loads(process.stdout)
                self.assertEqual(answer['status'], 'Error')
                self.assertIn('standalone source changed: ' + name, answer['detail'])
                self.assertFalse(marker.exists())

    def test_oversize_file_and_stdin_both_report_unknown(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            raw = b' ' * (profile.MAX_BYTES + 1)
            source = root / 'oversize.json'
            source.write_bytes(raw)
            base = [sys.executable, *OPT, '-I', '-B', str(ROOT / '__main__.py'), 'run', str(root / 'absent')]
            for suffix, payload in [(str(source), None), ('-', raw)]:
                process = subprocess.run([*base, suffix], input=payload, capture_output=True,
                                         timeout=10, check=False)
                self.assertEqual(process.returncode, 2, process.stderr.decode(errors='replace'))
                self.assertEqual(json.loads(process.stdout)['status'], 'UNKNOWN')

    def test_vacuity_and_replayed_refutation_are_distinct(self):
        q = comparison()
        witness = refutation()
        self.assertEqual(profile._validate_answer(q, witness, profile.DEFAULT_LIMITS, upstream, oracle), 'RefutedAt')
        forged = copy.deepcopy(witness)
        # Structurally plausible and consistent with the reported histogram,
        # but this particular model makes P true, contradicting the claimed left.
        forged['counterexample']['model'] = {'P': [['0']]}
        with self.assertRaises(profile.ProfileFailure) as cm:
            profile._validate_answer(q, forged, profile.DEFAULT_LIMITS, upstream, oracle)
        self.assertEqual(cm.exception.status, 'Error')
        vacuous = dict(witness, status='VacuousAt', holds=None, vacuous=True,
                       admitted_models='0', counterexample=None,
                       partition={'both_true': '0', 'left_only': '0', 'right_only': '0', 'both_false': '0'})
        self.assertEqual(profile._validate_answer(q, vacuous, profile.DEFAULT_LIMITS, upstream, oracle), 'VacuousAt')
        partial = copy.deepcopy(witness)
        partial['enumerated_models'] = '1'
        with self.assertRaises(RuntimeError):
            profile._validate_answer(q, partial, profile.DEFAULT_LIMITS, upstream, oracle)


class TransportFixtureControls(unittest.TestCase):
    """Synthetic subprocess responses exercise the boundary, not its mathematics."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.runtime = self.root / 'runtime'
        profile.prepare_runtime(self.runtime)

    def worker(self, code):
        binary = self.runtime / '.build/zal-relational-worker'
        binary.parent.mkdir(exist_ok=True)
        binary.write_text('#!/usr/bin/python3\n' + code + '\n')
        binary.chmod(0o755)
        _, sources = profile.verify_provider()
        evidence = self.runtime / 'evidence'
        evidence.mkdir(exist_ok=True)
        record = {'schema': 'zal/relational-build/1', 'source_sha256': sources,
                  'binary_sha256': profile._sha(binary.read_bytes()),
                  'ghc': 'synthetic test fixture, not GHC', 'exit': 0}
        (evidence / 'build.json').write_text(json.dumps(record))

    def respond(self, value):
        self.worker('import sys\nsys.stdin.readline()\nsys.stdout.write(' + repr(json.dumps(value) + '\n') + ')')

    def test_round_trip_and_worker_failure_separation(self):
        self.respond(count_answer())
        result = profile.execute(count_query(), self.runtime, seconds=5, scratch=self.root)
        self.assertEqual(result['status'], 'ExactCount')
        self.assertEqual(result['result']['value'], '1')
        self.assertEqual(result['context_id'], 'transport-fixture')
        self.assertEqual(result['worker']['ghc'], 'synthetic test fixture, not GHC')
        self.assertEqual(result['profile_binding'], 'trusted-local-import')
        process = subprocess.run([sys.executable, *OPT, '-I', '-B', str(ROOT / '__main__.py'),
                                  'run', str(self.runtime), '-', '--seconds', '5', '--scratch', str(self.root)],
                                 input=json.dumps(count_query()).encode(), capture_output=True,
                                 timeout=10, check=False)
        self.assertEqual(process.returncode, 0, process.stderr.decode(errors='replace'))
        standalone = json.loads(process.stdout)
        self.assertEqual(standalone['status'], 'ExactCount')
        self.assertEqual(standalone['profile_binding'], 'captured-standalone')
        self.assertEqual(standalone['profile_sha256'], profile._sha((ROOT / 'profile.py').read_bytes()))
        self.respond({'status': 'Unsupported', 'message': 'fixture unsupported'})
        result = profile.execute(count_query(), self.runtime, seconds=5, scratch=self.root)
        self.assertEqual(result['status'], 'Unsupported')
        self.assertNotIn('result', result)
        self.respond({'status': 'ResourceExhausted', 'resource': 'work', 'used': '2', 'limit': '1'})
        result = profile.execute(count_query(), self.runtime, seconds=5, scratch=self.root)
        self.assertEqual(result['status'], 'UNKNOWN')
        self.assertNotIn('result', result)
        self.respond({'status': 'Invalid', 'message': 'failure', 'value': '1'})
        result = profile.execute(count_query(), self.runtime, seconds=5, scratch=self.root)
        self.assertEqual(result['status'], 'Error')
        self.assertNotIn('result', result)

    def test_malformed_wrong_scope_and_noncanonical_responses_refused(self):
        self.worker("import sys\nsys.stdin.readline()\nsys.stdout.write('{not json}\\n')")
        self.assertEqual(profile.execute(count_query(), self.runtime, seconds=5, scratch=self.root)['status'], 'Error')
        for answer in [dict(count_answer(), value='1/1'), dict(count_answer(), domain_size='2'),
                       dict(count_answer(), work='10000001'), dict(count_answer(), status='Mystery')]:
            self.respond(answer)
            result = profile.execute(count_query(), self.runtime, seconds=5, scratch=self.root)
            self.assertEqual(result['status'], 'Error')
            self.assertNotIn('result', result)

    def test_actual_timeout_has_no_result(self):
        self.worker('import sys,time\nsys.stdin.readline()\ntime.sleep(5)')
        result = profile.execute(count_query(), self.runtime, seconds=1, scratch=self.root)
        self.assertEqual(result['status'], 'UNKNOWN')
        self.assertNotIn('result', result)

    def test_output_bound_does_not_admit_partial_json(self):
        self.worker("import sys\nsys.stdin.readline()\nsys.stdout.write('x' * (1024*1024+10))\nsys.stdout.flush()")
        result = profile.execute(count_query(), self.runtime, seconds=5, scratch=self.root)
        self.assertIn(result['status'], ('UNKNOWN', 'Error'))
        self.assertNotIn('result', result)

    def test_source_inventory_binary_and_symlink_drift_refused(self):
        self.respond(count_answer())
        source = self.runtime / 'src/Zal/Logic/Syntax.hs'
        original = source.read_bytes()
        source.write_bytes(original + b'\n-- drift\n')
        self.assertEqual(profile.execute(count_query(), self.runtime, seconds=5, scratch=self.root)['status'], 'Error')
        source.write_bytes(original)
        extra = self.runtime / 'src/Extra.hs'
        extra.write_text('module Extra where\n')
        self.assertEqual(profile.execute(count_query(), self.runtime, seconds=5, scratch=self.root)['status'], 'Error')
        extra.unlink()
        link = self.runtime / 'src/linked'
        link.symlink_to(self.runtime / 'app', target_is_directory=True)
        self.assertEqual(profile.execute(count_query(), self.runtime, seconds=5, scratch=self.root)['status'], 'Error')
        link.unlink()
        binary = self.runtime / '.build/zal-relational-worker'
        with binary.open('ab') as handle:
            handle.write(b'# drift\n')
        self.assertEqual(profile.execute(count_query(), self.runtime, seconds=5, scratch=self.root)['status'], 'Error')


class LadderBridgeControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Load the exact upstream adapter against the current 2.7 ladder. Its
        # local import of run is supplied explicitly and restored afterwards.
        name = 'run'
        old = sys.modules.get(name)
        sys.modules[name] = upstream
        try:
            path = ROOT / 'vendor/ladder_adapter.py'
            source = path.read_bytes()
            if profile._sha(source) != _MANIFEST['files']['ladder_adapter.py']['sha256']:
                raise RuntimeError('retained adapter changed')
            cls.adapter = types.ModuleType('retained_pr153_adapter')
            cls.adapter.__file__ = str(path)
            exec(compile(source, str(path), 'exec'), cls.adapter.__dict__)  # noqa: S102 - pinned adapter test
        finally:
            if old is None:
                sys.modules.pop(name, None)
            else:
                sys.modules[name] = old
        cls.path = CHECKOUT / 'integrations/zal/ladder.py'
        source = cls.path.read_bytes()
        spec = importlib.util.spec_from_file_location('current_ladder_for_relational_test', cls.path)
        cls.ladder = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = cls.ladder
        exec(compile(source, str(cls.path), 'exec'), cls.ladder.__dict__)  # noqa: S102 - local parser test
        cls.ctx = cls.ladder.load(CONTEXT)

    def test_current_ladder_all_rungs_match_retained_adapter(self):
        texts = [('intuitive', 'every owner has a car.'),
                 ('typed', 'forall x in D: Owner(x) => (exists1 y in D: Car(y) and Has(x, y))'),
                 ('typed', 'forall x in D: Owner(x) => #{y in D: Car(y) and Has(x, y)} = 2')]
        checked = 0
        for rung, text in texts:
            tree = self.ladder.parse(text, rung, self.ctx)
            for level in self.ladder.RUNGS:
                recovered = self.ladder.parse(self.ladder.render(tree, level, self.ctx), level, self.ctx)
                actual = bridge.lower_tree(self.ladder, self.ctx, recovered, 2)
                expected = self.adapter.translate(self.ladder, self.ctx, recovered, 2)
                self.assertEqual(actual['context_id'], 'unbound-ladder:' + self.ladder.scope_identity(self.ctx))
                expected['context_id'] = actual['context_id']
                expected['signature'].sort(key=lambda predicate: predicate['id'])
                self.assertEqual(actual, expected)
                checked += 1
        self.assertEqual(checked, 21)

    def test_third_variable_and_primed_predicates_are_not_aliased(self):
        text = 'forall x in D: forall y in D: forall z in D: (Has(x, y) and Has(y, z)) => Has(x, z)'
        result = bridge.lower_text(CONTEXT, text, 'typed', 2)
        self.assertEqual(result['status'], 'Unsupported')
        self.assertIn('third live variable', result['detail'])
        with self.assertRaises(bridge.UnsupportedLowering):
            bridge._lower(('call', 'Owner', True, (('var', 'x'),)), self.ladder, 'D', {'x': 'x'})

    def test_context_binding_and_existing_fixture_translation(self):
        result = bridge.lower_text(CONTEXT, 'every owner has a car.', 'intuitive', 2)
        self.assertEqual(result['status'], 'Lowered')
        self.assertEqual(result['human_semantic_approval'], 'not inferred')
        self.assertEqual(result['authority'], 'none')
        self.assertEqual(result['bindings']['context_sha256'], profile._sha(CONTEXT.read_bytes()))
        self.assertEqual(result['bindings']['ladder_sha256'], profile._sha(self.path.read_bytes()))
        self.assertEqual(result['bindings']['registry_sha256'], profile._sha(self.path.with_name('ladder_en.json').read_bytes()))
        tree = self.ladder.parse('every owner has a car.', 'intuitive', self.ctx)
        for fixture in self.ctx.data['fixtures']:
            labels = fixture['domains']['D']
            model = {name: {tuple(labels.index(label) for label in row) for row, truth in data['rows'] if truth}
                     for name, data in fixture['symbols'].items()}
            query = bridge.lower_tree(self.ladder, self.ctx, tree, len(labels))
            # Python-to-Python translation control, not Haskell qualification.
            self.assertEqual(oracle.evaluate(query['formula'], model, len(labels)), fixture['expected'])
            self.assertEqual(self.ladder.evaluate(tree, self.ctx, fixture), fixture['expected'])

    def test_comparison_requires_bound_matching_context_and_query(self):
        lowered = bridge.lower_text(CONTEXT, 'every owner has a car.', 'intuitive', 2)
        assumptions = bridge.lower_text(CONTEXT, 'forall x in D: x = x', 'typed', 2)
        self.assertEqual(lowered['schema'], 'zal/relational-lowering/3')
        self.assertEqual(lowered['bindings']['context_identity']['schema'], 'zal/relational-ladder-context/2')
        result = bridge.comparison_request(lowered, lowered, assumptions, 'implication')
        self.assertEqual(result['relation'], 'implication')
        self.assertEqual(result['context_id'], lowered['query']['context_id'])
        with self.assertRaises(ValueError):
            bridge.comparison_request(lowered['query'], lowered, assumptions)
        old_envelope = copy.deepcopy(lowered)
        old_envelope['schema'] = 'zal/relational-lowering/2'
        with self.assertRaises(ValueError):
            bridge.comparison_request(lowered, old_envelope, assumptions)
        old_descriptor = copy.deepcopy(lowered)
        old_descriptor['bindings']['context_identity']['schema'] = 'zal/relational-ladder-context/1'
        with self.assertRaises(ValueError):
            bridge.comparison_request(lowered, old_descriptor, assumptions)
        different_size = bridge.lower_text(CONTEXT, 'every owner has a car.', 'intuitive', 3)
        with self.assertRaises(ValueError):
            bridge.comparison_request(lowered, different_size, assumptions)
        weighted = copy.deepcopy(lowered)
        weighted['query']['weights']['Owner'] = ['2', '1']
        weighted['bindings']['query_sha256'] = profile._sha(bridge._canonical(weighted['query']))
        with self.assertRaises(ValueError):
            bridge.comparison_request(lowered, weighted, assumptions)
        for field in ('query', 'tree', 'descriptor'):
            altered = copy.deepcopy(lowered)
            if field == 'query':
                altered['query']['formula'] = {'op': 'true'}
            elif field == 'tree':
                altered['tree'] = ['bool', True]
            else:
                altered['bindings']['context_identity']['scope_sha256'] = '0' * 64
            with self.subTest(field=field), self.assertRaises(ValueError):
                bridge.comparison_request(lowered, altered, assumptions)

    def test_context_scope_is_canonical_but_changed_definition_is_incomparable(self):
        original = json.loads(CONTEXT.read_text())
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            reordered = root / 'reordered.json'
            reordered.write_text(json.dumps(original, sort_keys=True, separators=(',', ':')))
            changed = copy.deepcopy(original)
            changed['definitions'][0]['body'] = 'exists y in D: Car(y)'
            different = root / 'different.json'
            different.write_text(json.dumps(changed))
            text = 'exists x in D: Owner(x)'
            left = bridge.lower_text(CONTEXT, text, 'typed', 2)
            same = bridge.lower_text(reordered, text, 'typed', 2)
            other = bridge.lower_text(different, text, 'typed', 2)
            self.assertEqual([q['status'] for q in (left, same, other)], ['Lowered'] * 3)
            self.assertNotEqual(left['bindings']['context_sha256'], same['bindings']['context_sha256'])
            self.assertEqual(left['bindings']['context_identity'], same['bindings']['context_identity'])
            self.assertEqual(left['query'], same['query'])
            self.assertNotEqual(left['query']['context_id'], other['query']['context_id'])
            self.assertEqual(left['query']['formula'], other['query']['formula'])
            self.assertEqual(bridge.comparison_request(left, same, left)['context_id'], left['query']['context_id'])
            with self.assertRaises(ValueError):
                bridge.comparison_request(left, other, left)
            with self.assertRaises(ValueError):
                bridge.comparison_request(left, same, other)

    def test_scope_keeps_exact_json_number_tokens_and_types(self):
        original = json.loads(CONTEXT.read_text())
        original['extra_scope'] = {'value': 'scope-token', 'other': 'preserved'}
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            def lower(token, sorted_keys=False):
                path = root / ('context-' + str(len(list(root.iterdir()))) + '.json')
                raw = json.dumps(original, sort_keys=sorted_keys).replace('"scope-token"', token)
                path.write_text(raw)
                packet = bridge.lower_text(path, 'exists x in D: Owner(x)', 'typed', 2)
                self.assertEqual(packet['status'], 'Lowered', packet)
                return packet
            pairs = [('9007199254740992.0', '9007199254740993.0'), ('1e-400', '0.0'),
                     ('1e1000000', '1e1000001'),
                     ('1', '1.0'), ('1.0', '1.00'), ('1.0', '"1.0"'),
                     ('true', '1'), ('null', '"null"'), ('[1]', '{"0":1}')]
            for first, second in pairs:
                with self.subTest(first=first, second=second):
                    left, right = lower(first), lower(second)
                    self.assertNotEqual(left['query']['context_id'], right['query']['context_id'])
                    with self.assertRaises(ValueError):
                        bridge.comparison_request(left, right, left)
            left, reordered = lower('1e-400'), lower('1e-400', sorted_keys=True)
            self.assertNotEqual(left['bindings']['context_sha256'], reordered['bindings']['context_sha256'])
            self.assertEqual(left['query']['context_id'], reordered['query']['context_id'])
            self.assertEqual(bridge.comparison_request(left, reordered, left)['context_id'], left['query']['context_id'])

    def test_ladder_registry_is_read_from_captured_temporary_capsule(self):
        original = Path.read_text
        seen = []
        def track(path, *args, **kwargs):
            if path.name == 'ladder_en.json':
                seen.append(path)
            return original(path, *args, **kwargs)
        with patch.object(Path, 'read_text', track):
            answer = bridge.lower_text(CONTEXT, 'every owner has a car.', 'intuitive', 2)
        self.assertEqual(answer['status'], 'Lowered')
        self.assertEqual(len(seen), 1)
        self.assertNotEqual(seen[0], self.path.with_name('ladder_en.json'))
        self.assertFalse(seen[0].exists())


if __name__ == '__main__':
    unittest.main(verbosity=2)
