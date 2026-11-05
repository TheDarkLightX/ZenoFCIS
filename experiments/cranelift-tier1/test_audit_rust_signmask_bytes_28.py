"""Archived source receipt and semantic-cost audit tampering/refusal checks."""
import argparse
import json
import tempfile
import unittest
import zipfile
from pathlib import Path
from audit_rust_signmask_bytes_28 import parse, replay

p=argparse.ArgumentParser(add_help=False)
p.add_argument('--zip', type=Path)
a,_=p.parse_known_args()

class AuditTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if a.zip is None:
            raise unittest.SkipTest('exact source artifact must be supplied')
        cls.src = a.zip
        with zipfile.ZipFile(a.zip) as archive:
            cls.files = {n:archive.read(n) for n in archive.namelist()}

    def mutate(self,name,fn):
        with tempfile.TemporaryDirectory() as folder:
            dest=Path(folder)/'altered.zip'
            with zipfile.ZipFile(dest,'w',zipfile.ZIP_DEFLATED) as archive:
                for k,v in self.files.items():
                    archive.writestr(k,fn(v) if k==name else v)
            return replay(dest)

    def test_positive(self):
        rows=replay(self.src)['profiles']
        self.assertEqual([r['sum_extra_prefix_bytes_for_three_saturation_exports'] for r in rows],[18,17])
        self.assertTrue(all(r['zero_complete_aot_savings'] for r in rows))

    def test_substituted_binary_refused(self):
        with self.assertRaises(ValueError):
            self.mutate('cases/optimized/add/idiom.cwasm',lambda _ :b'fake')

    def test_false_cost_delta_refused(self):
        def forge(b):
            v=json.loads(b)
            v['profiles'][1]['comparisons']['add']['baseline_minus_candidate_whole_cwasm_bytes']=8192
            return json.dumps(v).encode()
        with self.assertRaises(ValueError):
            self.mutate('cases/qualification.json',forge)

    def test_duplicate_selected_export_refused(self):
        src=self.files['cases/optimized/add/disassembly.txt']
        with self.assertRaises(ValueError):
            parse(src+b'\nwasm[0]::function[9876]::sat_add:\n            c3                           retq\n')

    def test_truncated_disassembly_refused(self):
        with self.assertRaises(ValueError):
            self.mutate('cases/optimized/add/disassembly.txt',lambda b:b[:100])

if __name__ == '__main__':
    unittest.main(argv=['test_audit_rust_signmask_bytes_28.py'],verbosity=2)
