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
import behavior2
from workflow import checker_identity


CORRESPONDENCE_TEST = '''//! Exhaustive ZAL reference decisions versus the actual library Authority.
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


def lower(model: Model) -> dict[str, str]:
    if isinstance(model, behavior2.Machine):
        return lower_machine(model)
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
    correspondence = CORRESPONDENCE_TEST
    mapping = {"schema": "zal/factory-source-map/1", "revision": model.revision,
               "states": states, "events": events, "context_fields": facts, "reasons": reasons,
               "case_ids": [r.id for r in model.rules] + ["default"],
               "scope": "class, reason, successor and empty deliveries; not Step or publication identity equivalence"}
    return {"project.zeno": "\n".join(project) + "\n", "v2/policy.json": json.dumps(policy, indent=2) + "\n",
            "tests/decision-examples.txt": "\n".join(journey) + "\n",
            "tests/zal-authority-examples.txt": "\n".join(table) + "\n",
            "tests/zal_correspondence.rs": correspondence, "zal-source-map.json": json.dumps(mapping, indent=2) + "\n"}


def _partial(node, accepted):
    """Fold the step's outcome class into a requirement; return a Python bool when it is constant."""
    op = node.op
    if op == "accepted":
        return accepted
    if op in {"true", "false"}:
        return op == "true"
    if op == "not":
        inner = _partial(node.args[0], accepted)
        return not inner if isinstance(inner, bool) else behavior2.Expr("not", (inner,))
    if op in {"and", "or"}:
        parts = [_partial(a, accepted) for a in node.args]
        if (False if op == "and" else True) in parts:
            return op == "or"
        kept = tuple(p for p in parts if not isinstance(p, bool))
        return op == "and" if not kept else kept[0] if len(kept) == 1 else behavior2.Expr(op, kept)
    if op == "implies":
        premise, conclusion = (_partial(a, accepted) for a in node.args)
        if premise is False or conclusion is True:
            return True
        if premise is True:
            return conclusion
        return behavior2.Expr("not", (premise,)) if conclusion is False else behavior2.Expr("implies", (premise, conclusion))
    if op == "iff":
        left, right = (_partial(a, accepted) for a in node.args)
        if isinstance(left, bool) and isinstance(right, bool):
            return left == right
        if isinstance(left, bool) or isinstance(right, bool):
            constant, other = (left, right) if isinstance(left, bool) else (right, left)
            return other if constant else behavior2.Expr("not", (other,))
        return behavior2.Expr("iff", (left, right))
    return node


def lower_machine(model: behavior2.Machine) -> dict[str, str]:
    """Lower profile 2 to the contract; constructs without a contract form refuse by name."""
    for label, collection in [("temporal property", model.properties), ("fairness assumption", model.fairness)]:
        for name, _ in collection:
            raise Refusal(f"unsupported lowering: {label} {name}")
    for kind, _ in model.obligations:
        raise Refusal(f"unsupported lowering: unresolved {kind}")
    report = behavior2.check(model, checker_identity())
    if report["status"] != "pass-with-scope":
        raise Refusal("Lowering needs passing finite-profile checks and no unresolved obligations")
    if len(model.states) < 2:
        raise Refusal("This factory-qualified subset requires at least two states for its independent rejected-genesis check")
    states = {name: 150 + i for i, name in enumerate(model.states)}
    events = {name: 120 + i for i, name in enumerate(model.events)}
    facts = {name: 160 + i for i, name in enumerate(model.facts)}
    inputs = {item.name: 164 + i for i, item in enumerate(model.inputs)}
    variables = {item.name: 170 + i for i, item in enumerate(model.variables)}
    reasons = {name: 200 + i for i, name in enumerate(sorted({model.default, *[r.reason for r in model.rules if r.reason]}))}

    def term(node, root):
        """root names where variables are read: pre, post, or the requirement law's pre/post pair."""
        op = node.op
        if op == "int":
            return str(node.args[0]) if node.args[0] >= 0 else f"({node.args[0]})"
        if op == "input":
            return f"context.102.{inputs[node.args[0]]}"
        if op in {"var", "var_next"}:
            return f"{root[op == 'var_next']}.100.{variables[node.args[0]]}"
        return f"({term(node.args[0], root)} {'+' if op == 'add' else '-'} {term(node.args[1], root)})"

    def expression(node, root, law=False):
        """Case guards read a Boolean fact directly; a law formula's atoms must be comparisons."""
        op = node.op
        if op in {"true", "false"}:
            return op
        if op == "fact":
            return f"context.102.{facts[node.args[0]]}" + (" == 1" if law else "")
        if op in {"state", "state_next"}:
            return f"{root[op == 'state_next']}.100.110 == {states[node.args[0]]}"
        if op == "event":
            return f"command.101 == {events[node.args[0]]}"
        if op == "not":
            return f"!({expression(node.args[0], root, law)})"
        if op in {"and", "or"}:
            return "(" + (" && " if op == "and" else " || ").join(expression(a, root, law) for a in node.args) + ")"
        if op == "implies":
            return f"({expression(node.args[0], root, law)} -> {expression(node.args[1], root, law)})"
        if op == "iff":
            left, right = expression(node.args[0], root, law), expression(node.args[1], root, law)
            return f"(({left} -> {right}) && ({right} -> {left}))"
        if op == "cmp":
            return f"({term(node.args[1], root)} {node.args[0]} {term(node.args[2], root)})"
        raise Refusal(f"unsupported lowering: {op}")

    observed = bool(facts or inputs)
    project = ["zeno 1;", f"project 1 {model.name};", "namespace 2 dialogue;",
               "type 100 state ControlState;", "type 101 command Event;",
               "type 102 context Observations;" if observed else "type 102 int UnitContext in 0..=0;",
               "type 105 data ControlLabel;"]
    if facts:
        project.append("type 106 bool ContextFact;")
    project.extend(f"type {107 + i} int Variable{i} in {v.low}..={v.high};" for i, v in enumerate(model.variables))
    project.extend(f"type {111 + i} int Input{i} in {v.low}..={v.high};" for i, v in enumerate(model.inputs))
    project.append("field 110 100 control 105;")
    project.extend(f"field {fid} 100 var_{name} {107 + i};" for i, (name, fid) in enumerate(variables.items()))
    project.extend(f"field {fid} 102 observed_{name} 106;" for name, fid in facts.items())
    project.extend(f"field {fid} 102 input_{name} {111 + i};" for i, (name, fid) in enumerate(inputs.items()))
    project.extend(f"variant {sid} 105 state_{name} none;" for name, sid in states.items())
    project.extend(f"variant {eid} 101 event_{name} none;" for name, eid in events.items())
    project.extend(f"reason {rid} refused_{name} precedence {i};" for i, (name, rid) in enumerate(reasons.items()))
    project.extend(["component 400 control {", "  owns 100;", "  reads pre.100;", "  writes post.100;",
                    "  contexts context.102;", "  budget steps 64;", "}", "merge [400];"])
    shape = " || ".join(f"post.100.110 == {sid}" for sid in states.values())
    project.append(f"law 500 declared_control_domain on commit, genesis = {shape};")
    kinds = {"500": "StateInvariant"}
    for i, (name, formula) in enumerate(model.invariants, 501):
        project.append(f"law {i} invariant_{name} on commit, genesis = {expression(formula, ('post', 'post'), True)};")
        kinds[str(i)] = "StateInvariant"
    law = 520
    for name, formula in model.requirements:
        # A rejection keeps every field, so its post-state reads are the pre-state.
        for scope, accepted, root, kind in [("accept", True, ("pre", "post"), "AuthoritySubjectRecipient"),
                                            ("reject", False, ("pre", "pre"), "RejectNoAuthority")]:
            folded = _partial(formula, accepted)
            if folded is True:
                continue
            body = "false" if folded is False else expression(folded, root, True)
            project.append(f"law {law} require_{name}_{scope} on {scope} = {body};")
            kinds[str(law)] = kind
            law += 1

    def update(item):
        value = term(item.term, ("pre", "post"))
        if item.op == "=":
            return item.term.args[0] if item.term.op == "int" else value
        return f"pre.100.{variables[item.variable]} {'+' if item.op == '+=' else '-'} {value}"

    cases = []
    for rule in model.rules:
        post = {"110": states[rule.target], **{str(variables[u.variable]): update(u) for u in rule.updates}} if rule.target else {}
        cases.append({"rule": rule.id,
                      "when": f"pre.100.110 == {states[rule.source]} && command.101 == {events[rule.event]} && ({expression(rule.guard, ('pre', 'pre'))})",
                      "class": "Accept" if rule.target else "Reject",
                      "reason": reasons[rule.reason] if rule.reason else None, "post": post, "outbox": []})
    # The factory's ordered cases are equivalent only because check() proved
    # these guards disjoint over the declared finite domain.
    cases.append({"rule": "default", "when": "true", "class": "Reject",
                  "reason": reasons[model.default], "post": {}, "outbox": []})
    policy = {"schema": "zeno-fcis/template-declarative-policy/2", "template": model.name.replace("_", "-"),
              "roots": {"state": 100, "command": 101, "context": 102},
              "leaf_bindings": {"106": ["Bool"]} if facts else {}, "variables": {},
              "cases": cases,
              "genesis": {"110": states[model.initial], **{str(variables[v.name]): v.initial for v in model.variables}},
              "law_kinds": kinds}
    names = [v.name for v in model.variables]

    def admissible(state, values):
        probe = behavior2._Step(state, values, None, {})
        return all(behavior2.holds(f, probe) for _, f in model.invariants)

    def example(state, values, event, context):
        result = model.decide(state, values, event, context)
        inputs_row = [states[state], *[values[n] for n in names], events[event],
                      *[int(context[n]) for n in facts], *[context[n] for n in inputs]]
        if not observed:
            inputs_row.append(0)
        reason = str(reasons[result["reason"]]) if result["reason"] else "-"
        post = [states[result["state"]], *[result["variables"][n] for n in names]]
        return f"{' '.join(map(str, inputs_row))} | {result['class'].lower()} {reason} {' '.join(map(str, post))} | -"

    domain = [row for row in model.inputs_domain() if admissible(row[0], row[1])]
    table = [example(*row) for row in domain]
    # One deterministic persistent journey from genesis, stopping on a repeated configuration.
    journey, seen = [], set()
    state, values = model.initial, model.initial_values()
    contexts = list(model.contexts())
    while (state, tuple(values[n] for n in names)) not in seen:
        seen.add((state, tuple(values[n] for n in names)))
        options = [(event, context) for event in model.events for context in contexts
                   if model.decide(state, values, event, context)["class"] == "Accept"]
        if not options:
            break
        event, context = options[0]
        journey.append(example(state, values, event, context))
        result = model.decide(state, values, event, context)
        state, values = result["state"], result["variables"]
    rejects = [(event, context) for event in model.events for context in contexts
               if model.decide(state, values, event, context)["class"] == "Reject"]
    if rejects:
        journey.append(example(state, values, *rejects[0]))
    mapping = {"schema": "zal/factory-source-map/2", "profile": behavior2.PROFILE, "revision": model.revision,
               "states": states, "events": events, "context_fields": facts, "input_fields": inputs,
               "variable_fields": variables, "reasons": reasons,
               "case_ids": [r.id for r in model.rules] + ["default"],
               "laws": {k: v for k, v in kinds.items()},
               "reference_domain": {"admissible_tuples": len(table), "declared_tuples": report["input_tuples"],
                                    "excluded": "pre-states that break a declared invariant; the contract's genesis "
                                                "and commit laws keep every held state admissible"},
               "correspondence": "tested correspondence, not a theorem",
               "scope": "class, reason, successor control state and variables, and empty deliveries; "
                        "not Step or publication identity equivalence"}
    return {"project.zeno": "\n".join(project) + "\n", "v2/policy.json": json.dumps(policy, indent=2) + "\n",
            "tests/decision-examples.txt": "\n".join(journey) + "\n",
            "tests/zal-authority-examples.txt": "\n".join(table) + "\n",
            "tests/zal_correspondence.rs": CORRESPONDENCE_TEST, "zal-source-map.json": json.dumps(mapping, indent=2) + "\n"}


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
                                   for name in ["behavior.py", "factory.py", "workflow.py",
                                                *(["behavior2.py"] if isinstance(model, behavior2.Machine) else [])]},
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
        report["input_tuples"] = (sum(1 for _ in model.inputs()) if isinstance(model, Model)
                                  else files["tests/zal-authority-examples.txt"].count("\n"))
    if output:
        output.write_text(json.dumps(report, indent=2) + "\n")
    return report
