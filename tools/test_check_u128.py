"""Cargo target isolation controls; the gate supplies native mutation evidence."""
from pathlib import Path
import os
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import check_u128 as gate


class TargetIsolationTests(unittest.TestCase):
    def native_environment(self, root, environment):
        binary = root / 'test-binary'
        binary.write_bytes(b'fixture binary, not native evidence')

        def run(command, **kwargs):
            kwargs['stdout'].write(
                f'Running tests/u128_contract.rs ({binary})\n'
                'running 1 test\n1 passed; 0 failed\n')
            return subprocess.CompletedProcess(command, 0)

        with patch.dict(os.environ, environment, clear=True), \
             patch.object(gate, 'ROOT', root), \
             patch.object(gate.shutil, 'disk_usage', return_value=SimpleNamespace(free=7*1024**3)), \
             patch.object(gate.subprocess, 'check_output', return_value='disk fixture\n'), \
             patch.object(gate.subprocess, 'run', side_effect=run) as invoked:
            gate.native('u128_conversion_boundaries', root / 'native.log')
        return invoked.call_args.kwargs['env']

    def test_caller_target_is_used_without_a_shared_fallback(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = str(root / 'private-target')
            actual = self.native_environment(root, {'CARGO_TARGET_DIR': target})
            self.assertEqual(actual['CARGO_TARGET_DIR'], target)

    def test_missing_target_uses_the_current_source_tree(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            actual = self.native_environment(root, {})
            self.assertEqual(actual['CARGO_TARGET_DIR'], str(root / 'target'))


if __name__ == '__main__':
    unittest.main()
