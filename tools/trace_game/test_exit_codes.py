"""Exercise verdict exits through the real game and standalone checker CLIs."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(os.environ.get('ZENO_TRACE_GAME_NATIVE'), 'set ZENO_TRACE_GAME_NATIVE to a built driver')
class VerdictExits(unittest.TestCase):
    def invoke(self, script, *args):
        result = subprocess.run([sys.executable, '-B', str(ROOT / 'tools/trace_game' / script), *map(str, args)],
                                capture_output=True, text=True, timeout=90)
        return result, json.loads(result.stdout)

    def test_game_and_replay_verdict_exits(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            for mode in ('check', 'certify', 'select-guard'):
                for label, expected in (('safe', 0), ('planted', 1)):
                    with self.subTest(mode=mode, label=label):
                        out = base / f'{mode}-{label}'
                        result, verdict = self.invoke('game.py', mode, '--game', ROOT / f'test-data/trace-games/escrow-{label}.json',
                                                     '--out', out, '--native', os.environ['ZENO_TRACE_GAME_NATIVE'])
                        self.assertEqual(result.returncode, expected, result.stderr + result.stdout)
                        self.assertIn(verdict['evidence'], ('certified safety', 'checked up to K') if expected == 0
                                      else ('counterexample', 'no guard in class'))
                        replay, checked = self.invoke('certificate_check.py', out, 'certificate.json')
                        self.assertEqual(replay.returncode, expected, replay.stderr + replay.stdout)
                        self.assertEqual(checked['evidence'], verdict['evidence'])
            # An explicitly unsupported observation model must remain UNKNOWN.
            game = json.loads((ROOT / 'test-data/trace-games/escrow-safe.json').read_text())
            game['contract'] = str(ROOT / 'test-data/trace-games/escrow-safe')
            game['observation'] = 'partial'
            path = base / 'unsupported.json'
            path.write_text(json.dumps(game))
            result, verdict = self.invoke('game.py', 'certify', '--game', path, '--out', base / 'unknown',
                                         '--native', os.environ['ZENO_TRACE_GAME_NATIVE'])
            self.assertEqual(result.returncode, 2, result.stderr + result.stdout)
            self.assertEqual(verdict['evidence'], 'UNKNOWN')
            replay, checked = self.invoke('certificate_check.py', base / 'unknown', 'certificate.json')
            self.assertEqual(replay.returncode, 2, replay.stderr + replay.stdout)
            self.assertEqual(checked['evidence'], 'UNKNOWN')

    def test_negative_and_unknown_checked_evidence_classes(self):
        from certificate_check import verdict_exit
        for evidence in ('certified unrealizability', 'exhausted fixed grammar', 'no guard in class', 'counterexample'):
            self.assertEqual(verdict_exit({'evidence': evidence}), 1)
        for evidence in ('UNKNOWN', 'unrecognized'):
            self.assertEqual(verdict_exit({'evidence': evidence}), 2)


if __name__ == '__main__':
    unittest.main()
