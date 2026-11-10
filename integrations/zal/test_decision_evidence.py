"""Adversarial controls for versioned ZAL proof-carrying decisions."""

from dataclasses import replace
import json
from pathlib import Path
import subprocess
import sys
import unittest

import behavior2
from behavior import Refusal
from decision_evidence import Evidence, CheckedDecision, issue, verify, main, MAGIC, FORMAT


SOURCE = """zal 2;
machine payment;
states ready;
events pay;
context authorized;
input amount in 1..=3;
var balance in 0..=4 initially 4;
var spent in 0..=4 initially 0;
initial ready;
step atomic; frame control_and_variables;
default reject not_enabled unchanged;
rule pay_ok: ready + pay [authorized & amount <= balance] -> accept ready, balance -= amount, spent += amount;
rule pay_unauthorized: ready + pay [!authorized] -> reject unauthorized unchanged;
invariant conserved: balance + spent == 4;
require only_authorized: accepted => authorized;
"""


class DecisionEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.model = behavior2.parse(SOURCE)
        self.before = {"balance": 4, "spent": 0}
        self.context = {"authorized": True, "amount": 2}
        self.claim = issue(self.model, "ready", self.before, "pay", self.context)

    def checked(self, evidence=None, model=None, before=None, context=None, event="pay"):
        return verify(model or self.model, evidence or self.claim, "ready",
                      before if before is not None else self.before,
                      event, context if context is not None else self.context)

    def test_issue_verify_and_untrusted_wire_roundtrip(self):
        wire = self.claim.encode()
        self.assertTrue(wire.startswith(MAGIC))
        self.assertEqual(Evidence.decode(wire), self.claim)
        checked = self.checked(Evidence.decode(wire))
        self.assertIsInstance(checked, CheckedDecision)
        self.assertEqual(checked.state, "ready")
        self.assertEqual(dict(checked.variables), {"balance": 2, "spent": 2})
        self.assertEqual(checked.authority, "none")
        self.assertEqual(checked.evidence_digest, self.claim.digest)
        self.assertEqual(len(self.claim.specification), 32)
        self.assertNotEqual(self.claim.specification, self.claim.policy)
        self.assertEqual(self.claim.effect, b"")

    def test_binding_refuses_stale_snapshot_and_action_context(self):
        cases = [
            {"before": {"balance": 3, "spent": 1}},
            {"context": {"authorized": True, "amount": 1}},
            {"context": {"authorized": False, "amount": 2}},
            {"event": "other"},
        ]
        for changed in cases:
            with self.subTest(changed=changed), self.assertRaises(Refusal):
                self.checked(**changed)

    def test_forged_claim_fields_never_admit(self):
        for label, value in [
            ("specification", bytes([9]) * 32),
            ("policy", bytes([8]) * 32),
            ("prestate", b"{}"),
            ("command", b"{}"),
            ("context", b"{}"),
            ("poststate", b"{}"),
            ("effect", b"send money"),
        ]:
            with self.subTest(field=label), self.assertRaises(Refusal):
                self.checked(replace(self.claim, **{label: value}))

    def test_model_upgrade_invalidates_prior_evidence(self):
        new = behavior2.parse(SOURCE.replace("amount <= balance", "amount < balance"))
        with self.assertRaisesRegex(Refusal, "identity mismatch"):
            self.checked(model=new)

    def test_rejected_and_nonadmissible_steps_refuse(self):
        with self.assertRaises(Refusal):
            issue(self.model, "ready", self.before, "pay", {"authorized": False, "amount": 2})
        with self.assertRaises(Refusal):
            issue(self.model, "ready", {"balance": 1, "spent": 3},
                  "pay", {"authorized": True, "amount": 2})
        with self.assertRaisesRegex(Refusal, "invariant"):
            issue(self.model, "ready", {"balance": 3, "spent": 0},
                  "pay", {"authorized": True, "amount": 2})
        with self.assertRaises(Refusal):
            issue(self.model, "ready", {"balance": True, "spent": 0}, "pay", self.context)

    def test_unresolved_model_obligations_refuse(self):
        model = replace(self.model, obligations=(("unknown", "needs review"),))
        with self.assertRaisesRegex(Refusal, "unresolved"):
            issue(model, "ready", self.before, "pay", self.context)

    def test_noncanonical_and_oversized_wire_refuse(self):
        wire = self.claim.encode()
        for invalid in (wire[:-1], wire + b"x", b"wrong" + wire[5:],
                        wire[:len(MAGIC) + 65] + b"\xff\xff\xff\xff" + wire[len(MAGIC) + 69:],
                        bytes(70_000)):
            with self.subTest(length=len(invalid)), self.assertRaises(Refusal):
                Evidence.decode(invalid)
        with self.assertRaises(Refusal):
            replace(self.claim, poststate=b"x" * 16_385)
        with self.assertRaises(Refusal):
            replace(self.claim, policy=bytes(32))

    def test_wire_hash_is_not_authorization(self):
        self.assertEqual(FORMAT, "zeno-fcis/decision-evidence/1")
        self.assertEqual(self.claim.digest, Evidence.decode(self.claim.encode()).digest)
        self.assertEqual(self.checked().authority, "none")

    def test_cli_issue_and_replay(self):
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as folder:
            path = Path(folder) / "payment.zal"
            path.write_text(SOURCE)
            args = ["issue", str(path), "--state", "ready",
                    "--variables", '{"balance":4,"spent":0}', "--event", "pay",
                    "--context", '{"amount":2,"authorized":true}']
            self.assertEqual(main(args), 0)
            cmd = [sys.executable, str(Path(__file__).parent / "decision_evidence.py"), *args]
            run = subprocess.run(cmd, capture_output=True, text=True, check=True)
            data = json.loads(run.stdout)
            self.assertEqual(data["authority"], "none")
            self.assertEqual(data["digest"], self.claim.digest)
            replay = subprocess.run([sys.executable,
                  str(Path(__file__).parent / "decision_evidence.py"),
                  "verify", str(path), *args[2:],
                  "--evidence-hex", data["evidence_hex"]],
                  capture_output=True, text=True, check=True)
            self.assertEqual(json.loads(replay.stdout)["status"], "verified")


if __name__ == "__main__":
    unittest.main()
