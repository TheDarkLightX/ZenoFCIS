#!/usr/bin/env python3
"""Manual full refinement controls; a trusted native CLI and task package are required."""
from __future__ import annotations
import argparse
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import check_runtime_refinement as check
import make_runtime_refinement as producer
from test_runtime_refinement import Admission, write_candidate, repin

CLI = None
PACKAGE = None

class Native(unittest.TestCase):
    def test_positive_model_certificate_and_claim_boundary(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);write_candidate(root/'candidate')
            receipt=check.check(CLI,root/'candidate',PACKAGE,root/'evidence')
            self.assertEqual(receipt['claim_level'],'MODEL_REFINEMENT')
            self.assertFalse(receipt['executable_assembly_refinement'])
            self.assertFalse(receipt['publication_authority'])
            self.assertTrue(receipt['open_obligations'])
            self.assertEqual(receipt['scope']['compared_pairs'],1152)

    def test_budget_and_fake_inconclusive_status_leave_no_receipt(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);write_candidate(root/'candidate')
            with self.assertRaises(check.Refused):check.check(CLI,root/'candidate',PACKAGE,root/'budget',max_checks=1)
            self.assertFalse((root/'budget/receipt.json').exists())
            with patch.object(check,'run',return_value={'verdict':'inconclusive','status':'inductive'}):
                with self.assertRaises(check.Refused):check.check(CLI,root/'candidate',PACKAGE,root/'unknown')
            self.assertFalse((root/'unknown/receipt.json').exists())

    def test_frozen_input_and_checker_drift_refuse_after_native_checks(self):
        for fault in ('frozen_input','checker','reference_set'):
            with self.subTest(fault=fault),tempfile.TemporaryDirectory() as d:
                root=Path(d);write_candidate(root/'candidate')
                original_run,original_read,original_inventory=check.run,check.read,check.inventory
                own_reads=0
                reference_reads=0
                def run(*args,**kwargs):
                    result=original_run(*args,**kwargs)
                    if fault=='frozen_input' and args[2].name=='refinement':
                        (root/'evidence/input/concrete/project.zeno').write_text('changed after verification')
                    return result
                def read(path,*args,**kwargs):
                    nonlocal own_reads
                    data=original_read(path,*args,**kwargs)
                    if Path(path)==Path(check.__file__):
                        own_reads+=1
                        if fault=='checker' and own_reads>1:return data+b'changed'
                    return data
                def inventory(path,*args,**kwargs):
                    nonlocal reference_reads
                    result=original_inventory(path,*args,**kwargs)
                    if path==check.REFERENCE:
                        reference_reads+=1
                        if fault=='reference_set' and reference_reads>1:return result|{'unexpected'}
                    return result
                with patch.object(check,'run',side_effect=run),patch.object(check,'read',side_effect=read),patch.object(check,'inventory',side_effect=inventory):
                    with self.assertRaises(check.Refused):check.check(CLI,root/'candidate',PACKAGE,root/'evidence')
                self.assertFalse((root/'evidence/receipt.json').exists())

    def test_rehashed_semantic_mutations_are_detected_by_native_checker(self):
        faults=('premature_apply','wrong_ack_slot','lost_publication','payload','destination','duplicate_apply','removed_guard','changed_mapping','missing_mapping','extra_mapping')
        for fault in faults:
            with self.subTest(fault=fault),tempfile.TemporaryDirectory() as d:
                files=producer.candidate_files();policy=json.loads(files['concrete/v2/policy.json'])
                if fault=='premature_apply':
                    policy['cases'].insert(0,{'when':'state == 0 && action == 4','class':'Accept','reason':None,'post':{'110':'8'},'outbox':[{'ordinal':0,'channel':300,'destination':'local-test','payload':{'140':'1'},'idempotency_ordinal':0}]})
                elif fault=='wrong_ack_slot':
                    policy['cases'].insert(0,{'when':'state == 73 && action == 5','class':'Accept','reason':None,'post':{'110':'11'},'outbox':[]})
                elif fault=='lost_publication':
                    row=next(r for r in policy['cases'] if r['when'].startswith('action == 0') and r['class']=='Accept');row['post']={'110':'state'}
                elif fault in ('payload','destination','duplicate_apply'):
                    row=next(r for r in policy['cases'] if r['outbox'])
                    if fault=='payload':row['outbox'][0]['payload']['140']='2' if row['outbox'][0]['payload']['140']=='1' else '1'
                    elif fault=='destination':row['outbox'][0]['destination']='elsewhere'
                    else:
                        extra=copy.deepcopy(row['outbox'][0]);extra['ordinal']=1;extra['idempotency_ordinal']=1;row['outbox'].append(extra)
                elif fault=='removed_guard':
                    lines=files['concrete/project.zeno'].decode().splitlines();lines=[('law 500 valid_pre on any = true;' if x.startswith('law 500 ') else x) for x in lines];files['concrete/project.zeno']=('\n'.join(lines)+'\n').encode()
                else:
                    mapping=json.loads(files['abstraction.json'])
                    if fault=='changed_mapping':mapping['abstraction'][1]=mapping['abstraction'][2]
                    elif fault=='missing_mapping':mapping['abstraction'].pop()
                    else:mapping['abstraction'].append(0)
                    files['abstraction.json']=producer.dump(mapping).encode()
                files['concrete/v2/policy.json']=producer.dump(policy).encode();repin(files)
                path=Path(d)/'candidate';write_candidate(path,files)
                check.candidate(path)  # Byte bindings are intentionally valid; not the rejecting layer.
                run=subprocess.run([str(CLI),'contract','refine-check',str(check.REFERENCE),str(path/'concrete'),
                    '--abstraction',str(path/'abstraction.json'),'--max-checks','1152','--format','json','--no-kb'],capture_output=True,timeout=120)
                self.assertNotEqual(run.returncode,0)
                packet=json.loads(run.stdout)
                self.assertNotEqual(packet.get('verdict'),'refines')
                if fault in ('missing_mapping','extra_mapping'):
                    self.assertIn(packet.get('verdict',packet.get('status')),('counterexample','malformed'))
                else:
                    self.assertEqual(packet.get('verdict'),'counterexample',packet)
                    self.assertIsNotNone(packet.get('witness'))


def main():
    global CLI, PACKAGE
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli',type=Path,required=True)
    parser.add_argument('--package',type=Path,required=True)
    args=parser.parse_args()
    CLI,PACKAGE=args.cli.absolute(),args.package.absolute()
    suite=unittest.defaultTestLoader.loadTestsFromTestCase(Admission)
    suite.addTests(unittest.defaultTestLoader.loadTestsFromTestCase(Native))
    return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1

if __name__=='__main__':raise SystemExit(main())
