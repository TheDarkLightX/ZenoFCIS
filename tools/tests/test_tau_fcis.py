from __future__ import annotations
import json
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path
APP = Path(__file__).resolve().parents[1] / 'tau_fcis.py'
COMMIT = 'a' * 40

def write_exec(path: Path, body: str) -> None:
    path.write_text('#!/usr/bin/python3\n' + body, encoding='utf-8')
    path.chmod(493)

class TauFcisTests(unittest.TestCase):

    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.spec = self.root / 'spec.tau'
        self.spec.write_text('G(o1[t]:bv = i1[t]:bv).\n', encoding='utf-8')
        self.tau = self.root / 'tau'
        write_exec(self.tau, "import sys\nprint('tau test-build 1')\nsys.exit(0)\n")
        self.codegen = self.root / 'tau_codegen'
        write_exec(self.codegen, textwrap.dedent("                import os, pathlib, sys\n                spec = pathlib.Path(sys.argv[1]).read_bytes()\n                out = pathlib.Path(sys.argv[sys.argv.index('-o') + 1])\n                out.write_bytes(b'// deterministic\\n' + spec)\n                strategy = os.environ.get('TAU_LTL_EXPORT_STRATEGY_FILE')\n                if strategy:\n                    pathlib.Path(strategy).write_text('HOA: v1\\n--BODY--\\nState: 0\\n[t] 0\\n--END--\\n')\n                print('generated')\n                "))
        self.ltlsynt = self.root / 'ltlsynt'
        write_exec(self.ltlsynt, "print('ltlsynt test')\n")

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def run_app(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run([sys.executable, str(APP), *args], text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)

    def capture(self) -> Path:
        bundle = self.root / 'bundle'
        result = self.run_app('capture', '--spec', str(self.spec), '--tau', str(self.tau), '--tau-codegen', str(self.codegen), '--ltlsynt', str(self.ltlsynt), '--tau-source-commit', COMMIT, '--out', str(bundle))
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertEqual(payload['classification'], 'proposed_synthesis')
        return bundle

    def test_capture_and_replay(self) -> None:
        bundle = self.capture()
        receipt = json.loads((bundle / 'receipt.json').read_text())
        self.assertEqual(receipt['authority'], 'none')
        self.assertEqual(receipt['classification'], 'proposed_synthesis')
        self.assertIn('program.h', receipt['artifacts'])
        self.assertIn('strategy.hoa', receipt['artifacts'])
        replay = self.run_app('replay', '--bundle', str(bundle), '--tau', str(self.tau), '--tau-codegen', str(self.codegen), '--ltlsynt', str(self.ltlsynt))
        self.assertEqual(replay.returncode, 0, replay.stderr)
        self.assertEqual(json.loads(replay.stdout)['replay'], 'matched')

    def test_tampered_subject_blocks_replay(self) -> None:
        bundle = self.capture()
        (bundle / 'subject.tau').write_text('G(o1[t] = 0).\n', encoding='utf-8')
        replay = self.run_app('replay', '--bundle', str(bundle), '--tau', str(self.tau), '--tau-codegen', str(self.codegen), '--ltlsynt', str(self.ltlsynt))
        self.assertEqual(replay.returncode, 2)
        self.assertIn('artifact mismatch: subject.tau', replay.stderr)

    def test_changed_tool_blocks_replay(self) -> None:
        bundle = self.capture()
        with self.codegen.open('a', encoding='utf-8') as handle:
            handle.write('\n# drift\n')
        replay = self.run_app('replay', '--bundle', str(bundle), '--tau', str(self.tau), '--tau-codegen', str(self.codegen), '--ltlsynt', str(self.ltlsynt))
        self.assertEqual(replay.returncode, 2)
        self.assertIn('tau_codegen bytes do not match receipt', replay.stderr)

    def test_missing_strategy_is_blocked_but_retained(self) -> None:
        no_strategy = self.root / 'tau_codegen_no_strategy'
        write_exec(no_strategy, "import pathlib,sys\npathlib.Path(sys.argv[sys.argv.index('-o')+1]).write_text('// ok\\n')\n")
        bundle = self.root / 'blocked'
        result = self.run_app('capture', '--spec', str(self.spec), '--tau', str(self.tau), '--tau-codegen', str(no_strategy), '--tau-source-commit', COMMIT, '--out', str(bundle))
        self.assertEqual(result.returncode, 2, result.stderr)
        receipt = json.loads((bundle / 'receipt.json').read_text())
        self.assertEqual(receipt['classification'], 'blocked')
        self.assertEqual(receipt['block_reason'], 'missing_strategy')
        self.assertTrue((bundle / 'program.h').exists())

    def test_symlink_tool_is_rejected(self) -> None:
        link = self.root / 'tau-link'
        try:
            link.symlink_to(self.tau)
        except OSError:
            self.skipTest('symlink unsupported')
        bundle = self.root / 'bundle-link'
        result = self.run_app('capture', '--spec', str(self.spec), '--tau', str(link), '--tau-codegen', str(self.codegen), '--tau-source-commit', COMMIT, '--out', str(bundle))
        self.assertEqual(result.returncode, 2)
        self.assertIn('must not be a symlink', result.stderr)

    def test_receipt_tamper_is_rejected(self) -> None:
        bundle = self.capture()
        receipt_path = bundle / 'receipt.json'
        receipt = json.loads(receipt_path.read_text())
        receipt['classification'] = 'blocked'
        receipt_path.write_text(json.dumps(receipt, sort_keys=True), encoding='utf-8')
        replay = self.run_app('replay', '--bundle', str(bundle), '--tau', str(self.tau), '--tau-codegen', str(self.codegen), '--ltlsynt', str(self.ltlsynt))
        self.assertEqual(replay.returncode, 2)
        self.assertIn('receipt identity mismatch', replay.stderr)

    def test_extra_environment_value_is_hash_bound(self) -> None:
        bundle = self.root / 'env-bundle'
        result = self.run_app('capture', '--spec', str(self.spec), '--tau', str(self.tau), '--tau-codegen', str(self.codegen), '--ltlsynt', str(self.ltlsynt), '--tau-source-commit', COMMIT, '--out', str(bundle), '--env', 'TAU_TEST_MODE=alpha')
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads((bundle / 'receipt.json').read_text())
        env_record = receipt['invocation']['environment']['extra_value_sha256']
        self.assertIn('TAU_TEST_MODE', env_record)
        self.assertNotIn('alpha', json.dumps(receipt))
        replay = self.run_app('replay', '--bundle', str(bundle), '--tau', str(self.tau), '--tau-codegen', str(self.codegen), '--ltlsynt', str(self.ltlsynt), '--env', 'TAU_TEST_MODE=beta')
        self.assertEqual(replay.returncode, 2)
        self.assertIn('replay environment value mismatch: TAU_TEST_MODE', replay.stderr)

    def test_nonempty_output_directory_is_rejected(self) -> None:
        bundle = self.root / 'nonempty'
        bundle.mkdir()
        (bundle / 'keep').write_text('do not overwrite')
        result = self.run_app('capture', '--spec', str(self.spec), '--tau', str(self.tau), '--tau-codegen', str(self.codegen), '--tau-source-commit', COMMIT, '--out', str(bundle))
        self.assertEqual(result.returncode, 2)
        self.assertEqual((bundle / 'keep').read_text(), 'do not overwrite')
if __name__ == '__main__':
    unittest.main()
