"""Construction and migration-ledger checks; not mutation replay evidence."""
import json
import unittest
import check_account_graph_v2 as gate

class AccountMigration(unittest.TestCase):
    def test_ten_changed_account_subjects_and_complete_legacy_dispositions(self):
        rows = gate.mutations()
        self.assertEqual(len(rows), 10)
        self.assertEqual(len(set(rows.values())), 10)
        ledger = json.loads((gate.ROOT/gate.LEDGER).read_text())
        self.assertEqual(len(ledger['abstraction_controls']), 16)
        self.assertEqual(len(ledger['composition_controls']), 3)
        self.assertEqual(len(ledger['native_consumers']), 4)
        self.assertTrue(all(row['replacement'] and row['reason'] for row in ledger['abstraction_controls']))
        targets = {target for row in ledger['abstraction_controls'] for target in row['replacement']}
        self.assertTrue({f'account-data:{name}' for name in rows} - {'account-data:account_full_deadline', 'account-data:account_backwards_fact'} <= targets)


class AccountPool(unittest.TestCase):
    def test_isolated_native_controls_keep_order_and_refuse_survivors(self):
        from contextlib import redirect_stdout
        import io
        import os
        from pathlib import Path
        import subprocess
        import tempfile
        from unittest.mock import patch
        mutations = {'lose_original_seen': 'bad one', 'account_backwards_fact': 'bad two'}
        def run(jobs, survivor):
            with tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp); (root / 'subject.rs').write_text('original')
                seen = []
                def fake(command, cwd, env, **kwargs):
                    Path(env['CARGO_TARGET_DIR']).mkdir(parents=True)
                    if cwd == root:
                        return subprocess.CompletedProcess(command, 0, 'test result: ok. 1 passed', '')
                    seen.append(Path(cwd).name)
                    if survivor and Path(cwd).name == 'lose_original_seen':
                        return subprocess.CompletedProcess(command, 0, 'test result: ok. 1 passed', '')
                    return subprocess.CompletedProcess(command, 101,
                        '---- all_reviewed_examples_bind_actual_value_bytes_without_command_reencoding stdout ----\ntest result: FAILED.', '')
                with patch.dict(os.environ, {'ZENO_FCIS_CONTROL_JOBS': str(jobs)}), \
                     patch.object(gate, 'ROOT', root), patch.object(gate, 'SUBJECT', Path('subject.rs')), \
                     patch.object(gate, 'source_files', return_value=['subject.rs']), \
                     patch.object(gate, 'mutations', return_value=mutations), \
                     patch.object(gate.subprocess, 'run', side_effect=fake), redirect_stdout(io.StringIO()):
                    out = root / 'out'
                    if survivor:
                        with self.assertRaisesRegex(RuntimeError, 'survived'):
                            gate.check(out)
                        rows = json.loads((out / 'controls.json').read_text())
                        self.assertFalse(rows[0]['killed'])
                    else:
                        gate.check(out)
                        rows = json.loads((out / 'result.json').read_text())['controls']
                self.assertCountEqual(seen, list(mutations))
                self.assertEqual([r['name'] for r in rows], list(mutations))
                return rows
        self.assertEqual(run(1, False), run(3, False))
        run(1, True); run(3, True)


if __name__ == '__main__':
    unittest.main()
