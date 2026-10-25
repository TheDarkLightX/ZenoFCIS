"""Lightweight scheduling and refusal checks; no proof/native tools invoked."""
import json
import os
from pathlib import Path
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

from qualification_controls import control_jobs, run_controls
import check_canonical_bytes as gate
import check_verus as verifier
import subprocess


class ControlPool(unittest.TestCase):
    def test_bounds(self):
        self.assertEqual(control_jobs({}), 1)
        for value in ('0', '9', '-1', '1.0', ' 2', '02', '', 'two'):
            with self.subTest(value=value), self.assertRaises(ValueError):
                control_jobs({'ZENO_FCIS_CONTROL_JOBS': value})

    def test_bounded_overlap_exactly_once_and_identical_ordered_receipts(self):
        items = [(str(i), i) for i in range(12)]
        def run(jobs):
            lock = threading.Lock()
            seen, active, peak = [], 0, 0
            def worker(item):
                nonlocal active, peak
                with lock:
                    seen.append(item[0]); active += 1; peak = max(peak, active)
                time.sleep((12 - item[1]) * 0.001)
                with lock:
                    active -= 1
                return {'name': item[0], 'killed': True, 'exit_code': 1}
            with patch.dict(os.environ, {'ZENO_FCIS_CONTROL_JOBS': str(jobs)}):
                result = run_controls(worker, items)
            self.assertCountEqual(seen, [x[0] for x in items])
            self.assertLessEqual(peak, jobs)
            if jobs > 1:
                self.assertGreater(peak, 1)
            return json.dumps(result, separators=(',', ':'))
        self.assertEqual(run(1), run(3))

    def test_all_controls_finish_after_failure_and_first_declared_error_wins(self):
        for jobs in (1, 3):
            seen = []
            def worker(item):
                seen.append(item[0])
                if item[1] in (0, 2):
                    raise ValueError(item[0])
                return {'name': item[0], 'killed': True}
            with tempfile.TemporaryDirectory() as tmp, patch.dict(os.environ, {'ZENO_FCIS_CONTROL_JOBS': str(jobs)}):
                log = Path(tmp) / 'time.json'
                with self.assertRaisesRegex(ValueError, '^first$'):
                    run_controls(worker, [('first', 0), ('good', 1), ('later', 2)], timings=log)
                recorded = json.loads(log.read_text())
                self.assertEqual([r['name'] for r in recorded['controls']], ['first', 'good', 'later'])
                self.assertEqual([r['completed'] for r in recorded['controls']], [False, True, False])
                self.assertTrue(all(r['elapsed_ns'] >= 0 for r in recorded['controls']))
                partial = [json.loads(p.read_text()) for p in sorted((Path(tmp) / 'time-records').glob('*.json'))]
                self.assertEqual([r['name'] for r in partial], ['first', 'good', 'later'])
                self.assertEqual(partial[0]['error'], 'first')
            self.assertCountEqual(seen, ['first', 'good', 'later'])

    def test_known_bad_proof_failure_remains_refused_by_actual_classifier(self):
        pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        # An apparent control kill accompanied by resource exhaustion must never
        # qualify. Run the real classifier inside both execution modes.
        specimen = ('wrong_radix', ('changed',))
        def worker(item):
            process = subprocess.CompletedProcess([], 1, '{}', 'error: postcondition not satisfied\nresource limit')
            killed, category, _ = gate.classify_mutation(
                process, {}, item[0], '', Path('/tmp/cx-q25'), pin, {})
            if not killed:
                raise ValueError(category)
            return {'name': item[0], 'killed': killed}
        for jobs in (1, 3):
            with patch.dict(os.environ, {'ZENO_FCIS_CONTROL_JOBS': str(jobs)}), self.assertRaisesRegex(ValueError, 'inconclusive'):
                run_controls(worker, [specimen])



class GateReceipts(unittest.TestCase):
    def test_non_linux_profiles_refuse_before_tool_setup_or_evidence_writes(self):
        for checker in (gate, verifier):
            for system, machine in (('Darwin', 'arm64'), ('Linux', 'aarch64'), ('Windows', 'AMD64')):
                with self.subTest(checker=checker.__name__, system=system, machine=machine), \
                        tempfile.TemporaryDirectory() as temporary, \
                        patch.object(checker.platform, 'system', return_value=system), \
                        patch.object(checker.platform, 'machine', return_value=machine), \
                        patch.object(verifier, 'prepare_tools', side_effect=AssertionError('tool setup ran')):
                    cache = Path(temporary) / 'cache'
                    with self.assertRaisesRegex(RuntimeError, 'requires .*Linux x86-64'):
                        checker.check(cache, False)
                    self.assertEqual(list(Path(temporary).iterdir()), [])

    def test_real_gate_serial_and_parallel_acceptance_receipts_are_identical(self):
        import copy
        pin = json.loads((verifier.ROOT / verifier.PIN).read_text())
        positive = {'verus': {k: pin[k] for k in ('version', 'commit')},
                    'func-details': dict.fromkeys(pin['target_functions'], {}),
                    'verification-results': {'success': True, 'verified': pin['expected_verified'],
                        'errors': 0, 'encountered-error': False, 'encountered-vir-error': False,
                        'is-verifying-entire-crate': True}}
        positive['verus']['toolchain'] = pin['rust_toolchain']
        controls = verifier.mutation_sources((verifier.ROOT / verifier.SUBJECT).read_text())
        def run(jobs, survivor=False):
            seen = []
            def fake(command, cwd, environment, **kwargs):
                if '--output-json' not in command:
                    return subprocess.CompletedProcess(command, 0, 'mock native/version', '')
                report = copy.deepcopy(positive)
                name = Path(cwd).name
                stderr = ''
                if cwd != verifier.ROOT:
                    seen.append(name)
                    expected = controls[name][1]
                    if not (survivor and name == 'wrong_interval_width'):
                        report['verification-results'].update(success=expected != 'proof',
                            errors=int(expected == 'proof'), verified=pin['expected_verified'] - 1)
                        report['verification-results']['encountered-error'] = expected == 'proof'
                        if expected == 'coverage':
                            report['func-details'].pop('domain_bounds::finite_bounds::interval_width')
                        if expected == 'proof':
                            line, _ = verifier.function_lines(controls[name][0], verifier.SEMANTIC_TARGETS[name])
                            stderr = (f'error: postcondition not satisfied\n --> {verifier.SUBJECT}:{line}:1\n'
                                      'error: aborting due to 1 previous error\n')
                return subprocess.CompletedProcess(command,
                    int(not report['verification-results']['success']), json.dumps(report), stderr)
            # Only the synthetic protocol pipeline runs; do not claim a proof
            # profile for the machine executing this unit test.
            with patch.dict(os.environ, {'ZENO_FCIS_CONTROL_JOBS': str(jobs)}), \
                 patch.object(verifier.platform, 'system', return_value='Linux'), \
                 patch.object(verifier.platform, 'machine', return_value='x86_64'), \
                 patch.object(verifier, 'prepare_tools', return_value=Path('/tmp/cx-q25/pinned-tools')), \
                 patch.object(verifier, 'verify_tool_files'), patch.object(verifier, 'snapshot', return_value={}), \
                 patch.object(verifier, 'run', side_effect=fake), \
                 patch.object(verifier.subprocess, 'check_output', side_effect=lambda args, **k: 'a'*40 if 'rev-parse' in args else ''):
                if survivor:
                    with self.assertRaisesRegex(RuntimeError, 'survived'):
                        verifier.check(Path('/tmp/cx-q25/cache'), False)
                    self.assertCountEqual(seen, list(controls))
                    return
                receipt = verifier.check(Path('/tmp/cx-q25/cache'), False)
            self.assertCountEqual(seen, list(controls))
            return json.dumps(receipt, sort_keys=True)
        self.assertEqual(run(1), run(3))
        run(1, survivor=True)
        run(3, survivor=True)

    def test_subcommand_telemetry_retains_timeouts_separately_from_receipts(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(verifier.subprocess, 'run',
                side_effect=subprocess.TimeoutExpired(['fake'], 7)):
            with self.assertRaises(subprocess.TimeoutExpired):
                verifier.run(['fake'], Path(tmp), {'ZENO_FCIS_TIMING_DIR': tmp}, timeout=7)
            rows = [json.loads(p.read_text()) for p in Path(tmp).glob('*.json')]
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]['failure'], 'TimeoutExpired')
        self.assertIsNone(rows[0]['returncode'])
        self.assertGreaterEqual(rows[0]['elapsed_ns'], 0)


if __name__ == '__main__':
    unittest.main()
