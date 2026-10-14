"""Publication gate wiring; these tests are not proof or release authority."""
import json
import unittest
import check_publication_v2 as gate
from verus_coverage import descriptor_inventory, require_coverage

class PublicationGate(unittest.TestCase):
    def test_complete_runtime_contracts_and_raw_bodies(self):
        profile=json.loads((gate.ROOT/gate.PROFILE).read_text())
        functions=profile['functions']
        executable={n for n,r in functions.items() if r['mode']=='Exec'}
        self.assertEqual(executable,set(profile['body_covered_functions']))
        self.assertTrue(all(r['requires']==0 and r['ensures']>0 and 'body_sha256' in r for r in functions.values() if r['mode']=='Exec'))
        # S5: genesis and transition share authority::publication::finish.
        for suffix in ['::publish','::replay_publication','::publish_genesis','::replay_genesis_publication',
                       '::authority::publication::finish','::state_bytes','::delivery_list','::subject_bytes','::policy_bytes']:
            self.assertTrue(any(n.endswith(suffix) for n in executable),suffix)
        for substring in ['::canonical_v2::output::','::execution_v2::catalog::','::execution_v2::composition::','::execution_v2::decision::','::execution_v2::laws::']:
            self.assertTrue(any(substring in n for n in executable),substring)
        with self.assertRaises(ValueError): require_coverage('',profile)

    def test_source_inventory_and_approved_evaluator(self):
        self.assertEqual(set(gate.snapshot()),set(map(str,gate.SOURCES)))
        generation=gate.authority.check_generated_sources()
        self.assertEqual((generation['paths'],generation['pins']),(119,3))
        for path in [gate.SUBJECT,gate.SPEC,gate.PUBLIC_TEST,gate.HARNESS,gate.authority.EVALUATOR]:
            self.assertIn(path,gate.UNIT_SOURCES)
        for package in ['zeno-fcis-codec','zeno-fcis-value']:
            self.assertIn(f'crates/{package}/src/lib.rs',gate.snapshot())

    def test_meaningful_control_families_and_owned_scope(self):
        rows=gate.mutations()
        self.assertEqual(len(rows),17)
        self.assertEqual({kind for _,_,kind in rows.values()},{'native','proof','coverage'})
        for path,changed,_ in rows.values():
            self.assertIn(path,[gate.SUBJECT,gate.SPEC])
            self.assertNotEqual(changed,(gate.ROOT/path).read_text())
        for name in ['state_root','state_schema','state_exact_cap','channel_roots','delivery_payload','delivery_idempotency',
                     'delivery_order','outbox_lane','subject_idempotency','full_replay_comparison','reject_has_no_capability',
                     'reject_replay_comparison','missing_channel_refusal','encoding_refusal_class',
                     'weak_getter_contract','equivalent_link_spec','uncontracted_inventory']:
            self.assertIn(name,rows)

    def coverage_fixture(self):
        namespace = 'publication_v2::'
        prefix = namespace + 'execution_v2::authority::publication::'
        descriptor = namespace + 'execution_v2::composition::descriptor::'
        def function(name, *, mode='Exec', ensures='true', requires='', body='(42)'):
            return (f'(Function (Fun :path {name}) :mode {mode} :typ_bounds () '
                    f':params () :ret () :require ({requires}) :ensure ({ensures}) '
                    f':d () :body {body})')
        declarations = (function(descriptor + 'DescriptorData::model', mode='Spec',
                                 ensures='', body='None') +
                        function(descriptor + 'DescriptorData::project', body='None'))
        concrete = ''.join(function(descriptor + f'impl&%{index}::{method}',
                                   mode=mode, ensures='' if mode == 'Spec' else 'true')
                           for index in (2, 3)
                           for method, mode in (('model', 'Spec'), ('project', 'Exec')))
        source = declarations + concrete + function(prefix + 'impl&%0::ordinal')
        source += function(prefix + 'spec::link', mode='Spec', ensures='')
        raw, _ = descriptor_inventory(source, namespace, descriptor)
        bodies = [n for n, row in raw.items() if row['mode'] == 'Exec']
        functions, abstract = descriptor_inventory(source, namespace, descriptor, tuple(bodies))
        profile = {'namespace': namespace, 'functions': functions,
                   'abstract_declarations': abstract, 'body_covered_functions': bodies}
        require_coverage(source, profile)
        drift = source.replace(function(descriptor + 'DescriptorData::project', body='None'),
                               function(descriptor + 'DescriptorData::project', ensures='changed-read-id', body='None'))
        return source, drift, profile, prefix, function

    def test_field_controls_require_exact_inventory_and_the_named_field(self):
        source, drift, profile, prefix, function = self.coverage_fixture()
        for name, target, original, changed, wrong_field in (
                ('weak_getter_contract', prefix + 'impl&%0::ordinal',
                 function(prefix + 'impl&%0::ordinal'),
                 function(prefix + 'impl&%0::ordinal', ensures='weakened'),
                 function(prefix + 'impl&%0::ordinal', body='(43)')),
                ('equivalent_link_spec', prefix + 'spec::link',
                 function(prefix + 'spec::link', mode='Spec', ensures=''),
                 function(prefix + 'spec::link', mode='Spec', ensures='', body='(43)'),
                 function(prefix + 'spec::link', mode='Spec', ensures='wrong'))):
            with self.subTest(control=name):
                for base in (source, drift):
                    caught, refusal, detail = gate.coverage_control(base.replace(original, changed), profile, name)
                    self.assertTrue(caught)
                    self.assertEqual(detail['function'], target)
                    self.assertTrue(detail['intended_refusal'])
                    if base == drift:
                        self.assertEqual(refusal, 'translated descriptor declarations changed')
                    for refused in (base, base.replace(original, wrong_field),
                                    base.replace(original, changed) + function(prefix + 'unexpected'),
                                    base.replace(original, ''),
                                    base.replace(original, changed).replace(
                                        function(prefix + 'spec::link', mode='Spec', ensures=''), '')
                                    if name == 'weak_getter_contract' else
                                    base.replace(original, changed).replace(function(prefix + 'impl&%0::ordinal'), '')):
                        self.assertFalse(gate.coverage_control(refused, profile, name)[0])

    def test_uncontracted_control_requires_one_added_exec_without_a_contract(self):
        source, drift, profile, prefix, function = self.coverage_fixture()
        name = 'uncontracted_inventory'
        target = prefix + 'uncontracted_publication_probe'
        probe = function(target, ensures='')
        for base in (source, drift):
            with self.subTest(descriptor_drift=base == drift):
                caught, refusal, detail = gate.coverage_control(base + probe, profile, name)
                self.assertTrue(caught)
                self.assertEqual(detail['function'], target)
                self.assertEqual(detail['field'], 'inventory')
                if base == drift:
                    self.assertEqual(refusal, 'translated descriptor declarations changed')
                for refused in (base, base + function(prefix + 'wrong_probe', ensures=''),
                                base + probe + function(prefix + 'unexpected'),
                                base.replace(function(prefix + 'impl&%0::ordinal'), '') + probe,
                                base + function(target, mode='Spec', ensures=''),
                                base + function(target, mode='Proof', ensures=''),
                                base + function(target, requires='narrowed', ensures=''),
                                base + function(target, ensures='contracted')):
                    self.assertFalse(gate.coverage_control(refused, profile, name)[0])

    def test_stale_or_ambiguous_mutation_anchor_refuses(self):
        with self.assertRaises(ValueError):gate.once('x x','x','y')
        with self.assertRaises(ValueError):gate.once('x','y','z')

if __name__=='__main__':unittest.main()
