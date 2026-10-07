"""Publication gate wiring; these tests are not proof or release authority."""
import json
import unittest
import check_publication_v2 as gate
from verus_coverage import require_coverage

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
        self.assertEqual((generation['paths'],generation['pins']),(90,3))
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

    def test_stale_or_ambiguous_mutation_anchor_refuses(self):
        with self.assertRaises(ValueError):gate.once('x x','x','y')
        with self.assertRaises(ValueError):gate.once('x','y','z')

if __name__=='__main__':unittest.main()
