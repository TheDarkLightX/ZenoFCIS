"""Negative partition controls and the genuine pinned libtest selector."""
from collections import Counter
from pathlib import Path
import subprocess
import tempfile
import unittest

import miri_shards as shards


class Partitions(unittest.TestCase):
    def rows(self):
        base = {'group': 'suite', 'packages': '-p example', 'miri_skip': ['native_only']}
        return [dict(base, part=n, parts=2) for n in [1, 2]]

    def test_complete_parts_recover_the_original_selector(self):
        self.assertEqual(shards.logical_rows(self.rows()),
                         [{'group': 'suite', 'packages': '-p example', 'miri_skip': ['native_only']}])

    def test_missing_duplicate_extra_and_boolean_parts_are_refused(self):
        rows = self.rows()
        for bad in [rows[:1], rows + rows[:1], rows + [dict(rows[0], part=3)],
                    [dict(rows[0], part=True), rows[1]]]:
            with self.subTest(rows=bad), self.assertRaises(ValueError):
                shards.logical_rows(bad)

    def test_changes_to_package_filters_and_exclusions_are_refused(self):
        for key, value in [('packages', '-p other'), ('miri_skip', []), ('target', '--doc')]:
            rows = self.rows(); rows[1][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError): shards.logical_rows(rows)

    def test_exact_test_and_non_test_partitions_are_refused(self):
        for extra in [{'test': 'one'}, {'command': 'run'}]:
            with self.assertRaises(ValueError): shards.logical_rows([dict(r, **extra) for r in self.rows()])

    def test_duplicate_unpartitioned_group_is_refused(self):
        row = {'group': 'suite', 'packages': '-p example'}
        with self.assertRaises(ValueError): shards.logical_rows([row, row])

    def test_multiset_is_preserved_and_equal_names_stay_together(self):
        names = ['case_' + str(i) for i in range(100)] + ['same', 'same']
        parts = shards.partition(names, 8)
        self.assertEqual(Counter(n for p in parts for n in p), Counter(names))
        self.assertEqual(sum('same' in p for p in parts), 1)
        self.assertEqual(parts, shards.partition(names, 8))

    def test_argument_injection_and_invalid_part_counts_are_refused(self):
        for name in ['', '--skip', 'a\nb', 'a\0b']:
            with self.assertRaises(ValueError): shards.partition([name], 2)
        for count in [0, 1, 17, True]:
            with self.assertRaises(ValueError): shards.partition(['one'], count)

    def test_existing_exact_test_siblings_and_native_exclusions_are_retained(self):
        row = {'group': 'remainder', 'packages': '-p zeno-fcis-synthesis',
               'target': '--lib', 'miri_skip': ['native_only']}
        rows = [row, dict(row, group='exact', test='separate', miri_skip=[])]
        self.assertEqual(shards.base_arguments(row, rows),
                         ['--', '--exact', '--skip', 'native_only', '--skip', 'separate'])

    def test_real_libtest_accepts_multiple_exact_filters(self):
        with tempfile.TemporaryDirectory(prefix='zf-miri-part-') as temp:
            root = Path(temp); source = root / 'tests.rs'; binary = root / 'tests'
            source.write_text('#[test] fn alpha() {}\n#[test] fn alpha_more() {}\n'
                              '#[test] fn beta() {}\n#[test] fn gamma() {}\n')
            subprocess.run(['rustc', '+nightly-2026-07-21', '--test', str(source), '-o', str(binary)], check=True)
            listing = subprocess.check_output([str(binary), '--exact', 'alpha', 'gamma',
                                               '--list', '--format', 'terse'], text=True)
            self.assertEqual(listing.splitlines(), ['alpha: test', 'gamma: test'])
            result = subprocess.check_output([str(binary), '--exact', 'alpha', 'gamma'], text=True)
            self.assertIn('test result: ok. 2 passed;', result)
            self.assertNotIn('test beta ...', result)
            self.assertNotIn('test alpha_more ...', result)

    def test_real_libtest_preserves_existing_ignored_cases(self):
        with tempfile.TemporaryDirectory(prefix='zf-miri-part-') as temp:
            root = Path(temp); source = root / 'tests.rs'; binary = root / 'tests'
            source.write_text('#[test] fn active() {}\n'
                              '#[test] #[ignore] fn ignored() { panic!("must remain ignored"); }\n')
            subprocess.run(['rustc', '+nightly-2026-07-21', '--test', str(source), '-o', str(binary)], check=True)
            names, ignored = shards.inventory([str(binary)], ['--exact', 'active', 'ignored'])
            self.assertEqual(names, ['active', 'ignored'])
            self.assertEqual(ignored, ['ignored'])
            result = subprocess.check_output([str(binary), '--exact', 'active', 'ignored'], text=True)
            self.assertIn('test result: ok. 1 passed; 0 failed; 1 ignored;', result)

    def test_real_libtest_complement_preserves_exact_names_and_earlier_skips(self):
        with tempfile.TemporaryDirectory(prefix='zf-miri-part-') as temp:
            root = Path(temp); source = root / 'tests.rs'; binary = root / 'tests'
            source.write_text('#[test] fn alpha() {}\n#[test] fn alpha_more() {}\n'
                              '#[test] fn beta() { panic!("must remain excluded"); }\n#[test] fn gamma() {}\n')
            subprocess.run(['rustc', '+nightly-2026-07-21', '--test', str(source), '-o', str(binary)], check=True)
            arguments = shards.part_arguments(['--exact', '--skip', 'beta'],
                                              ['alpha', 'alpha_more', 'gamma'], ['alpha', 'gamma'])
            self.assertEqual(arguments[:3], ['--exact', '--skip', 'beta'])
            names, ignored = shards.inventory([str(binary)], arguments)
            self.assertEqual(names, ['alpha', 'gamma'])
            self.assertEqual(ignored, [])
            output = subprocess.check_output([str(binary), *arguments], text=True)
            self.assertIn('test result: ok. 2 passed; 0 failed;', output)


if __name__ == '__main__':
    unittest.main()
