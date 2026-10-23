"""Meaning consumers retain negative verdicts without treating UNKNOWN as a key."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import loop_doors_controls as controls

ROOT = Path(__file__).resolve().parents[1]


class MeaningClients(unittest.TestCase):
    def test_unknown_and_inconsistent_exit_packets_are_refused(self):
        for code, same in ((2, False), (0, False), (1, True), (1, None)):
            report = {"status": "keyed", "detail": {"same_as_original": same, "key": "d" * 64}}
            with self.subTest(code=code, same=same), patch.object(controls, 'run', return_value=(code, report)):
                self.assertIn('refused', controls.keyed(Path('cli'), Path('original'), Path('document')))

    @unittest.skipUnless(os.environ.get('ZENO_FCIS_CLI'), 'set ZENO_FCIS_CLI to a built CLI')
    def test_live_positive_and_negative_keys_reach_the_control_comparison(self):
        original = ROOT / 'crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c8.original.zcve'
        vocabulary = {"inputs": ["value", "action", "authorized"],
                      "variants": {"action": {"Inc": 150, "Dec": 151}}, "cases": 5}
        right = 'authorized ? action - (value == (action == Inc ? 8 : 0) ? 149 : 147) : 0'
        with tempfile.TemporaryDirectory() as directory:
            for formula, same in ((right, True), (right.replace('? 8 :', '? 7 :'), False)):
                document = Path(directory) / 'expressions.json'
                document.write_text(json.dumps(controls.loop_doors.document(vocabulary, [formula, 'value + 1', 'value - 1'])))
                result = controls.keyed(Path(os.environ['ZENO_FCIS_CLI']), original, document)
                self.assertNotIn('refused', result, result)
                self.assertEqual(result['same_as_original'], same)
                self.assertRegex(result['key'], r'^[0-9a-f]{64}$')


if __name__ == '__main__':
    unittest.main()
