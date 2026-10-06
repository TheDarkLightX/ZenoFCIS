#!/usr/bin/env python3
"""Regression tests for complete-decision proof/coverage mutation controls."""
import unittest
import check_decision_v2 as gate
import verus_contract_controls as contract_controls
from test_verus_contract_controls import PIN,assert_strict_classification,report
from verus_coverage import require_coverage
import json
# Excerpt of the retained hosted weaken_construct_contract stderr (run 37404295156).
HOSTED_WEAKEN="""note: function body check: not all errors may have been reported; rerun with a higher value for --multiple-errors to find other potential errors in this function
   --> verification/verus/../../crates/zeno-fcis-synthesis/src/finite/execution_v2/composition/producer.rs:189:1
    |
189 | / pub(super) fn from_atoms<'a>(
    | |__________________________________^

error: postcondition not satisfied
   --> verification/verus/../../crates/zeno-fcis-synthesis/src/finite/execution_v2/composition/producer.rs:185:56
    |
185 | #[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures final(meter).limits==old(meter).limits,
    |                                                        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ failed this postcondition
...
229 |             return Err(Failure::Decision(e));
    |             -------------------------------- at this exit

error: postcondition not satisfied
   --> verification/verus/../../crates/zeno-fcis-synthesis/src/finite/execution_v2/composition/producer.rs:186:5
    |
186 | /     spec::from_atoms(d,ingress::spec::value(state),ingress::spec::value(command),ingress::spec::value(context),atoms@,
    | |__________________________________________________________________________________________________________________________^ failed this postcondition

error: aborting due to 2 previous errors
"""
class GateTests(unittest.TestCase):
    def test_controls_are_unique_and_source_bound(self):
        source=(gate.ROOT/gate.SUBJECT).read_text();spec=(gate.ROOT/gate.SPEC).read_text()
        controls=gate.mutations(source,spec)
        self.assertEqual(len(controls),15)
        self.assertEqual(sum(expected=='proof' for _,_,expected,_ in controls.values()),10)
        self.assertEqual(sum(expected=='proof_contract' for _,_,expected,_ in controls.values()),2)
        self.assertEqual(sum(native for _,_,_,native in controls.values()),10)
        for path,changed,expected,native in controls.values():
            self.assertNotEqual(changed,(gate.ROOT/path).read_text())
            self.assertIn(expected,('proof','proof_contract','coverage'))
    def test_construct_contract_controls_change_only_that_contract_and_classify_strictly(self):
        source=(gate.ROOT/gate.SUBJECT).read_text();spec=(gate.ROOT/gate.SPEC).read_text()
        header=source.index("pub(super) fn construct<'a>")
        start=source.rindex('#[cfg_attr(verus_keep_ghost, verus_spec(result =>',0,header)
        end=source.index('#[cfg_attr(verus_keep_ghost, verifier::rlimit',start)
        controls=gate.mutations(source,spec)
        self.assertEqual(set(gate.CONTRACT_REJECTIONS),{n for n,(_,_,e,_) in controls.items() if e=='proof_contract'})
        for name,expected in gate.CONTRACT_REJECTIONS.items():
            path,changed,_,native=controls[name]
            self.assertEqual((path,native),(gate.SUBJECT,False))
            self.assertTrue(changed.startswith(source[:start]) and changed.endswith(source[end:]),name)
            assert_strict_classification(self,expected)
        outcome=contract_controls.contract_rejected(1,report(errors=2),HOSTED_WEAKEN,PIN,
            gate.CONTRACT_REJECTIONS['weaken_construct_contract'])
        self.assertTrue(outcome['accepted'],outcome)
    def test_profile_covers_all_runtime_bodies_and_has_total_contracts(self):
        profile=json.loads((gate.ROOT/gate.PROFILE).read_text())
        runtime={name for name,v in profile['functions'].items() if v['mode']=='Exec'}
        self.assertEqual(runtime,set(profile['body_covered_functions']))
        for name in runtime:
            row=profile['functions'][name]
            self.assertEqual(row['requires'],0,name);self.assertGreater(row['ensures'],0,name)
            self.assertIn('body_sha256',row)
        for helper in ('construct','assignments','deliveries','payload','resolve','admitted','equal'):
            self.assertIn('decision::execution_v2::decision::'+helper,runtime)
if __name__=='__main__':unittest.main()
