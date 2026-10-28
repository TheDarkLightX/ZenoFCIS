"""Focused product host/source/approval controls; optional existing native runtime."""
from copy import deepcopy
from dataclasses import FrozenInstanceError
import os
from pathlib import Path
import tempfile
import unittest

from category_profile import host, tools


class CategoryProfileTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.H = host.load_adapter()

    def setUp(self):
        H = self.H
        self.category = {'id': 'One', 'objects': ['O'], 'arrows': [{'id': 'e', 'source': 'O', 'target': 'O'}],
                         'identities': [['O', 'e']], 'composition': [['e', 'e', 'e']]}
        ctx = H.category_context(self.category)
        self.entry = H.A.make_entry('object', 'Explicit source', story='Finite declaration; original program meaning unresolved.',
            discussion=H.sharing.make_discussion(ctx, H.builtin.generic_pack(ctx['grammar'])), open_questions=['What program meaning is intended?'])
        self.library = H.A.register(H.A.new_library(), self.entry)
        self.identity = H.core.digest(self.entry)
        self.parameters = {'categories': [self.category], 'functors': [], 'relationships': [], 'selection': {'category': 'One'}}
        self.interpretation = {'mode': 'formal-category-source', 'description': 'Exact declaration only.',
                               'assumptions': [], 'preserves': ['All category rows.'], 'forgets': []}
        self.app = H.prepare(self.library, self.identity, 'check-category', self.parameters, self.interpretation)

    def configured(self):
        path = os.environ.get('ZENO_CATEGORY_TEST_RUNTIME_APPROVAL')
        if not path:
            self.skipTest('explicit trusted existing-runtime approval not supplied; no runtime is auto-selected')
        return host.operator_context(runtime_config=Path(path))

    def test_shared_tool_schemas_and_real_prefix_routing_without_runtime(self):
        H = self.H
        from zal_notation import sharing
        names = {r['name'] for r in sharing.toolkit()['tools']}
        self.assertTrue({r['name'] for r in tools.toolkit()['tools']} <= names)
        result = sharing.dispatch_tool('zal_library_category_check',
            {'library': self.library, 'entry_digest': self.identity, 'application': self.app})
        self.assertEqual(result['status'], 'Unavailable')
        result = sharing.dispatch_tool('zal_library_category_check',
            {'library': self.library, 'entry_digest': self.identity, 'application': self.app, 'runtime': '/tmp/worker'})
        self.assertEqual(result['status'], 'Invalid')
        self.assertEqual(H.check(self.library, self.identity, self.app)['status'], 'Unavailable')

    def test_generic_hint_remains_pending_and_infinite_objects_remain_authorable(self):
        H = self.H
        entry = H.A.make_entry('naturals', 'Natural numbers', story='An infinite carrier with symbolic successor.',
                               open_questions=['Which universal laws and proof profile should be supplied?'])
        lib = H.A.register(H.A.new_library(), entry)
        report = H.L.discover(lib, H.core.digest(entry))
        row = next(r for r in report['lenses'] if r['definition']['id'] == 'typed-category')
        self.assertEqual(row['status'], 'pending')
        self.assertEqual(row['evidence'], 'not-checked')
        self.assertEqual(report['entry_status']['status'], 'draft')

    def test_full_model_binding_and_open_meaning_axes(self):
        H = self.H
        self.assertEqual(self.app['interpretation']['status'], 'ExactRepresentation')
        self.assertEqual(self.app['entry_status']['status'], 'draft')
        changed = deepcopy(self.parameters); changed['categories'][0]['composition'][0][2] = 'other'
        app = H.prepare(self.library, self.identity, 'check-category', changed, self.interpretation)
        self.assertEqual(app['interpretation']['status'], 'Unsupported')
        self.assertEqual(H.check(self.library, self.identity, app)['status'], 'Unsupported')
        changed_app = deepcopy(self.app); changed_app['context']['assumptions'].append('swap')
        self.assertEqual(H.check(self.library, self.identity, changed_app)['status'], 'Invalid')

    def test_operator_approvals_are_immutable_and_have_no_default_runtime(self):
        H = self.H
        approval = H.RuntimeApproval(Path('/tmp/explicit-operator-runtime'), '0'*64, '1'*64)
        self.assertTrue(approval.directory.is_absolute())
        with self.assertRaises(FrozenInstanceError):
            approval.directory = Path('/tmp/changed')
        with self.assertRaises(FrozenInstanceError):
            approval.binary_sha256 = '2'*64
        evidence = H.EvidenceApproval(frozenset(), '0'*64, '1'*64)
        with self.assertRaises(FrozenInstanceError):
            evidence.record_digests = frozenset({'2'*64})
        self.assertFalse(hasattr(H.RuntimeApproval, 'qualified_prototype'))
        self.assertEqual(host.dispatch_tool('zal_library_category_check',
            {'library': self.library, 'entry_digest': self.identity, 'application': self.app},
            context={'runtime': approval})['status'], 'Invalid')

    def test_source_only_preparation_is_fresh_exact_and_builds_nothing(self):
        H = self.H
        with tempfile.TemporaryDirectory(prefix='category-sources-') as directory:
            destination = Path(directory)/'prepared'
            result = host.prepare_runtime(destination)
            self.assertEqual((result['status'], result['files'], result['worker_built']), ('Prepared', 10, False))
            self.assertFalse((destination/'.build').exists())
            prefix = 'category_profile/provider/'
            for name, blob in H._BLOBS.items():
                if name.startswith(prefix):
                    self.assertEqual((destination/name[len(prefix):]).read_bytes(), blob)
            with self.assertRaises(ValueError):
                host.prepare_runtime(destination)

    def test_source_preparation_normalizes_dotdot_before_containment(self):
        H = self.H
        destination = H.ZAL_ROOT.parent/'..'/'integrations'/'zal'/'unexpected-runtime'
        actual = destination.resolve()
        self.assertFalse(actual.exists())
        with self.assertRaises(ValueError):
            host.prepare_runtime(destination)
        self.assertFalse(actual.exists(), 'refused destination must write no source files')
        with tempfile.TemporaryDirectory(prefix='category-link-') as directory:
            parent = Path(directory)
            (parent/'link').symlink_to(parent, target_is_directory=True)
            with self.assertRaises(ValueError):
                host.prepare_runtime(parent/'link'/'prepared')
            self.assertFalse((parent/'prepared').exists())

    def test_fractional_notation_is_retained_in_bounded_exact_source_chunks(self):
        H = self.H
        entry = deepcopy(self.entry)
        entry['story'] = 'x'*2000 + ' Réparation\r\ndesk'
        entry['discussion']['notation']['glyphs']['Compose']['primitives'][0]['x'] = 8.5
        library = H.A.register(H.A.new_library(), entry)
        H.A.validate_library(library)
        identity = H.core.digest(entry)
        app = H.prepare(library, identity, 'check-category', self.parameters, self.interpretation)
        source = app['request']['source_context']['source_entry']
        self.assertGreater(len(H.core.canonical(entry)), 4096)
        self.assertEqual(source['encoding'], 'zal/canonical-json-text-chunks/1')
        self.assertEqual(source['sha256'], identity)
        self.assertTrue(all(len(part) <= 2048 for part in source['chunks']))
        self.assertEqual(''.join(source['chunks']), H.core.canonical(entry))
        self.assertEqual(H.core.loads(''.join(source['chunks'])), entry)
        self.assertEqual(app['context'], self.app['context'])
        self.assertEqual(app['interpretation']['status'], 'ExactRepresentation')
        context = self.configured()
        result = H.check(library, identity, app, runtime=context.runtime)
        self.assertEqual(result['status'], 'CheckedAt')
        self.assertTrue(H.verify_receipt(library, identity, app, result, runtime=context.runtime))
        approval = H.EvidenceApproval(frozenset({H._digest(result)}), context.runtime.binary_sha256, context.runtime.build_record_sha256)
        self.assertEqual(H.audit_record(library, identity, app, result, evidence=approval)['status'], 'Audited')
        for field, value in [('chunks', ['altered']), ('sha256', '0'*64), ('encoding', 'unsupported')]:
            changed = deepcopy(app); changed['request']['source_context']['source_entry'][field] = value
            self.assertEqual(H.check(library, identity, changed, runtime=context.runtime)['status'], 'Invalid')
        changed = deepcopy(app); changed['source_entry']['discussion']['notation']['glyphs']['Compose']['primitives'][0]['x'] = 8
        self.assertEqual(H.check(library, identity, changed, runtime=context.runtime)['status'], 'Invalid')

    def test_configured_native_replay_and_typed_static_falsifiers(self):
        H = self.H
        context = self.configured()
        from zal_notation import sharing
        result = sharing.dispatch_tool('zal_library_category_check',
            {'library': self.library, 'entry_digest': self.identity, 'application': self.app}, category_host=context)
        self.assertEqual(result['status'], 'CheckedAt')
        self.assertEqual(result['interpretation']['status'], 'ExactRepresentation')
        self.assertTrue(H.verify_receipt(self.library, self.identity, self.app, result, runtime=context.runtime))
        forged = deepcopy(result); forged['status'] = 'Authored'
        self.assertFalse(H.verify_receipt(self.library, self.identity, self.app, forged, runtime=context.runtime))
        approval = H.EvidenceApproval(frozenset({H._digest(result)}), context.runtime.binary_sha256, context.runtime.build_record_sha256)
        self.assertEqual(H.audit_record(self.library, self.identity, self.app, result, evidence=approval)['status'], 'Audited')
        for alias in (True, 1.0):
            native = deepcopy(result['native_receipt']); native['result']['objects'] = alias
            record = H._record(self.app, native, 'fresh-native')
            false_approval = H.EvidenceApproval(frozenset({H._digest(record)}), context.runtime.binary_sha256, context.runtime.build_record_sha256)
            self.assertEqual(H.audit_record(self.library, self.identity, self.app, record, evidence=false_approval)['status'], 'Invalid')


if __name__ == '__main__':
    unittest.main(verbosity=2)
