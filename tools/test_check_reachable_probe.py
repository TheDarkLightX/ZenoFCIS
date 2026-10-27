#!/usr/bin/env python3
"""Admission, source binding and control attribution for the reachable-probe gate."""
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import check_reachable_probe as gate


class Admission(unittest.TestCase):
    def setUp(self):
        self.profile=json.loads((gate.ROOT/gate.PROFILE).read_text())
        self.pin=json.loads((gate.ROOT/gate.verifier.PIN).read_text())
        self.report={'verification-results':{'success':True,'verified':self.profile['expected_verified'],
            'errors':0,'encountered-error':False,'encountered-vir-error':False,'is-verifying-entire-crate':True},
            'verus':{k:self.pin[k] for k in ('version','commit')},
            'func-details':{n:{} for n in self.profile['target_functions']}}
        self.report['verus']['toolchain']=self.pin['rust_toolchain']
        self.proc=subprocess.CompletedProcess([],0,'','')

    def accepted(self):
        return gate.lemma.positive_accepted(self.proc,self.report,self.pin,self.profile)

    def test_exact_pinned_whole_crate_only(self):
        self.assertTrue(self.accepted())
        for count in (True,0,self.profile['expected_verified']-1,self.profile['expected_verified']+1):
            self.report['verification-results']['verified']=count
            self.assertFalse(self.accepted())
        self.report['verification-results']['verified']=self.profile['expected_verified']
        self.report['verification-results']['is-verifying-entire-crate']=False
        self.assertFalse(self.accepted())

    def test_missing_theorem_timeout_and_changed_tool_refuse(self):
        for field in ('version','commit','toolchain'):
            previous=self.report['verus'][field];self.report['verus'][field]='other'
            self.assertFalse(self.accepted());self.report['verus'][field]=previous
        self.proc.stderr='error: solver returned UNKNOWN'
        self.assertFalse(self.accepted());self.proc.stderr=''
        self.report['func-details'].pop(self.profile['target_functions'][0])
        self.assertFalse(self.accepted())

    def test_frozen_shared_and_probe_contracts_cannot_be_weakened(self):
        actual=copy.deepcopy(self.profile['functions'])
        with patch.object(gate,'inventory',return_value=actual):
            self.assertEqual(gate.check_inventory('',self.profile),actual)
            for name in ('probe_kernel::decide_reachable','lemma_fixed_point::verdict_fixed_point_form',
                         'finite_machine_proofs::park_induction','finite_machine_spec::reach'):
                with self.subTest(name=name):
                    record=actual[gate.NAMESPACE+name]
                    old=record['ensures_sha256'];record['ensures_sha256']='weakened'
                    with self.assertRaisesRegex(ValueError,'contracts changed'):
                        gate.check_inventory('',self.profile)
                    record['ensures_sha256']=old
            actual[gate.NAMESPACE+'extra']={}
            with self.assertRaisesRegex(ValueError,'contracts changed'):
                gate.check_inventory('',self.profile)

    def test_two_controls_have_unique_anchors_and_live_named_assertions(self):
        controls=gate.source_controls(gate.ROOT)
        self.assertEqual(set(controls),{'noncommitting_target','early_frontier'})
        tests=(gate.ROOT/gate.TESTS).read_text()
        for path,changed,function,test in controls.values():
            self.assertNotEqual(changed,(gate.ROOT/path).read_text())
            self.assertIn('fn '+function+'(',changed)
            self.assertIn('fn '+test+'()',tests)
            start,end=gate.lemma.function_region(changed,function)
            self.assertGreater(end,start)

    def test_all_transitive_application_sources_are_configuration_checked(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            for path in gate.UNIT:
                (root/path).parent.mkdir(parents=True,exist_ok=True)
                (root/path).write_bytes((gate.ROOT/path).read_bytes())
            gate.configuration(root,self.profile)
            for path in (gate.SUBJECT,gate.SEARCH,gate.SPEC,gate.PROOFS,gate.VERDICTS):
                original=(root/path).read_bytes()
                (root/path).write_bytes(original+b'\n#[cfg(not(verus_keep_ghost))] const HIDDEN: bool = true;\n')
                with self.subTest(path=path), self.assertRaises(gate.verifier.ConfigurationRefused):
                    gate.configuration(root,self.profile)
                (root/path).write_bytes(original)

    def test_gate_refuses_configuration_before_any_compilation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            for path in (*gate.UNIT,gate.PROFILE,gate.verifier.PIN):
                (root/path).parent.mkdir(parents=True,exist_ok=True)
                (root/path).write_bytes((gate.ROOT/path).read_bytes())
            with (root/gate.SUBJECT).open('a') as f:
                f.write('\n#[cfg(not(verus_keep_ghost))] const HIDDEN: bool = true;\n')
            with patch.object(gate,'ROOT',root),patch.object(gate,'snapshot',return_value={}), \
                    patch.object(gate.verifier,'prepare_tools') as prepare:
                with self.assertRaises(gate.verifier.ConfigurationRefused):gate.check(root,False,root)
                prepare.assert_not_called()


if __name__=='__main__':unittest.main()
