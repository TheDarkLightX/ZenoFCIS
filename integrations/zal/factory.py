"""Additive declaration lowering and actual factory qualification adapter.

The reference decision table is computed independently by behavior.Model.
The real generated application's Authority must match every declared tuple.
No replacement receipt, publication, application adoption or upgrade is made.
"""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

from behavior import Formula, Model, Refusal, check
from workflow import checker_identity


def lower(model: Model) -> dict[str, str]:
    report = check(model, checker_identity())
    if report["status"] != "pass-with-scope":
        raise Refusal("Lowering needs passing finite-profile checks and no unresolved obligations")
    if len(model.states) < 2:
        raise Refusal("This factory-qualified subset requires at least two states for its independent rejected-genesis check")
    states = {name: 150 + i for i, name in enumerate(model.states)}
    events = {name: 120 + i for i, name in enumerate(model.events)}
    facts = {name: 160 + i for i, name in enumerate(model.facts)}
    reasons = {name: 200 + i for i, name in enumerate(sorted({model.default, *[r.reason for r in model.rules if r.reason]}))}

    def expression(formula: Formula, post=False):
        if formula.op in {"true", "false"}:
            return formula.op
        if formula.op == "fact":
            return f"context.102.{facts[formula.args[0]]}"
        if formula.op == "state":
            return f"{'post' if post else 'pre'}.100.110 == {states[formula.args[0]]}"
        if formula.op == "not":
            return f"!({expression(formula.args[0], post)})"
        return "(" + (" && " if formula.op == "and" else " || ").join(expression(a, post) for a in formula.args) + ")"

    project = ["zeno 1;", f"project 1 {model.name};", "namespace 2 dialogue;",
               "type 100 state ControlState;", "type 101 command Event;",
               "type 102 context Observations;" if facts else "type 102 int UnitContext in 0..=0;",
               "type 105 data ControlLabel;", "field 110 100 control 105;"]
    if facts:
        project.append("type 106 bool ContextFact;")
        project.extend(f"field {fid} 102 observed_{name} 106;" for name, fid in facts.items())
    project.extend(f"variant {sid} 105 state_{name} none;" for name, sid in states.items())
    project.extend(f"variant {eid} 101 event_{name} none;" for name, eid in events.items())
    project.extend(f"reason {rid} refused_{name} precedence {i};" for i, (name, rid) in enumerate(reasons.items()))
    project.extend(["component 400 control {", "  owns 100;", "  reads pre.100;", "  writes post.100;",
                    "  contexts context.102;", "  budget steps 64;", "}", "merge [400];"])
    # Required structural law is explicitly type-only, not evidence about intent.
    shape = " || ".join(f"post.100.110 == {sid}" for sid in states.values())
    project.append(f"law 500 declared_control_domain on commit, genesis = {shape};")
    kinds = {"500": "StateInvariant"}
    for i, (name, formula) in enumerate(model.invariants, 501):
        project.append(f"law {i} invariant_{name} on commit, genesis = {expression(formula, True)};")
        kinds[str(i)] = "StateInvariant"
    cases = []
    for rule in model.rules:
        cases.append({"rule": rule.id,
                      "when": f"pre.100.110 == {states[rule.source]} && command.101 == {events[rule.event]} && ({expression(rule.guard)})",
                      "class": "Accept" if rule.target else "Reject",
                      "reason": reasons[rule.reason] if rule.reason else None,
                      "post": {"110": states[rule.target]} if rule.target else {}, "outbox": []})
    # The factory's ordered cases are equivalent only because check() proved
    # these guards disjoint over the declared finite domain.
    cases.append({"rule": "default", "when": "true", "class": "Reject",
                  "reason": reasons[model.default], "post": {}, "outbox": []})
    policy = {"schema": "zeno-fcis/template-declarative-policy/2", "template": model.name.replace("_", "-"),
              "roots": {"state": 100, "command": 101, "context": 102},
              "leaf_bindings": {"106": ["Bool"]} if facts else {}, "variables": {},
              "cases": cases, "genesis": {"110": states[model.initial]}, "law_kinds": kinds}

    def example(state, event, context):
        result = model.decide(state, event, context)
        inputs = [states[state], events[event], *[int(context[name]) for name in facts]]
        if not facts:
            inputs.append(0)
        reason = str(reasons[result["reason"]]) if result["reason"] else "-"
        return f"{' '.join(map(str, inputs))} | {result['class'].lower()} {reason} {states[result['state']]} | -"

    table = [example(state, event, context) for state, event, context in model.inputs()]
    # Persistent journey examples must be one actual trace, unlike the whole
    # Cartesian table. Take a deterministic path, then stop on a repeated state.
    journey = []
    state, seen = model.initial, set()
    while state not in seen:
        seen.add(state)
        options = [(event, context) for s, event, context in model.inputs()
                   if s == state and model.decide(s, event, context)["class"] == "Accept"]
        if not options:
            break
        event, context = options[0]
        journey.append(example(state, event, context))
        state = model.decide(state, event, context)["state"]
    # At least one rejection tests its explicit unchanged frame.
    rejects = [(event, context) for s, event, context in model.inputs()
               if s == state and model.decide(s, event, context)["class"] == "Reject"]
    if rejects:
        journey.append(example(state, *rejects[0]))
    correspondence = '''//! Exhaustive ZAL reference decisions versus the actual library Authority.
#[test]
fn zal_whole_declared_domain_matches_authority() {
    let contract = application::v2_contract::Contract::new();
    let descriptor = contract.descriptor();
    let authority = application::authority(&descriptor).unwrap_or_else(|e| panic!("{e}"));
    let rows = application::examples(include_str!("zal-authority-examples.txt"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(!rows.is_empty());
    for row in &rows {
        application::check(&authority, row).unwrap_or_else(|e| panic!("{e}"));
    }
    println!("ZAL whole-domain tuples checked: {}", rows.len());
}
'''
    mapping = {"schema": "zal/factory-source-map/1", "revision": model.revision,
               "states": states, "events": events, "context_fields": facts, "reasons": reasons,
               "case_ids": [r.id for r in model.rules] + ["default"],
               "scope": "class, reason, successor and empty deliveries; not Step or publication identity equivalence"}
    return {"project.zeno": "\n".join(project) + "\n", "v2/policy.json": json.dumps(policy, indent=2) + "\n",
            "tests/decision-examples.txt": "\n".join(journey) + "\n",
            "tests/zal-authority-examples.txt": "\n".join(table) + "\n",
            "tests/zal_correspondence.rs": correspondence, "zal-source-map.json": json.dumps(mapping, indent=2) + "\n"}


def write_new(files: dict[str, str], destination: Path):
    if destination.exists() and (destination.is_symlink() or any(destination.iterdir())):
        raise Refusal("Export destination must be absent or an empty directory")
    destination.mkdir(parents=True, exist_ok=True)
    for name, contents in files.items():
        path = destination / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(contents, encoding="ascii")


def qualify(model: Model, root: Path, cli: Path, output: Path | None = None):
    files = lower(model)
    if not cli.is_file():
        raise Refusal("Build the pinned zeno-fcis CLI first")
    report = {"schema": "zal/factory-qualification/1", "revision": model.revision,
              "checker": checker_identity(),
              "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
              "authoring_hashes": {name: hashlib.sha256((Path(__file__).parent / name).read_bytes()).hexdigest()
                                   for name in ["behavior.py", "factory.py", "workflow.py"]},
              "cli_sha256": hashlib.sha256(cli.read_bytes()).hexdigest(),
              "declaration_hashes": {name: hashlib.sha256(text.encode()).hexdigest() for name, text in files.items()},
              "authority": "none", "checks": [], "status": "unfinished"}
    with tempfile.TemporaryDirectory(prefix="zal-factory-") as temporary:
        work = Path(temporary)
        def public(text):
            for actual, neutral in [(str(cli), "<zeno-fcis-cli>"), (str(work), "<temporary>"),
                                    (str(root), "<source>"),
                                    (os.environ.get("CARGO_TARGET_DIR", str(root / "target/zal-app")), "<target>")]:
                text = text.replace(actual, neutral)
            return text
        contract, app = work / "contract", work / "app"
        write_new(files, contract)
        commands = [
            [str(cli), "generate", "contract", str(contract), "--format", "json"],
            [str(cli), "generate", "contract", str(contract), "--check", "--format", "json"],
            [str(cli), "contract", "review", str(contract), "--max-tuples", "1024", "--out", str(work / "review.json"), "--format", "json"],
            [str(cli), "new", str(app), "--contract", str(contract), "--source", str(root)],
        ]
        for command in commands:
            result = subprocess.run(command, cwd=root, capture_output=True, text=True, timeout=120, check=False)
            report["checks"].append({"command": [public(arg) for arg in command],
                                     "exit_code": result.returncode, "stdout": public(result.stdout[-8000:]), "stderr": public(result.stderr[-2000:])})
            if result.returncode:
                if output:
                    output.write_text(json.dumps(report, indent=2) + "\n")
                return report
        for name in ["tests/zal-authority-examples.txt", "tests/zal_correspondence.rs"]:
            (app / name).write_text(files[name])
        lock_command = ["cargo", "+1.97.1", "generate-lockfile", "--manifest-path", str(app / "Cargo.toml"), "--offline"]
        locked = subprocess.run(lock_command, cwd=root, capture_output=True, text=True, timeout=60, check=False)
        report["checks"].append({"command": [public(arg) for arg in lock_command],
                                 "exit_code": locked.returncode, "stdout": public(locked.stdout[-2000:]), "stderr": public(locked.stderr[-2000:])})
        if locked.returncode:
            if output:
                output.write_text(json.dumps(report, indent=2) + "\n")
            return report
        report["application_lock_sha256"] = hashlib.sha256((app / "Cargo.lock").read_bytes()).hexdigest()
        command = ["cargo", "+1.97.1", "test", "--manifest-path", str(app / "Cargo.toml"), "--locked", "--offline", "--", "--nocapture"]
        result = subprocess.run(command, cwd=root, capture_output=True, text=True, timeout=240, check=False,
                                env={**os.environ, "CARGO_TARGET_DIR": os.environ.get("CARGO_TARGET_DIR", str(root / "target/zal-app"))})
        report["checks"].append({"command": [public(arg) for arg in command],
                                 "exit_code": result.returncode, "stdout": public(result.stdout[-10000:]), "stderr": public(result.stderr[-3000:])})
        report["status"] = "pass-with-scope" if result.returncode == 0 else "unfinished"
        report["input_tuples"] = sum(1 for _ in model.inputs())
    if output:
        output.write_text(json.dumps(report, indent=2) + "\n")
    return report
