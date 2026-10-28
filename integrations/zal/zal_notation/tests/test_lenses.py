from copy import deepcopy
from dataclasses import FrozenInstanceError
import unittest

from zal_notation import lenses as L


class LensTests(unittest.TestCase):
    def setUp(self):
        self.source = L.make_counter_source()

    def check_lens(self, lens, parameters=None, status='checked'):
        app = L.apply(self.source, lens, parameters)
        result = L.check(self.source, app)
        self.assertEqual(result['status'], status)
        self.assertTrue(L.verify_receipt(self.source, app, result))
        return app, result

    def test_catalog_is_not_universal(self):
        catalog = L.discover(self.source)
        self.assertFalse(catalog['universal_completeness'])
        self.assertEqual(catalog['enumeration'], 'registered-candidates-only')
        self.assertNotIn('checked', {x['status'] for x in catalog['lenses']})

    def test_source_has_exact_counter_rows(self):
        c = L.source_contract(self.source)
        table = c.table()
        self.assertEqual(len(table), 8)
        self.assertEqual(table[('unseen', 'inc', 'yes')], (('ok', 'one', 'done'),))
        self.assertEqual(table[('three', 'inc', 'yes')], (('ok', 'unseen', 'done'),))
        self.assertEqual(table[('two', 'reset', 'yes')], (('ok', 'unseen', 'done'),))

    def test_native_fold_is_exact_presentation(self):
        expanded, _ = self.check_lens('definition-expand')
        folded, _ = self.check_lens('definition-fold')
        self.assertEqual(expanded['target_context'], folded['target_context'])
        self.assertNotEqual(L.render_application(self.source, expanded), L.render_application(self.source, folded))
        for app in (expanded, folded):
            source = L.exchange.recover_source(L.render_application(self.source, app))
            self.assertEqual(source['model'], self.source['model'])

    def test_invalid_fold_path_refused(self):
        with self.assertRaises(L.core.NotationError):
            L.apply(self.source, 'definition-fold', {'paths': ['root.missing']})

    def test_native_table_round_trip(self):
        app, _ = self.check_lens('finite-table')
        self.assertEqual(L.finite.parse_table(app['payload']['controlled_table'], True), L.source_contract(self.source))

    def test_native_bridge_preserves_rows(self):
        app, r = self.check_lens('state-machine')
        self.assertEqual(r['evidence']['rows_recovered'], 8)
        self.assertEqual(len(app['target_context']['model']['body']['edges']), 8)

    def test_native_quotient_checks_all_eight_rows(self):
        app, r = self.check_lens('parity-quotient')
        self.assertEqual(r['evidence']['native_result']['cases'], 8)
        self.assertEqual(r['evidence']['native_result']['abstract_states'], 2)
        self.assertIn('concrete-state', app['target_context']['observation']['omits'])
        self.assertNotEqual(app['source_context_digest'], app['target_context_digest'])

    def test_wrong_quotient_is_refuted_with_witness(self):
        app, r = self.check_lens('parity-quotient', {'mapping': {'unseen':'a','one':'a','two':'b','three':'b'}}, 'refuted')
        self.assertIsNone(app['target_context'])
        self.assertIn('witness', r['evidence']['native_result'])

    def test_partial_map_is_unsupported(self):
        with self.assertRaises(L.Unsupported):
            L.apply(self.source, 'parity-quotient', {'mapping': {'unseen': 'even'}})

    def test_monoid_has_eight_effects(self):
        app, r = self.check_lens('transformation-monoid')
        self.assertEqual(r['evidence']['carrier_size'], 8)
        self.assertEqual(len(app['payload']['algebra']['elements']), 8)

    def test_reset_refutes_whole_effect_group(self):
        _, r = self.check_lens('all-effects-group', status='refuted')
        self.assertIn('noninvertible_effect', r['evidence'])
        self.assertIn('does not refute', r['evidence']['nonclaim'])

    def test_units_are_restricted_four_element_group(self):
        app, r = self.check_lens('unit-group')
        self.assertEqual(r['evidence']['carrier_size'], 4)
        self.assertIn('two-sided inverses', r['evidence']['finite_laws'])
        algebra = app['payload']['algebra']
        self.assertEqual(len(algebra['units']), 4)
        self.assertNotIn(algebra['generators']['["reset","yes"]'], algebra['units'])

    def test_category_is_explicit_one_object(self):
        app, r = self.check_lens('one-object-category')
        params = app['target_context']['semantics']['parameters']
        self.assertEqual(params['objects'], ['object'])
        self.assertEqual(len(params['arrows']), 8)
        self.assertEqual(r['evidence']['category_objects'], 1)

    def test_semantic_zoom_never_mutates_source(self):
        original = deepcopy(self.source)
        for lens_id in L.SPECS:
            if lens_id in ('execution-tree', 'automorphism-group'):
                continue
            L.apply(self.source, lens_id)
        self.assertEqual(self.source, original)

    def test_retained_snapshot_is_immutable(self):
        snapshot = L.SourceSnapshot.capture(self.source)
        self.source['assumptions'].clear()
        self.assertEqual(len(snapshot.context['assumptions']), 4)
        copy = snapshot.context
        copy['assumptions'].clear()
        self.assertEqual(len(snapshot.context['assumptions']), 4)
        with self.assertRaises(FrozenInstanceError):
            snapshot._json = '{}'

    def test_restore_uses_retained_source_not_inverse(self):
        snapshot = L.SourceSnapshot.capture(self.source)
        app = snapshot.apply('parity-quotient')
        self.assertEqual(snapshot.restore(app), self.source)
        with self.assertRaises(L.Unsupported):
            L.zoom_back(app['target_context'], app)

    def test_assumption_observation_model_mutations_stale(self):
        app = L.apply(self.source, 'parity-quotient')
        for mutate in [lambda s: s['assumptions'].append('extra'),
                       lambda s: s['observation'].update(description='changed'),
                       lambda s: s['model']['body']['fields'].update(name='changed')]:
            edited = deepcopy(self.source)
            mutate(edited)
            self.assertEqual(L.check(edited, app)['status'], 'unsupported')

    def test_valid_different_source_cannot_reuse_receipt(self):
        app, r = self.check_lens('state-machine')
        c = L.counter_contract()
        rows = c.table()
        rows[('one', 'inc', 'yes')] = (('ok', 'three', 'done'),)
        changed = L.make_source(L.finite.build(c.inputs, c.outputs, rows, c.state))
        self.assertFalse(L.verify_receipt(changed, app, r))

    def test_mapping_mutation_and_reseal_rejected(self):
        app = L.apply(self.source, 'parity-quotient')
        app['parameters']['mapping']['one'] = 'even'
        app['mapping_digest'] = L.digest(app['parameters'])
        self.assertEqual(L.check(self.source, app)['status'], 'unsupported')

    def test_target_mutation_and_reseal_rejected(self):
        app = L.apply(self.source, 'state-machine')
        app['target_context']['model']['body']['edges'][0]['target'][0] = 'three'
        app['target_context_digest'] = L.digest(app['target_context'])
        self.assertEqual(L.check(self.source, app)['status'], 'unsupported')

    def test_claimed_status_and_revision_are_not_trusted(self):
        app = L.apply(self.source, 'parity-quotient')
        for key, value in [('status', 'checked'), ('lens_revision', '0'*64), ('preserves', ['everything'])]:
            edited = deepcopy(app)
            edited[key] = value
            self.assertEqual(L.check(self.source, edited)['status'], 'unsupported')

    def test_receipt_tampering_replayed_not_hash_trusted(self):
        app, receipt = self.check_lens('parity-quotient')
        edited = deepcopy(receipt)
        edited['evidence']['native_result']['cases'] = 999
        self.assertFalse(L.verify_receipt(self.source, app, edited))

    def test_zero_budget_is_incomplete_not_refuted(self):
        for lens in ('parity-quotient', 'transformation-monoid', 'definition-fold'):
            self.assertEqual(L.check(self.source, L.apply(self.source, lens), 0)['status'], 'incomplete')

    def test_future_lenses_are_unsupported_not_refuted(self):
        for lens in ('execution-tree', 'automorphism-group'):
            with self.assertRaises(L.Unsupported):
                L.apply(self.source, lens)
        self.assertEqual({c['status'] for c in L.discover(self.source)['lenses'] if c['id'] in ('execution-tree','automorphism-group')}, {'unsupported'})

    def test_unknown_code_directed_dispatch_is_refused(self):
        with self.assertRaises(L.Unsupported):
            L.apply(self.source, 'os.system', {'command': 'echo unsafe'})
        with self.assertRaises(L.Unsupported):
            L.dispatch({'action': 'import', 'module': 'os'})
        with self.assertRaises(L.Unsupported):
            L.dispatch({'action': 'discover', 'source': self.source, 'executable': 'evil.py'})
        with self.assertRaises(L.Unsupported):
            L.apply(self.source, 'state-machine', {'executable': 'evil.py'})



if __name__ == '__main__':
    unittest.main()
