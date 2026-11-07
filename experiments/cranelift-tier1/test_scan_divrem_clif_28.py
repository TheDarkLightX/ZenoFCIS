import tempfile
import unittest
from pathlib import Path
from scan_divrem_clif_28 import inspect, scan, UPSTREAM

class ScanTests(unittest.TestCase):
    def test_pair(self):
        out = inspect('''function %work(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
    v2 = udiv v0, v1
    v3 = urem v0, v1
}
''')
        self.assertEqual(out['candidate_pair_count'], 1)
        self.assertEqual(out['candidate_pairs'][0]['operands'], ['v0', 'v1'])
    def test_real_pinned_emit_clif_shape(self):
        # Exact syntax from CI run 42385101549, not a native-cost conclusion.
        data = """function u0:0(i64 vmctx, i64, i64, i64) -> i64 tail {
                                block0(v0: i64, v1: i64, v2: i64, v3: i64):
@0028                               v4 = udiv v2, v3
@002d                               v5 = urem v2, v3
@0032                               jump block1
                                block1:
@002e                               v6 = iconst.i64 17
}"""
        got = inspect(data)
        self.assertEqual(got['candidate_pair_count'], 1)
        self.assertEqual(got['candidate_pairs'][0]['operands'], ['v2', 'v3'])
        self.assertEqual(got['candidate_pairs'][0]['first']['line'], 3)
    def test_reversed_order(self):
        self.assertEqual(inspect('block0(v0: i64, v1: i64):\n v2 = urem v0, v1\n v3 = udiv v0, v1')['candidate_pair_count'],1)
    def test_different_operands_and_block(self):
        cases = ['block0(v0: i64, v1: i64):\n v2 = udiv v0, v1\n v3 = urem v1, v0',
                 'block0(v0: i64, v1: i64):\n v2 = udiv v0, v1\nblock1:\n v3 = urem v0, v1',
                 '; v2 = udiv v0, v1\nblock0:\n v3 = urem v0, v1',
                 'block0:\n v2 = sdiv v0, v1\n v3 = urem v0, v1',
                 'block0:\n v2 = udiv v0, v1\n}\nblock1:\n v3 = urem v0, v1']
        for case in cases:
            self.assertEqual(inspect(case)['candidate_pair_count'], 0,case)
    def test_source_pin_and_no_overwrite(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            (root/'clif').mkdir()
            (root/'clif'/'test.clif').write_text('block0:\n v2 = udiv v0, v1\n v3 = urem v0, v1\n')
            (root/'source.wat').write_text('(module)')
            (root/'module.cwasm').write_bytes(b'fake for parser test')
            outfile=root/'observation.json'
            with self.assertRaises(ValueError):
                scan(root/'clif',root/'source.wat',root/'module.cwasm',outfile,'bad')
            result=scan(root/'clif',root/'source.wat',root/'module.cwasm',outfile,UPSTREAM)
            self.assertEqual(result['total_candidate_pairs'],1)
            with self.assertRaises(ValueError):
                scan(root/'clif',root/'source.wat',root/'module.cwasm',outfile,UPSTREAM)
    def test_no_pair_is_valid_observation(self):
        self.assertEqual(inspect('block0:\n v4 = iadd v0, v1')['candidate_pair_count'],0)

if __name__=='__main__': unittest.main()
