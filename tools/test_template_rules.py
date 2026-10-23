#!/usr/bin/env python3
"""Counterpolicy regression checks through the generated contract Authority."""
import hashlib
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
            if directory := os.environ.get('ZF_CONTROL_OUTPUT'):
                identity = hashlib.sha256((relative + values + json.dumps(policy)).encode()).hexdigest()[:16]
                shutil.copytree(path, Path(directory) / identity, dirs_exist_ok=True)
            return outcome

    def test_account_eligible_login_cannot_be_arbitrarily_rejected(self):
        def mutate(policy):
            policy['cases'].insert(0, {'when': 'true', 'class': 'Reject', 'reason': 200,
                                       'post': {}, 'outbox': []})
        outcome = self.probe('crates/zeno-fcis-cli/templates/account-lockout', mutate,
                             '0 0 0 120 1 0')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 504, outcome)

    def test_account_rejection_reasons_keep_the_documented_precedence(self):
        for index, reason, values in [(0, 201, '0 901 1 120 0 0'),
                                      (1, 200, '0 901 1 120 1 0'),
                                      (2, 201, '0 901 1 122 1 0')]:
            with self.subTest(guard=index):
                def mutate(policy):
                    case = dict(policy['cases'][index])
                    case['reason'] = reason
                    policy['cases'].insert(0, case)
                outcome = self.probe('crates/zeno-fcis-cli/templates/account-lockout',
                                     mutate, values)
                self.assertEqual(outcome['class'], 'Refused', outcome)
                self.assertEqual(outcome['law'], 504, outcome)

    def test_account_failed_login_has_the_exact_failure_reason(self):
        def mutate(policy):
            # A reason has one decision class. Keep 203 on the third-failure
            # branch, and move 202 from the unused guard to the tested failure.
            policy['cases'][2]['reason'] = 200
            policy['cases'][5]['reason'] = 202
        outcome = self.probe('crates/zeno-fcis-cli/templates/account-lockout', mutate,
                             '0 0 0 121 1 0')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 505, outcome)

    def test_account_third_failure_delivers_the_locked_kind(self):
        def mutate(policy):
            for case in policy['cases']:
                if case['class'] == 'CommittedFailure':
                    for delivery in case['outbox']:
                        delivery['payload']['140'] = 151
        outcome = self.probe('crates/zeno-fcis-cli/templates/account-lockout', mutate,
                             '2 0 0 121 1 0')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 510, outcome)

    def test_account_alert_uses_the_security_destination(self):
        def mutate(policy):
            for case in policy['cases']:
                for delivery in case['outbox']:
                    delivery['destination'] = 'other-desk'
        outcome = self.probe('crates/zeno-fcis-cli/templates/account-lockout', mutate,
                             '0 0 0 122 1 1')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 511, outcome)

    def test_account_alerts_cannot_be_omitted(self):
        def mutate(policy):
            for case in policy['cases']:
                case['outbox'] = []
        for values in ['2 0 0 121 1 0', '0 0 0 122 1 1']:
            with self.subTest(input=values):
                outcome = self.probe('crates/zeno-fcis-cli/templates/account-lockout',
                                     mutate, values)
                self.assertEqual(outcome['class'], 'Refused', outcome)
                self.assertEqual(outcome['law'], 510, outcome)

    def test_account_login_success_cannot_send_an_alert(self):
        def mutate(policy):
            policy['cases'][3]['outbox'] = policy['cases'][-1]['outbox']
        outcome = self.probe('crates/zeno-fcis-cli/templates/account-lockout', mutate,
                             '0 0 0 120 1 0')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 510, outcome)

    def test_inventory_eligible_reservation_cannot_be_arbitrarily_rejected(self):
        def mutate(policy):
            policy['cases'].insert(0, {'when': 'true', 'class': 'Reject', 'reason': 200,
                                       'post': {}, 'outbox': []})
        outcome = self.probe('crates/zeno-fcis-cli/templates/inventory-reservation',
                             mutate, '3 0 150 2 1')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 503, outcome)

    def test_inventory_rejection_reasons_keep_the_documented_precedence(self):
        for index, reason, values in [(0, 203, '0 5 150 2 0'),
                                      (1, 203, '0 5 150 2 1'),
                                      (2, 203, '5 0 151 2 1'),
                                      (3, 201, '2 4 150 2 1')]:
            with self.subTest(guard=index):
                def mutate(policy):
                    case = dict(policy['cases'][index])
                    case['reason'] = reason
                    policy['cases'].insert(0, case)
                outcome = self.probe('crates/zeno-fcis-cli/templates/inventory-reservation',
                                     mutate, values)
                self.assertEqual(outcome['class'], 'Refused', outcome)
                self.assertEqual(outcome['law'], 503, outcome)

    def test_inventory_shipment_equals_the_reserved_debit(self):
        def mutate(policy):
            for case in policy['cases']:
                for delivery in case['outbox']:
                    delivery['payload']['140'] = 1
        outcome = self.probe('crates/zeno-fcis-cli/templates/inventory-reservation',
                             mutate, '0 2 152 2 1')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 510, outcome)

    def test_inventory_shipment_uses_the_warehouse_destination(self):
        def mutate(policy):
            for case in policy['cases']:
                for delivery in case['outbox']:
                    delivery['destination'] = 'other-warehouse'
        outcome = self.probe('crates/zeno-fcis-cli/templates/inventory-reservation',
                             mutate, '0 2 152 2 1')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 511, outcome)

    def test_inventory_ship_cannot_omit_its_request(self):
        def mutate(policy):
            for case in policy['cases']:
                case['outbox'] = []
        outcome = self.probe('crates/zeno-fcis-cli/templates/inventory-reservation',
                             mutate, '0 2 152 2 1')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 510, outcome)

    def test_inventory_other_actions_cannot_send_shipments(self):
        def mutate(policy):
            shipment = next(c['outbox'] for c in policy['cases'] if c['when'] == 'action == 152')
            for case in policy['cases']:
                if case['class'] == 'Accept' and not case['outbox']:
                    case['outbox'] = shipment
        for values in ['3 0 150 2 1', '0 2 151 2 1', '0 0 153 2 1']:
            with self.subTest(input=values):
                outcome = self.probe('crates/zeno-fcis-cli/templates/inventory-reservation',
                                     mutate, values)
                self.assertEqual(outcome['class'], 'Refused', outcome)
                self.assertEqual(outcome['law'], 510, outcome)

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

    def test_withdrawal_tick_cannot_stall_a_due_lane(self):
        def mutate(policy):
            policy['variables']['pay'] = '0'
        outcome = self.probe('crates/zeno-fcis-cli/templates/withdrawal-queue', mutate,
                             '2 181 2 180 0 0 0 170 162 170 1 193 0 0')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 504, outcome)

    def test_withdrawal_debit_cannot_omit_payout(self):
        def mutate(policy):
            for case in policy['cases']:
                case['outbox'] = []
        outcome = self.probe('crates/zeno-fcis-cli/templates/withdrawal-queue', mutate,
                             '2 181 2 180 0 0 0 170 162 170 1 193 0 0')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 510, outcome)

    def test_withdrawal_payout_cannot_change_the_lane(self):
        def mutate(policy):
            for case in policy['cases']:
                for delivery in case['outbox']:
                    delivery['payload']['150'] = 171
        outcome = self.probe('crates/zeno-fcis-cli/templates/withdrawal-queue', mutate,
                             '2 181 2 180 0 0 0 170 162 170 1 193 0 0')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 511, outcome)

    def test_compliance_required_block_cannot_be_allowed(self):
        def mutate(policy):
            policy['cases'].insert(3, {'when':'identity_tier == 0 && risk == 167 && amount_band == 0',
                'class':'Accept','reason':None,'post':{'120':'strikes'},'outbox':[]})
        outcome = self.probe('crates/zeno-fcis-cli/templates/compliance-gateway', mutate,
                             '0 150 160 0 167 0 0')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 504, outcome)

    def test_compliance_screen_cannot_be_arbitrarily_rejected(self):
        def mutate(policy):
            policy['cases'].insert(0, {'when':'action == 150','class':'Reject','reason':200,'post':{},'outbox':[]})
        outcome = self.probe('crates/zeno-fcis-cli/templates/compliance-gateway', mutate,
                             '0 150 160 1 165 2 0')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 506, outcome)

    def test_compliance_required_notice_cannot_be_omitted(self):
        def mutate(policy):
            for case in policy['cases']:
                case['outbox'] = []
        outcome = self.probe('crates/zeno-fcis-cli/templates/compliance-gateway', mutate,
                             '0 150 161 2 165 2 0')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 510, outcome)

    def test_compliance_alert_cannot_name_a_lower_priority_rule(self):
        def mutate(policy):
            for case in policy['cases']:
                for delivery in case['outbox']:
                    if delivery['channel'] == 301:
                        delivery['payload']['147'] = 174
        outcome = self.probe('crates/zeno-fcis-cli/templates/compliance-gateway', mutate,
                             '0 150 162 4 167 0 0')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 511, outcome)

    def test_order_checkout_cannot_omit_capture_request(self):
        def mutate(policy):
            for case in policy['cases']:
                case['outbox'] = []
        outcome = self.probe('crates/zeno-fcis-cli/templates/order-fulfillment', mutate,
                             '160 0 150 0 170 0')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 512, outcome)

    def test_order_valid_checkout_cannot_be_rejected(self):
        def mutate(policy):
            policy['cases'].insert(0, {'when':'action == 150','class':'Reject','reason':200,'post':{},'outbox':[]})
        outcome = self.probe('crates/zeno-fcis-cli/templates/order-fulfillment', mutate,
                             '160 0 150 0 170 0')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 510, outcome)

    def test_order_timeout_cannot_replace_refund_with_capture(self):
        def mutate(policy):
            for case in policy['cases']:
                for delivery in case['outbox']:
                    if delivery['channel'] == 300 and delivery['payload']['141'] == 177:
                        delivery['payload']['141'] = 175
        outcome = self.probe('crates/zeno-fcis-cli/templates/order-fulfillment', mutate,
                             '162 1 156 0 170 4')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 512, outcome)

    def test_withdrawal_pending_lane_cannot_stutter_forever(self):
        def mutate(policy):
            policy['variables']['pay'] = '0'
        outcome = self.probe('crates/zeno-fcis-cli/templates/withdrawal-queue', mutate,
                             '2 182 2 180 0 0 0 170 162 170 1 193 0 0')
        self.assertEqual(outcome['class'], 'Refused', outcome)
        self.assertEqual(outcome['law'], 504, outcome)

    def test_compliance_and_order_refuse_the_original_deny_all_counterpolicy(self):
        def mutate(policy):
            # The remaining cases are unreachable but retain every declared reason.
            policy['cases'].insert(0, {'when': 'true', 'class': 'Reject', 'reason': 200, 'post': {}, 'outbox': []})
        for application, values, law in [
            ('compliance-gateway', '0 150 160 1 165 2 0', 506),
            ('order-fulfillment', '160 0 150 0 170 0', 510),
        ]:
            with self.subTest(application=application):
                outcome = self.probe('crates/zeno-fcis-cli/templates/' + application, mutate, values)
                self.assertEqual(outcome['class'], 'Refused', outcome)
                self.assertEqual(outcome['law'], law, outcome)


if __name__ == '__main__':
    unittest.main()
