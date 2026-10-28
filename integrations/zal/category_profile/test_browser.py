"""Actual static category export/replay; explicit operator fixtures, no worker."""
from copy import deepcopy
import json
import hashlib
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

from category_profile import host
from zal_notation import abstractions as A, core, lens_library as L, library_lens_browser as B


class AttachmentBoundaryTests(unittest.TestCase):
    def request(self):
        entry = A.make_entry('draft', 'Infinite object', story='An authored infinite group proposal.')
        library = A.register(A.new_library(), entry)
        return {'schema': B.SCHEMA, 'library': library, 'registry': L.new_registry(),
                'entries': [core.digest(entry)], 'checks': [], 'proposals': [],
                'category_operations': [{'entry_digest': core.digest(entry), 'application': {}, 'record': {}}]}

    def test_missing_or_json_operator_context_refuses_before_output(self):
        for context in (None, {}, {'evidence': 'self-enrolled'}):
            with self.subTest(context=context), tempfile.TemporaryDirectory() as directory:
                target = Path(directory)/'refused'
                with self.assertRaises(ValueError):
                    B.export(self.request(), target, category_host=context)
                self.assertFalse(target.exists())
        plain = self.request(); plain['category_operations'] = []
        _, packet = B.artifacts(plain)
        self.assertNotIn('--category-evidence-catalog', packet['recheckCommand'])

    def test_attachment_shape_duplicates_and_request_approval_injection_refuse(self):
        for change in (lambda r: r.update(category_host={}),
                       lambda r: r['category_operations'][0].update(runtime='/tmp/not-selected'),
                       lambda r: r['category_operations'].append(deepcopy(r['category_operations'][0])),
                       lambda r: r['category_operations'][0].update(entry_digest=True)):
            request = self.request(); change(request)
            with self.assertRaises(ValueError):
                B.validate_request(request)


class StaticCategoryBrowserTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        names = ['ZENO_CATEGORY_BROWSER_REQUEST', 'ZENO_CATEGORY_BROWSER_EVIDENCE', 'ZENO_CATEGORY_BROWSER_EVIDENCE_SHA256']
        if not all(os.environ.get(n) for n in names):
            raise unittest.SkipTest('explicit source-bound request/operator evidence fixture not supplied')
        cls.H = host.load_adapter()
        cls.request = core.loads(Path(os.environ[names[0]]).read_text())
        cls.operator = host.operator_context(evidence_catalog=Path(os.environ[names[1]]), evidence_sha256=os.environ[names[2]])
        cls.workspace = tempfile.TemporaryDirectory(prefix='category-browser-static-')
        cls.root = Path(cls.workspace.name)
        with patch.object(cls.H.BACKEND, 'execute', side_effect=AssertionError('native execution forbidden')), \
             patch('subprocess.run', side_effect=AssertionError('any subprocess forbidden')):
            cls.export = B.export(cls.request, cls.root/'viewer', category_host=cls.operator)
        cls.packet = json.loads((cls.root/'viewer/data/packet.json').read_bytes())

    @classmethod
    def tearDownClass(cls):
        cls.workspace.cleanup()

    def test_actual_category_functor_symbolic_negative_and_incomplete_axes(self):
        self.assertIn('--category-evidence-catalog OPERATOR_APPROVAL.json', self.packet['recheckCommand'])
        self.assertIn('--category-evidence-sha256 OPERATOR_APPROVED_RAW_SHA256', self.packet['recheckCommand'])
        self.assertNotIn(os.environ['ZENO_CATEGORY_BROWSER_EVIDENCE'], self.packet['recheckCommand'])
        views = [v for s in self.packet['subjects'] for v in s['lenses'] if v.get('category')]
        self.assertEqual(len(views), len(self.request['category_operations']))
        self.assertEqual({v['category']['application']['operation'] for v in views}, set(self.H.OPERATIONS))
        statuses = {v['category']['record']['status'] for v in views}
        self.assertEqual(statuses, {'CheckedAt', 'RefutedAt', 'Authored', 'UNKNOWN'})
        for view in views:
            record, audit = view['category']['record'], view['category']['audit']
            self.assertEqual(record['status'], audit['recorded_status'])
            if record['status'] == 'UNKNOWN':
                self.assertEqual((view['status'], audit['status'], view['operations']), ('incomplete', 'RecordedIncomplete', None))
                self.assertEqual(audit['static_table_audit'], 'not performed: incomplete prior result')
            elif record['status'] == 'RefutedAt':
                self.assertEqual((view['status'], view['operations']), ('refuted', None))
                self.assertIn('unselected declarations', record['refutation_scope'])
            elif record['status'] == 'Authored':
                self.assertEqual(view['status'], 'authored')
                self.assertEqual(view['category']['application']['interpretation']['status'], 'Authored')
        self.assertTrue(any(v['status'] == 'checked' and v['category']['application']['interpretation']['status'] == 'Authored' for v in views))
        self.assertTrue(any(s['object']['meaning']['status'] == 'draft' for s in self.packet['subjects']))

    def test_worker_free_replay_and_deterministic_export(self):
        with patch.object(self.H.BACKEND, 'execute', side_effect=AssertionError('native execution forbidden')), \
             patch('subprocess.run', side_effect=AssertionError('any subprocess forbidden')):
            result = B.replay(self.root/'viewer', self.export['request_sha256'], category_host=self.operator)
            B.export(self.request, self.root/'second', category_host=self.operator)
        self.assertEqual(result['status'], 'replayed')
        files = lambda p: {str(f.relative_to(p)): f.read_bytes() for f in p.rglob('*') if f.is_file()}
        self.assertEqual(files(self.root/'viewer'), files(self.root/'second'))
        with self.assertRaises(ValueError):
            B.replay(self.root/'viewer', self.export['request_sha256'])

    def test_operator_repinned_false_results_and_malformed_incomplete_refuse(self):
        H = self.H
        mutations = [
            ('check-category', lambda n: n['result'].update(objects=True)),
            ('check-category', lambda n: n['result'].update(objects=1.0)),
            ('compose-path', lambda n: n['result'].update(composite='not-a-composite')),
            ('UNKNOWN', lambda n: n.update(result={'manufactured': True})),
            ('UNKNOWN', lambda n: n.update(work_used=True)),
            ('UNKNOWN', lambda n: n.update(work_used=1.0)),
            ('UNKNOWN', lambda n: n.update(work_used=n['work_used']-1)),
            ('UNKNOWN', lambda n: n.update(message='all laws proved')),
            ('UNKNOWN', lambda n: n['worker'].update(binary_sha256='0'*64)),
        ]
        for number, (operation, mutate) in enumerate(mutations):
            request = deepcopy(self.request)
            row = next(r for r in request['category_operations'] if
                       (r['record']['status'] == 'UNKNOWN' if operation == 'UNKNOWN' else
                        r['record']['status'] == 'CheckedAt' and r['application']['operation'] == operation))
            native = deepcopy(row['record']['native_receipt']); mutate(native)
            row['record'] = H._record(row['application'], native, 'fresh-native')
            evidence = self.operator.evidence
            approved = H.EvidenceApproval(evidence.record_digests | {H._digest(row['record'])}, evidence.binary_sha256, evidence.build_record_sha256)
            with self.subTest(operation=operation, number=number), self.assertRaises(ValueError):
                B.export(request, self.root/f'forged-{number}', category_host=host.OperatorContext(evidence=approved))
            self.assertFalse((self.root/f'forged-{number}').exists())

    def test_incomplete_provenance_calls_no_table_or_worker_checker(self):
        H = self.H
        request = deepcopy(self.request)
        request['category_operations'] = [r for r in request['category_operations'] if r['record']['status'] == 'UNKNOWN']
        captured = H.BACKEND.captured()
        with patch.object(captured[2], 'audit', side_effect=AssertionError('incomplete must not claim a table audit')), \
             patch.object(H.BACKEND, 'captured', return_value=captured), \
             patch.object(H.BACKEND, 'execute', side_effect=AssertionError('native execution forbidden')):
            B.export(request, self.root/'incomplete-only', category_host=self.operator)

    def test_request_context_catalog_and_packet_mutations_refuse(self):
        for number, mutate in enumerate([
            lambda r: r['category_operations'][0]['application'].update(context_digest='0'*64),
            lambda r: r['category_operations'][0]['record'].update(status='Authored'),
            lambda r: r['library'].update(label='different source library'),
        ]):
            request = deepcopy(self.request); mutate(request)
            with self.subTest(number=number), self.assertRaises(ValueError):
                B.export(request, self.root/f'swap-{number}', category_host=self.operator)
            self.assertFalse((self.root/f'swap-{number}').exists())
        empty = self.H.EvidenceApproval(frozenset(), self.operator.evidence.binary_sha256, self.operator.evidence.build_record_sha256)
        with self.assertRaises(ValueError):
            B.export(self.request, self.root/'unapproved', category_host=host.OperatorContext(evidence=empty))

    def test_resealed_packet_result_approval_and_scope_mutations_refuse(self):
        for name in ['result', 'approval', 'scope']:
            folder = self.root/('resealed-'+name)
            shutil.copytree(self.root/'viewer', folder)
            packet = json.loads((folder/'data/packet.json').read_bytes())
            view = next(v for s in packet['subjects'] for v in s['lenses'] if v.get('category'))
            if name == 'result':
                view['category']['record']['status'] = 'Authored'
            elif name == 'approval':
                packet['categoryApproval']['declaration']['records'] = []
            else:
                view['category']['audit']['static_table_audit'] = 'universal laws proved'
            (folder/'data/packet.json').write_text(json.dumps(packet))
            manifest = json.loads((folder/'manifest.json').read_bytes())
            manifest['files'] = {str(p.relative_to(folder)): hashlib.sha256(p.read_bytes()).hexdigest()
                                 for p in folder.rglob('*') if p.is_file() and p.name != 'manifest.json'}
            (folder/'manifest.json').write_text(json.dumps(manifest))
            with self.subTest(name=name), self.assertRaises(ValueError):
                B.replay(folder, self.export['request_sha256'], category_host=self.operator)


if __name__ == '__main__':
    unittest.main(verbosity=2)
