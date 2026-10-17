"""Requirement checks; SPOT_DEFECT plants actual oracle source defects."""
import dataclasses
import copy
import os
import sys
import tempfile
import subprocess
import inspect
import json
import hashlib
import unittest
from unittest.mock import patch
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
import spot_reference as reference
from check_no_free_value import check_step, dominates, holdings
from spot_reference import Command, K, Pool, World, step
from audit_explicit_state import BOUND, PUBLICATION, ROOT, audit, check_identity, check_shell
from native_check import strict_json
from replay_corpus import validate_rows, require_cli_identity
import native_check


def implementation():
    defect = os.environ.get("SPOT_DEFECT")
    if not defect:
        return step
    source = Path(reference.__file__).read_text()
    old, new = {
        "fee-free": ("fee = mc(gross, 30, 10000)", "fee = 0"),
        "burn-up": ("o0, o1 = mf(a, p.r0, p.supply), mf(a, p.r1, p.supply)",
                    "o0, o1 = mc(a, p.r0, p.supply), mc(a, p.r1, p.supply)"),
        "ignore-donations": ("mint = min(mf(u0, p.supply, p.r0), mf(u1, p.supply, p.r1))",
                             "mint = min(mf(u0, p.supply, p.r0 - 100), mf(u1, p.supply, p.r1 - 100))"),
    }[defect]
    if source.count(old) != 1:
        raise ValueError("defect site must be unique")
    # Execute only the retained local source, with original immutable types so
    # the exact-type admission checks remain intact.
    namespace = dict(vars(reference))
    body = source[source.index("def step(world, cmd):"):source.index("\ndef limbs(value):")]
    exec(compile(body.replace(old, new), f"<planted-{defect}>", "exec"), namespace)
    return namespace["step"]


def funded(x=1001, y=1001, supply=1001):
    return World(Pool(x, y, supply), (x, y, 0, 0, 100000, 100000, 100000, 100000),
                 (supply - K, 0, K))


class Stage2(unittest.TestCase):
    def test_cli_identity_is_checked(self):
        with tempfile.TemporaryDirectory(prefix="spot-cli-identity-") as directory:
            cli = Path(directory) / "cli"
            cli.write_bytes(b"stable executable fixture")
            wanted = hashlib.sha256(cli.read_bytes()).hexdigest()
            if os.environ.get("SPOT_NATIVE_DEFECT") == "replace-cli": cli.write_bytes(b"replacement")
            self.assertEqual(require_cli_identity(cli, wanted), wanted)
            cli.write_bytes(b"replacement")
            with self.assertRaisesRegex(ValueError, "CLI identity changed"):
                require_cli_identity(cli, wanted)

    def test_native_timeout_retains_measured_work(self):
        before, command = funded(), Command(1, a=100)
        function = native_check.probe
        if os.environ.get("SPOT_NATIVE_DEFECT") == "omit-timeout-record":
            source = inspect.getsource(function)
            start = source.index('        (out / f"{label}.command.json").write_text')
            end = source.index('\n        raise', start)
            namespace = dict(vars(native_check))
            exec(compile(source[:start] + "        pass" + source[end:], "<omit-timeout-record>", "exec"), namespace)
            function = namespace["probe"]
        with tempfile.TemporaryDirectory(prefix="spot-timeout-") as directory:
            with patch("native_check.subprocess.run", side_effect=subprocess.TimeoutExpired(["planted"], 20, output=b"partial")):
                with self.assertRaises(subprocess.TimeoutExpired):
                    function(before, command, step(before, command), directory, "timeout")
            record = json.loads((Path(directory) / "timeout.command.json").read_text())
            self.assertEqual((record["exit_code"], record["status"], record["stdout"]), (124, "timeout-UNKNOWN", "partial"))
            self.assertGreaterEqual(record["seconds"], 0)

    def test_explicit_state_shell_and_receipt_identity(self):
        self.assertEqual(audit()["authority"], "none")
        bound = (ROOT / BOUND).read_text()
        publication = (ROOT / PUBLICATION).read_text()
        with self.assertRaisesRegex(ValueError, "receipt_core_identity"):
            check_identity(bound.replace("outcome::seal(&completed, &self.identity)", "outcome::seal(&completed, &[])"), publication)
        shell = (ROOT / "core-apps/zenodex-spot/tools/native_check.py").read_text()
        if os.environ.get("SPOT_AUDIT_DEFECT") == "receipt-identity":
            check_identity(bound.replace("outcome::seal(&completed, &self.identity)", "outcome::seal(&completed, &[])"), publication)
        if os.environ.get("SPOT_AUDIT_DEFECT") == "shell-decision":
            check_shell(shell.replace("    out = Path(out)", "    if command.a > 1000: return None\n    out = Path(out)"))
        with self.assertRaisesRegex(ValueError, "shell_decision"):
            check_shell(shell.replace("    out = Path(out)", "    if command.a > 1000: return None\n    out = Path(out)"))

    def test_l207_intake_complete(self):
        path = ROOT / "core-apps/zenodex-spot/corpus/l207.jsonl"
        rows = [strict_json(line) for line in path.read_text().splitlines()]
        defect = os.environ.get("SPOT_INTAKE_DEFECT")
        if defect == "missing": rows.pop()
        elif defect == "duplicate": rows[-1] = copy.deepcopy(rows[0])
        elif defect == "label": rows[0]["id"] = "invented-label"
        elif defect == "exclusion": next(row for row in rows if row["disposition"] == "inapplicable").pop("scope_evidence")
        elif defect == "golden": rows[0]["expect"]["outputs"].pop()
        elif defect == "false-golden": rows[0]["expect"]["outputs"][0] += 1
        elif defect == "history": next(row for row in rows if row.get("history_index") == 1)["history_pre"] = "0" * 64
        elif defect == "rebase": next(row for row in rows if row.get("expect", {}).get("reason") == "ZERO_PAYOUT")["expect"]["reason"] = "SLIPPAGE"
        self.assertEqual(len(validate_rows(rows, "L207")), 209)

    def test_fixed_fee_control(self):
        before, command = funded(), Command(1, a=100)
        result = implementation()(before, command)
        self.assertIsNone(result.reason)
        check_step(before, command, result)

    def test_burn_floor_control(self):
        before, command = funded(1002, 1002), Command(4, a=1)
        result = implementation()(before, command)
        self.assertIsNone(result.reason)
        check_step(before, command, result)

    def test_mint_uses_donated_reserves_control(self):
        # Valid explicit snapshot after an external increase, not a new Spot
        # operation. Both authoritative reserves and their Balance mirror agree.
        before, command = funded(1101, 1101), Command(3, a=1101, b=1101)
        result = implementation()(before, command)
        self.assertIsNone(result.reason)
        check_step(before, command, result)
        self.assertEqual(result.outputs[2], 1001)

    def test_round_trip(self):
        before = funded(1000000, 1000000, 1001)
        first = step(before, Command(1, a=10000))
        second = step(first.post, Command(1, direction=1, a=first.outputs[1]))
        self.assertIsNone(second.reason)
        self.assertFalse(dominates(holdings(second.post, 0, floor=False), holdings(before, 0, floor=False)))

    def test_add_then_burn(self):
        before = funded()
        added = step(before, Command(3, a=1001, b=1001))
        burned = step(added.post, Command(4, a=added.outputs[2]))
        self.assertIsNone(burned.reason)
        self.assertFalse(dominates(holdings(burned.post, 0, floor=False), holdings(before, 0, floor=False)))

    def test_split_versus_single_swap(self):
        before = funded(1000000, 1000000, 1001)
        single = step(before, Command(1, a=10000))
        first = step(before, Command(1, a=5000))
        split = step(first.post, Command(1, a=5000))
        self.assertIsNone(single.reason)
        self.assertIsNone(split.reason)
        self.assertFalse(dominates(holdings(single.post, 0, floor=False), holdings(before, 0, floor=False)))
        self.assertFalse(dominates(holdings(split.post, 0, floor=False), holdings(before, 0, floor=False)))
        self.assertLessEqual(first.outputs[1] + split.outputs[1], single.outputs[1])

    def test_donation_then_deposit_boundary(self):
        # An unmirrored direct donation is an invalid snapshot; never silently
        # synchronize it or invent a sixth owner-approved command.
        before = funded()
        unmirrored = dataclasses.replace(before, balances=(1101, 1101, *before.balances[2:]))
        refused = step(unmirrored, Command(3, a=1001, b=1001))
        self.assertEqual(refused.reason, "DOMAIN")
        self.assertEqual(refused.post, unmirrored)
        self.assertEqual(refused.effects, ())
        self.test_mint_uses_donated_reserves_control()

    def test_dominance_includes_claims_and_detects_free_credit(self):
        before = funded()
        richer = dataclasses.replace(before, balances=(*before.balances[:4], before.balances[4] + 1, *before.balances[5:]))
        self.assertTrue(dominates(holdings(richer, 0, floor=False), holdings(before, 0, floor=False)))
        self.assertEqual(holdings(before, 0, floor=False), (100001, 100001))


if __name__ == "__main__":
    unittest.main()
