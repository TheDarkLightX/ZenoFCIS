"""Command-level exit regressions, using finite models rather than stub verdicts."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent


class VerdictExits(unittest.TestCase):
    def call(self, *args):
        result = subprocess.run([sys.executable, '-B', str(HERE / 'run.py'), *map(str, args)],
                                capture_output=True, text=True, timeout=30)
        return result, json.loads(result.stdout)

    def test_check_verdicts_in_both_machine_profiles(self):
        with tempfile.TemporaryDirectory() as folder:
            for name in ('order.zal', 'order2.zal'):
                source = (HERE / 'examples' / name).read_text()
                for label, extra, expected, status in (
                    ('positive', '', 0, 'pass-with-scope'),
                    ('negative', 'invariant never_cancelled: !(state == cancelled);\n', 1, 'counterexample'),
                    ('unknown', 'unresolved fairness: pending requests eventually resolve;\n', 2, 'unsupported'),
                ):
                    with self.subTest(profile=name, case=label):
                        path = Path(folder) / name
                        path.write_text(source + extra)
                        completed, report = self.call('check', path)
                        self.assertEqual(completed.returncode, expected, completed.stderr + completed.stdout)
                        self.assertEqual(report['status'], status)

    def test_diff_verdict_exits(self):
        with tempfile.TemporaryDirectory() as folder:
            for name in ('order.zal', 'order2.zal'):
                source = (HERE / 'examples' / name).read_text()
                original = HERE / 'examples' / name
                for label, candidate, expected, comparison in (
                    ('same', source, 0, 'same-functional-observations-on-declared-domain'),
                    ('different', source.replace('[authorized]', '[false]'), 1, 'different'),
                    ('domain', source.replace('context authorized;', 'context authorized, other;'), 2, 'domain-changed'),
                    ('overlap', source + 'rule overlap: pending + approve [true] -> accept cancelled;\n', 2, 'invalid-overlap'),
                ):
                    with self.subTest(profile=name, case=label):
                        path = Path(folder) / name
                        path.write_text(candidate)
                        completed, report = self.call('diff', original, path)
                        self.assertEqual(completed.returncode, expected, completed.stderr + completed.stdout)
                        self.assertEqual(report['comparison'], comparison)

    def test_why_not_distinguishes_found_and_none(self):
        source = HERE / 'examples/order.zal'
        given = json.dumps({'state': 'pending', 'event': 'approve', 'context': {'authorized': False}})
        for state, expected, status in (('approved', 0, 'found'), ('cancelled', 1, 'none-in-scope')):
            completed, report = self.call('why-not', source, '--input', given, '--want', json.dumps({'state': state}))
            self.assertEqual(completed.returncode, expected, completed.stderr + completed.stdout)
            self.assertEqual(report['status'], status)

    def test_typed_check_and_qualification_mapping(self):
        completed, report = self.call('check', HERE / 'examples/relay_typed.zal')
        self.assertEqual(completed.returncode, 0, completed.stderr + completed.stdout)
        completed, report = self.call('qualify', HERE / 'examples/order.zal', '--cli', '/bin/false')
        self.assertEqual(completed.returncode, 2, completed.stderr + completed.stdout)
        self.assertEqual(report['status'], 'unfinished')
        from run import verdict_exit
        # Qualification may stop at an external build; it has no positive verdict then.
        self.assertEqual(verdict_exit({'status': 'unfinished'}), 2)
        self.assertEqual(verdict_exit({'status': 'fail'}), 1)
        self.assertEqual(verdict_exit({'status': 'unsupported'}), 2)
        self.assertEqual(verdict_exit({'status': 'unknown'}), 2)
        self.assertEqual(verdict_exit({'status': 'UNKNOWN'}), 2)

    def test_a_header_in_the_other_syntax_names_the_flag(self):
        with tempfile.TemporaryDirectory() as folder:
            for name in ('order.zal', 'order2.zal', 'relay_typed.zal'):
                symbolic = HERE / 'examples' / name
                english = Path(folder) / name
                rendered = subprocess.run([sys.executable, '-B', str(HERE / 'run.py'), 'render', str(symbolic),
                                           '--to', 'english'], capture_output=True, text=True, timeout=30, check=True)
                english.write_text('# a leading comment\n' + rendered.stdout)
                for path, extra, flag in ((english, (), '--syntax english'), (symbolic, ('--syntax', 'english'), 'omit')):
                    with self.subTest(profile=name, flag=flag):
                        completed = subprocess.run([sys.executable, '-B', str(HERE / 'run.py'), 'check', str(path), *extra],
                                                   capture_output=True, text=True, timeout=30)
                        self.assertEqual(completed.returncode, 2, completed.stderr + completed.stdout)
                        refusal = json.loads(completed.stderr)
                        self.assertIn(flag, refusal['diagnostic'])
                        diagnostic = refusal['source_diagnostic']
                        self.assertEqual(diagnostic['code'], 'syntax-mismatch', diagnostic)
                        header = next(number for number, line in enumerate(path.read_text().splitlines(), 1)
                                      if line.strip() and not line.lstrip().startswith('#'))
                        self.assertEqual(diagnostic['location'], {'file': str(path), 'line': header, 'column': 1})
                completed, report = self.call('check', english, '--syntax', 'english')
                self.assertEqual(completed.returncode, 0, completed.stderr + completed.stdout)
                self.assertEqual(report['status'], 'pass-with-scope')


if __name__ == '__main__':
    unittest.main()
