#!/usr/bin/env python3
"""Check the example apps against what an attacker would try, with the contract judges.

For examples/apps/escrow-deal and examples/apps/library-loans it runs state
and transition lemmas, implication probes and the adversarial scenarios in
each app's tests/decision-scenarios.txt, and requires every verdict to hold.
It also records the properties the judges cannot state: a lemma over
deliveries must be refused as having no contract form (exit 65), so a change
that makes it expressible is noticed.

Controls copy an app, remove a protecting law and weaken one rule, and require
the named lemma or scenario to fail; a control that passes means the property
check could not fail. Every packet is advisory and goes to --out.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
ESCROW, LIBRARY = "escrow-deal", "library-loans"

STATE = {
    ESCROW: {
        "never-paid-and-refunded": "post.100.112 == 0 || post.100.113 == 0",
        "one-deposit-at-most": "post.100.111 + post.100.112 + post.100.113 <= 3",
        "paid-only-when-released": "post.100.112 > 0 -> post.100.110 == 153",
        "refunded-only-when-refunded": "post.100.113 > 0 -> post.100.110 == 154",
    },
    LIBRARY: {
        "no-member-holds-three": "post.100.110 == 0 || post.100.110 != post.100.111 || post.100.110 != post.100.112",
    },
}
TRANSITION = {
    ESCROW: {
        "paid-once": "pre.100.112 > 0 -> post.100.112 == pre.100.112",
        "refunded-once": "pre.100.113 > 0 -> post.100.113 == pre.100.113",
        "nothing-after-the-end": "pre.100.110 != 153 && pre.100.110 != 154",
        "conserved-unless-deposit": "command.101.120 != 160 -> post.100.111 + post.100.112 + post.100.113 == pre.100.111 + pre.100.112 + pre.100.113",
        "one-deposit-adds-its-amount": "command.101.120 == 160 -> pre.100.110 == 150 && post.100.111 + post.100.112 + post.100.113 == pre.100.111 + pre.100.112 + pre.100.113 + command.101.121",
        "refund-after-day-7": "command.101.120 == 163 -> context.102.131 > 7",
        "right-party": "(command.101.120 == 160 -> context.102.130 == 170) && (command.101.120 == 161 -> context.102.130 == 171) && (command.101.120 == 162 -> context.102.130 == 170) && (command.101.120 == 163 -> context.102.130 == 172)",
    },
    LIBRARY: {
        "librarian-only": "context.102 == 130",
        "no-copy-lent-while-on-loan": "command.101.120 == 150 -> (command.101.121 == 1 -> pre.100.110 == 0) && (command.101.121 == 2 -> pre.100.111 == 0) && (command.101.121 == 3 -> pre.100.112 == 0)",
        "only-the-holder-returns": "command.101.120 == 151 -> (command.101.121 == 1 -> pre.100.110 == command.101.122 && post.100.110 == 0) && (command.101.121 == 2 -> pre.100.111 == command.101.122 && post.100.111 == 0) && (command.101.121 == 3 -> pre.100.112 == command.101.122 && post.100.112 == 0)",
        "one-copy-changes-hands": "(post.100.110 == pre.100.110 && post.100.111 == pre.100.111) || (post.100.110 == pre.100.110 && post.100.112 == pre.100.112) || (post.100.111 == pre.100.111 && post.100.112 == pre.100.112)",
    },
}
IMPLIES = {
    ESCROW: {
        "seller-paid-only-by-buyer-release": ("post.100.112 > pre.100.112", "pre.100.110 == 152 && command.101.120 == 162 && context.102.130 == 170"),
        "buyer-refunded-only-by-operator-after-day-7": ("post.100.113 > pre.100.113", "(pre.100.110 == 151 || pre.100.110 == 152) && command.101.120 == 163 && context.102.130 == 172 && context.102.131 > 7"),
    },
    LIBRARY: {},
}
# Properties the judges cannot state today; each must stay refused (exit 65).
INEXPRESSIBLE = {
    ESCROW: {"at-most-one-payout-delivery": "outbox.104 <= 1"},
    LIBRARY: {},
}
# Verdicts that refute a property, as each judge names them.
REFUTED = {"false", "not-inductive", "refuted", "failed"}
# A law a control weakens keeps its ID, scope and kind; its formula becomes
# one that holds on every decision.
WEAK = {ESCROW: "post.100.111 >= 0", LIBRARY: "post.100.110 >= 0"}
# (name, app, laws weakened in project.zeno, (rule text, replacement) in
#  v2/policy.json, lemma or scenario that must then fail)
CONTROLS = [
    ("refund-before-deadline", ESCROW, ["503"], ("day <= 7", "day <= 6"),
     ["transition:refund-after-day-7", "scenario:no-refund-until-day-8"]),
    ("seller-releases", ESCROW, ["502"], ("(action == 162 && party != 170)", "(action == 162 && party == 172)"),
     ["transition:right-party", "scenario:no-party-uses-another-partys-permission"]),
    ("refund-after-release", ESCROW, ["500"], ("(action == 163 && status != 151 && status != 152)", "(action == 163 && status != 151 && status != 152 && status != 153)"),
     ["transition:nothing-after-the-end", "state:paid-only-when-released"]),
    ("lend-a-lent-copy", LIBRARY, ["503"], ("action == 150 && holder != 0", "action == 150 && holder == 4"),
     ["transition:no-copy-lent-while-on-loan", "scenario:a-lent-copy-is-not-lent-again"]),
    ("visitor-records", LIBRARY, ["501"], ("desk != 130", "desk == 132"),
     ["transition:librarian-only", "scenario:a-visitor-records-nothing"]),
]


def judge(cli, app, out, name, arguments):
    packet = out / f"{name}.json"
    result = subprocess.run([str(cli), "contract", *arguments, str(app), "--no-kb", "--out", str(packet)],
                            capture_output=True, text=True, timeout=1800)
    data = json.loads(packet.read_text()) if packet.is_file() else None
    return result.returncode, data, result.stdout + result.stderr


def verdicts(cli, app, name, out):
    """Every check's verdict for one app directory: name -> (passed, detail)."""
    found = {}
    if STATE[name]:
        _, data, log = judge(cli, app, out, f"{name}-state", ["lemma", *sum((["--lemma", f] for f in STATE[name].values()), [])])
        rows = {l["formula"]: l for l in (data or {}).get("lemmas", [])}
        for key, formula in STATE[name].items():
            verdict = rows.get(formula, {}).get("verdict", log.strip()[-200:])
            found[f"state:{key}"] = (verdict == "inductive", verdict)
    _, data, log = judge(cli, app, out, f"{name}-transition", ["lemma", "--transition", *sum((["--lemma", f] for f in TRANSITION[name].values()), [])])
    rows = {l["formula"]: l for l in (data or {}).get("lemmas", [])}
    for key, formula in TRANSITION[name].items():
        row = rows.get(formula, {})
        verdict = row.get("verdict", log.strip()[-200:])
        found[f"transition:{key}"] = (verdict == "checked" and row.get("checked", 0) > 0, f"{verdict} on {row.get('checked')} commits")
    for key, (premise, conclusion) in IMPLIES[name].items():
        _, data, log = judge(cli, app, out, f"{name}-implies-{key}", ["probe", "implies", "--if", premise, "--then", conclusion])
        answer = (data or {}).get("result", {})
        found[f"implies:{key}"] = (answer.get("answer") == "yes", answer.get("proof", log.strip()[-200:]))
    _, data, log = judge(cli, app, out, f"{name}-scenarios", ["scenario", "--file", str(app / "tests/decision-scenarios.txt")])
    for row in (data or {}).get("scenarios", []):
        found[f"scenario:{row['name']}"] = (row["verdict"] == "passed", row["verdict"])
    if data is None:
        found["scenario:*"] = (False, log.strip()[-200:])
    return found, data


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    report, failures = {"format": "zeno-fcis/attack-judges/1", "apps": {}, "inexpressible": {}, "controls": []}, []
    for name in (ESCROW, LIBRARY):
        app = ROOT / "examples/apps" / name
        found, scenarios = verdicts(args.cli, app, name, args.out)
        mutants = (scenarios or {}).get("mutants", {})
        report["apps"][name] = {"checks": {k: {"holds": ok, "detail": d} for k, (ok, d) in found.items()},
                                "scenario_mutants": {k: mutants.get(k) for k in ("count", "killed", "refused", "survived")}}
        failures += [f"{name} {key}: {detail}" for key, (ok, detail) in found.items() if not ok]
        for key, formula in INEXPRESSIBLE[name].items():
            code, _, log = judge(args.cli, app, args.out, f"{name}-inexpressible-{key}", ["lemma", "--transition", "--lemma", formula])
            report["inexpressible"][f"{name}:{key}"] = {"formula": formula, "exit": code, "message": log.strip()[-200:]}
            if code != 65:
                failures.append(f"{name} {key}: expected no contract form (65), got {code}; record the gap as closed")
    for control, name, laws, (before, after), expected in CONTROLS:
        with tempfile.TemporaryDirectory(prefix=f"attack-{control}-") as directory:
            app = Path(directory) / name
            shutil.copytree(ROOT / "examples/apps" / name, app)
            project = (app / "project.zeno").read_text()
            for law in laws:
                project, count = re.subn(rf"(?m)^(law {law} [^=\n]*= ).*;$", rf"\g<1>{WEAK[name]};", project)
                assert count == 1, (control, law)
            (app / "project.zeno").write_text(project)
            policy = json.loads((app / "v2/policy.json").read_text())
            changed = [case for case in policy["cases"] if before in case["when"]]
            assert len(changed) == 1, (control, before)
            changed[0]["when"] = changed[0]["when"].replace(before, after)
            (app / "v2/policy.json").write_text(json.dumps(policy, indent=1))
            found, _ = verdicts(args.cli, app, name, Path(directory) / "packets")
            # A kill is a judge's verdict against the property, never an error.
            failed = [key for key in expected if not found.get(key, (True, ""))[0]
                      and str(found[key][1]).split(" ")[0] in REFUTED]
            report["controls"].append({"control": control, "app": name, "weakened_laws": laws, "rule": [before, after],
                                       "expected_to_fail": expected, "failed": failed, "killed": failed == expected,
                                       "verdicts": {key: found.get(key, (None, "missing"))[1] for key in expected}})
            if failed != expected:
                failures.append(f"control {control}: expected {expected} to fail, failed {failed}")
    (args.out / "attack-judges.json").write_text(json.dumps(report, indent=2) + "\n")
    for line in failures:
        print(line, file=sys.stderr)
    checks = sum(len(app["checks"]) for app in report["apps"].values())
    print(f"attack judges: {'PASS' if not failures else 'FAIL'}; {checks} checks on 2 apps, "
          f"{len(report['inexpressible'])} recorded gap, {len(CONTROLS)} controls", flush=True)
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
