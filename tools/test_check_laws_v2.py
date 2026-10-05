"""Guard the actual law proof inventory and mutation anchors, not proof fixtures."""
import json
from pathlib import Path
import unittest
from unittest.mock import patch
import check_laws_v2 as gate
import check_verus as verifier

class LawCoverage(unittest.TestCase):
    def test_every_runtime_body_and_total_contract_is_covered(self):
        profile=json.loads((gate.ROOT/gate.PROFILE).read_text())
        runtime={name for name,data in profile['functions'].items() if data['mode']=='Exec'}
        self.assertEqual(len(profile['functions']),1085)
        self.assertEqual(len(runtime),468)
        self.assertEqual(set(profile['body_covered_functions']),runtime)
        self.assertTrue(all(v['requires']==0 and v['ensures']>0 for v in profile['functions'].values() if v['mode']=='Exec'))
        for suffix in ('laws::evaluate','laws::evaluate_into','laws::frame::observe','laws::predicate::node','laws::atoms::divide'):
            self.assertTrue(any(name.endswith(suffix) for name in runtime),suffix)
        self.assertTrue(any(name.endswith('spec::success_checks_every_applicable') for name in profile['functions']))

    def test_meaningful_controls_remain_distinct_from_coverage_controls(self):
        mutants=gate.mutation_sources()
        self.assertEqual(len(mutants),28)
        self.assertEqual(sum(kind=='proof' for _,_,kind in mutants.values()),23)
        self.assertEqual(mutants['observe_before_read_permission'][2],'coverage')
        for path,changed,kind in mutants.values():
            self.assertIn(path,gate.UNIT_SOURCES)
            self.assertNotEqual(changed,(gate.ROOT/path).read_text())
            self.assertIn(kind,('proof','coverage'))

    def test_forged_report_cannot_omit_law_entry_or_completeness_theorem(self):
        profile=json.loads((gate.ROOT/gate.PROFILE).read_text())
        pin=json.loads((gate.ROOT/verifier.PIN).read_text())
        pin.update({key:profile[key] for key in ('expected_verified','target_functions')})
        report={'verus':{'commit':pin['commit'],'version':pin['version']},
            'verification-results':{'success':True,'errors':0,'verified':profile['expected_verified'],'encountered-error':False,
                'encountered-vir-error':False,'is-verifying-entire-crate':True},
            'func-details':dict.fromkeys(pin['target_functions'],{})}
        self.assertTrue(verifier.accepted(report,pin))
        for missing in pin['target_functions']:
            changed={**report,'func-details':{k:v for k,v in report['func-details'].items() if k!=missing}}
            self.assertFalse(verifier.accepted(changed,pin))
        changed={**report,'verification-results':{**report['verification-results'],'is-verifying-entire-crate':False}}
        self.assertFalse(verifier.accepted(changed,pin))

    def test_every_declared_source_is_hashed_and_symlinks_refuse(self):
        self.assertEqual(set(gate.snapshot()),set(map(str,gate.SOURCES)))
        with patch.object(Path,'is_symlink',return_value=True):
            with self.assertRaisesRegex(RuntimeError,'symbolic link'):gate.snapshot()

if __name__=='__main__':unittest.main()
