#!/usr/bin/env python3
"""Regression tests for complete-decision proof/coverage mutation controls."""
import unittest
import check_decision_v2 as gate
from verus_coverage import require_coverage
import json
class GateTests(unittest.TestCase):
    def test_controls_are_unique_and_source_bound(self):
        source=(gate.ROOT/gate.SUBJECT).read_text();spec=(gate.ROOT/gate.SPEC).read_text()
        controls=gate.mutations(source,spec)
        self.assertEqual(len(controls),15)
        self.assertEqual(sum(expected=='proof' for _,_,expected,_ in controls.values()),10)
        self.assertEqual(sum(native for _,_,_,native in controls.values()),10)
        for path,changed,expected,native in controls.values():
            self.assertNotEqual(changed,(gate.ROOT/path).read_text())
            self.assertIn(expected,('proof','coverage'))
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
