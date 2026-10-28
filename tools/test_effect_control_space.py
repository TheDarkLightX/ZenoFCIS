"""Control launchers check their work and target mounts, not unrelated root."""
import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

import cargo_source_scope
import check_core_components as core_checks
import check_effect_policy_controls as policy_controls
import check_shell_effects_controls as shell_controls
import test_composed_domain_runtime_controls as composed_controls


class BuildSpaceTests(unittest.TestCase):
    def test_uncreated_target_checks_existing_mount_ancestor(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            with mock.patch.object(cargo_source_scope.shutil, 'disk_usage',
                                   return_value=SimpleNamespace(free=9 * 1024**3)) as usage:
                cargo_source_scope.require_build_space(root, root / 'new' / 'target')
            self.assertEqual(usage.call_args_list, [mock.call(root), mock.call(root)])

    def exercise_launcher(self, module, low_mount=None):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            original, work, target = root / 'source', root / 'work', root / 'target'
            for path in (original, work, target):
                path.mkdir()
            (original / 'control.txt').write_text('original')
            out = root / 'controls.json'
            edits = [('control.txt', 'original', 'mutant')]
            controls = ([('probe', 'named', 'named assertion', edits)]
                        if module is policy_controls else
                        [('probe', ['-p', 'zeno-fcis-shell', '--lib'], 'named',
                          ['named assertion'], 'test-only fixture', edits)])
            argv = ['controls', '--out', str(out)]
            if module is shell_controls:
                argv += ['--cli', str(root / 'unused-cli'), '--only', 'probe']
            checked = []

            def disk_usage(path):
                path = Path(path).resolve()
                self.assertNotEqual(path, Path('/'), 'queried an unrelated root mount')
                checked.append(path)
                mount = 'target' if path == target else 'work'
                return SimpleNamespace(free=0 if mount == low_mount else 9 * 1024**3)

            def command(args, *, cwd, **kwargs):
                if '--no-run' in args:
                    return subprocess.CompletedProcess(args, 0, 'built')
                mutant = (Path(cwd) / 'control.txt').read_text() == 'mutant'
                output = ('assertion failed: named assertion\ntest result: FAILED. 0 passed; 1 failed;'
                          if mutant else 'test result: ok. 1 passed; 0 failed;')
                return subprocess.CompletedProcess(args, 101 if mutant else 0, output)

            with (mock.patch.object(module, 'ROOT', original),
                  mock.patch.object(module, 'CONTROLS', controls),
                  mock.patch.object(module, 'source_environment', side_effect=lambda root, env: env),
                  mock.patch.object(module.subprocess, 'run', side_effect=command) as run,
                  mock.patch.object(cargo_source_scope.shutil, 'disk_usage', side_effect=disk_usage),
                  mock.patch.object(tempfile, 'tempdir', str(work)),
                  mock.patch.dict(os.environ, {'CARGO_TARGET_DIR': str(target), 'TMPDIR': str(work)}),
                  mock.patch('sys.argv', argv), contextlib.redirect_stdout(io.StringIO())):
                if low_mount:
                    with self.assertRaisesRegex(RuntimeError, low_mount + ' free space'):
                        module.main()
                    run.assert_not_called()
                else:
                    module.main()
                    rows = json.loads(out.read_text())
                    self.assertEqual(rows[0]['normal_exit'], 0)
                    self.assertEqual(rows[0]['build_exit'], 0)
                    self.assertEqual(rows[0]['check_exit'], 101)
                    self.assertEqual(rows[0]['restored_exit'], 0)
                    self.assertIn(target, checked)
                    self.assertTrue(any(path.is_relative_to(work) for path in checked))
            self.assertEqual((original / 'control.txt').read_text(), 'original')

    def test_both_launchers_ignore_unrelated_root_mount(self):
        for module in (policy_controls, shell_controls):
            with self.subTest(module=module.__name__):
                self.exercise_launcher(module)

    def test_both_launchers_refuse_low_target_mount(self):
        for module in (policy_controls, shell_controls):
            with self.subTest(module=module.__name__):
                self.exercise_launcher(module, 'target')

    def test_both_launchers_refuse_low_work_mount(self):
        for module in (policy_controls, shell_controls):
            with self.subTest(module=module.__name__):
                self.exercise_launcher(module, 'work')


class CoreBuildSpaceTests(unittest.TestCase):
    def exercise(self, low_mount=None):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            work, target = root / 'work', root / 'target'
            work.mkdir()
            target.mkdir()

            def disk_usage(path):
                path = Path(path).resolve()
                low = (path == Path('/') and low_mount is None) or path == {'work': work, 'target': target}.get(low_mount)
                return SimpleNamespace(free=0 if low else 9 * 1024**3)

            with (mock.patch.object(cargo_source_scope.shutil, 'disk_usage', side_effect=disk_usage),
                  mock.patch.object(core_checks.subprocess, 'run',
                                    return_value=subprocess.CompletedProcess(['cargo'], 0)) as child):
                invoke = lambda: core_checks.run(['cargo', '+1.97.1', 'test', '--locked', '--offline'],
                                                work, root / 'build.log', {'CARGO_TARGET_DIR': str(target)})
                if low_mount:
                    with self.assertRaisesRegex(RuntimeError, low_mount + ' free space'):
                        invoke()
                    child.assert_not_called()
                else:
                    invoke()
                    child.assert_called_once()

    def test_core_checks_ignore_unrelated_root_mount(self):
        self.exercise()

    def test_core_checks_refuse_low_work_or_target_mount(self):
        for mount in ('work', 'target'):
            with self.subTest(mount=mount):
                self.exercise(mount)


class ComposedBuildSpaceTests(unittest.TestCase):
    def exercise(self, low_mount=None):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            work, target = root / 'work', root / 'target'
            work.mkdir()
            target.mkdir()
            checked = []

            def disk_usage(path):
                path = Path(path).resolve()
                self.assertNotEqual(path, Path('/'), 'queried unrelated root mount')
                checked.append(path)
                low = path == {'work': work, 'target': target}.get(low_mount)
                # Seven GiB also catches accidentally raising this launcher's
                # existing six GiB threshold to the helper's eight GiB default.
                return SimpleNamespace(free=0 if low else 7 * 1024**3)

            with (mock.patch.object(cargo_source_scope.shutil, 'disk_usage', side_effect=disk_usage),
                  mock.patch.object(composed_controls.subprocess, 'run',
                                    return_value=subprocess.CompletedProcess(['cargo'], 0, '', '')) as child):
                invoke = lambda: composed_controls.run_native(
                    ['cargo', '+1.97.1', 'test', '--locked', '--offline'],
                    work, root, 'build', {'CARGO_TARGET_DIR': str(target / 'new')})
                if low_mount:
                    with self.assertRaisesRegex(RuntimeError, low_mount + ' free space'):
                        invoke()
                    child.assert_not_called()
                else:
                    invoke()
                    child.assert_called_once()
                    self.assertEqual(checked, [work, target])
                    self.assertEqual(json.loads((root / 'build.disk.json').read_text())[
                        'minimum_free_bytes'], 6 * 1024**3)

    def test_composed_controls_use_actual_mounts_and_six_gib_threshold(self):
        self.exercise()

    def test_composed_controls_refuse_low_work_or_target_before_cargo(self):
        for mount in ('work', 'target'):
            with self.subTest(mount=mount):
                self.exercise(mount)


class PreVetoDiagnosticTests(unittest.TestCase):
    def test_only_the_named_semantic_assertion_counts(self):
        target = composed_controls.PRE_VETO_TEST
        out = f'running 1 test\ntest {target} ... FAILED\ntest result: FAILED. 0 passed; 1 failed;'
        err = (f"thread '{target}' panicked at {composed_controls.OBLIGATION_TESTS}:693:5:\n"
               'assertion `left == right` failed\n  left: Proved\n right: Violated\n')
        good = subprocess.CompletedProcess([], 101, out, err)
        self.assertTrue(composed_controls.pre_veto_killed(good, 693))
        bad = [
            subprocess.CompletedProcess([], 0, out, err),
            subprocess.CompletedProcess([], 101, 'running 0 tests', err),
            subprocess.CompletedProcess([], 101, out.replace('running 1 test', 'running 2 tests'), err),
            subprocess.CompletedProcess([], 101, out, 'error: could not compile'),
            subprocess.CompletedProcess([], 101, out, err.replace(':693:', ':694:')),
            subprocess.CompletedProcess([], 101, out, err.replace('left: Proved', 'left: Refused')),
            subprocess.CompletedProcess([], 101, out, err + 'another thread panicked at elsewhere'),
        ]
        for process in bad:
            with self.subTest(process=process):
                self.assertFalse(composed_controls.pre_veto_killed(process, 693))

    def test_baseline_and_restored_require_exactly_one_named_test(self):
        target = composed_controls.PRE_VETO_TEST
        out = f'running 1 test\ntest {target} ... ok\ntest result: ok. 1 passed; 0 failed;'
        self.assertTrue(composed_controls.one_test_passed(subprocess.CompletedProcess([], 0, out), target))
        for code, text in ((101, out), (0, 'test result: ok. 0 passed; 0 failed;'),
                           (0, out.replace(target, 'unrelated_test'))):
            self.assertFalse(composed_controls.one_test_passed(subprocess.CompletedProcess([], code, text), target))


if __name__ == '__main__':
    unittest.main()
