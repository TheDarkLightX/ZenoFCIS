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

if __name__ == '__main__':
    unittest.main()
