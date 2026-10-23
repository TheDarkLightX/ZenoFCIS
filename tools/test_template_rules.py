#!/usr/bin/env python3
"""Counterpolicy regression checks through the generated contract Authority."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
CLI = Path(os.environ['ZF_BIN']).resolve()


class TemplateRules(unittest.TestCase):
    def probe(self, relative, mutate, values):
        with tempfile.TemporaryDirectory(prefix='rules-control-', dir=os.environ.get('TMPDIR')) as tmp:
            path = Path(tmp)
            (path / 'v2').mkdir()
            original = ROOT / relative
            shutil.copyfile(original / 'project.zeno', path / 'project.zeno')
            policy = json.loads((original / 'v2/policy.json').read_text())
            mutate(policy)
            (path / 'v2/policy.json').write_text(json.dumps(policy))
            built = subprocess.run([str(CLI), '--no-kb', 'generate', 'contract', str(path)], capture_output=True, text=True)
            self.assertEqual(built.returncode, 0, built.stdout + built.stderr)
            packet = path / 'probe.json'
            result = subprocess.run([str(CLI), '--no-kb', 'contract', 'probe', 'explain', str(path), '--input', values, '--out', str(packet), '--format', 'json'], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            outcome = json.loads(packet.read_text())['result']['authority']
            print(json.dumps({'input': values, 'outcome': outcome}, sort_keys=True), flush=True)
            return outcome

    def test_treasury_last_tick_proposal_is_refused_by_law(self):
        def mutate(policy):
            case = next(c for c in policy['cases'] if c['reason'] == 213)
            case['when'] = 'proposes && now > 11'
        outcome = self.probe('crates/zeno-fcis-cli/templates/agent-treasury-guard', mutate,
                             '6 1 0 0 170 0 0 175 173 1 1 0 0 178 11 1 11 180')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 510, outcome)

    def test_treasury_swap_request_cannot_be_omitted(self):
        def mutate(policy):
            for case in policy['cases']:
                case['outbox'] = []
        outcome = self.probe('crates/zeno-fcis-cli/templates/agent-treasury-guard', mutate,
                             '6 1 0 0 170 0 0 175 173 1 1 0 0 178 1 1 1 180')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 513, outcome)

    def test_treasury_swap_cannot_deliver_reversed_assets(self):
        def mutate(policy):
            for case in policy['cases']:
                for delivery in case['outbox']:
                    delivery['payload']['161'] = 184
                    delivery['payload']['162'] = 183
        outcome = self.probe('crates/zeno-fcis-cli/templates/agent-treasury-guard', mutate,
                             '6 1 0 0 170 0 0 175 173 1 1 0 0 178 1 1 1 180')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 515, outcome)

    def test_escrow_seller_cannot_pay_itself_when_marking_shipped(self):
        def mutate(policy):
            ship = next(c for c in policy['cases'] if c['when'] == 'action == 161')
            release = next(c for c in policy['cases'] if c['when'] == 'action == 162')
            ship['post'] = dict(release['post'])
            ship['outbox'] = release['outbox']
        outcome = self.probe('examples/apps/escrow-deal', mutate, '151 2 0 0 161 1 171 3')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 512, outcome)

    def test_escrow_cannot_release_before_shipping(self):
        def mutate(policy):
            policy['cases'][1]['when'] = policy['cases'][1]['when'].replace(
                '(action == 162 && status != 152)', '(action == 162 && status != 151 && status != 152)')
        outcome = self.probe('examples/apps/escrow-deal', mutate, '151 2 0 0 162 1 170 3')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 512, outcome)

    def test_escrow_wrong_destination_cannot_receive_the_payout(self):
        def mutate(policy):
            for case in policy['cases']:
                for delivery in case['outbox']:
                    delivery['destination'] = 'thief'
        outcome = self.probe('examples/apps/escrow-deal', mutate, '152 2 0 0 162 1 170 2')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 517, outcome)














if __name__ == '__main__':
    unittest.main()
