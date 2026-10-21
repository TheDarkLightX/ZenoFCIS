#!/usr/bin/env python3
"""Check the contract judges against an independent reference, with fault controls.

The subjects are the 77 declared instances of the ten core-component
families, regenerated from `core-components/` with tools/instantiate_core.py,
and four variants of the bounded counter with a committed failure, a law the
case table breaks, and a law that determines some decisions. For each, this
runs `contract review --gaps`, `contract profile`, `contract lemma`,
`contract scenario` and `contract probe` through a built CLI and compares
what the separately written reference tools/contract_judges_reference.py also
computes: the profile's counts, robustness and per-tuple and per-state table
digests; the gap judge's admissible counts and freedom on both domains, law
activation, per-tuple digest and reject-everything verdict; every lemma's
verdict, counts, per-state digest, counterexample and trace; scenario
verdicts and whether one kills the reject-everything core; and the probes
`decisions`, `depends` and `explain`'s case. The reference does not compute
mutant classifications, the measures repeated with proposed laws, or the
probes `count`, `implies` and `reachable`, so those are not compared. Both
sides must also equal the recorded known answers.

The 77 complete core instances must refuse the reject-everything candidate.
The failure and refusal variants retain their original partial laws and must
admit that candidate; a scenario must kill it in both groups. The variants
are diagnostic contracts and carry no core-family certificate.

`--controls` adds fault controls, each routed through the check it targets:
every compared field, perturbed in the CLI's measured answers, must make
`compare()` report it; every recorded field, perturbed, must make the
recorded-answer check report it; contracts with a single-tuple fault planted
at the first, last and a seeded random ordinal, run through the CLI, must
disagree with the original's answers; and CLI runs on a fault on a scenario's
path, a false proposed law, malformed proposals, a failing scenario and
limits must give the expected verdicts. `--self-test` shows, without a CLI,
that disabling either comparison makes the controls fail; with `--cli` it
also runs the whole check with `compare()` disabled and requires it to fail.

A pass is bounded evidence about these subjects and this CLI build, not a
proof about other contracts.
"""
from __future__ import annotations

import argparse
import concurrent.futures
import copy
import json
import random
import subprocess
import sys
import tempfile
from pathlib import Path

import contract_judges_reference as reference
import instantiate_core as core

KNOWN = core.ROOT / "tools/test_data/contract_judges/known-answers.json"
KNOWN_SCHEMA = "zeno-fcis/contract-judges-known-answers/2"
SEED = 20261012
# The complete C=8 idempotency instance requires 153684864 declared law checks.
# This qualification budget preserves the CLI default and all low-limit controls.
CORE_GAP_WORK = 160_000_000


class Mismatch(AssertionError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise Mismatch(message)


# --- subjects ----------------------------------------------------------------------------------

def core_files(family, parameters, examples=True) -> dict[str, str]:
    files = core.source(family, parameters, include_proof=False)
    wanted = ["project.zeno", "v2/policy.json"] + (["tests/decision-examples.txt"] if examples else [])
    return {name: files[name].decode() for name in wanted}


def counter_variant(capacity: int, failure: bool, refusal: bool, determining: bool):
    """A bounded counter with a committed failure, a law the table breaks, a determining law."""
    files = core_files("bounded-counter", {"C": capacity}, examples=False)
    policy = json.loads(files["v2/policy.json"])
    # These controls exercise the original partial contracts. Complete-family
    # laws would forbid the planted failure or refer to its former Reject code.
    partial, marker, _ = files["project.zeno"].partition("// Independent complete relation")
    require(bool(marker), "counter template is missing its complete-relation boundary")
    files["project.zeno"] = partial
    policy["law_kinds"] = {key: value for key, value in policy["law_kinds"].items()
                           if int(key) < 520}
    laws = []
    if failure:
        # Dec at zero commits a failure that keeps the state, instead of rejecting.
        case = next(case for case in policy["cases"] if case["when"] == "action == 151 && value == 0")
        case["class"] = "CommittedFailure"
        laws.append((503, "failure_keeps_state on failure = post.100.110 == pre.100.110",
                     "CommittedFailureEffects"))
    if refusal:
        # The case table accepts Inc up to the capacity; this law refuses the last step.
        laws.append((504, f"short_of_capacity on accept = post.100.110 <= {capacity - 1}",
                     "DebitCreditEffectEquality"))
    if determining:
        laws.append((505, "rejects_only_when_needed on reject = context.102.130 == 0 || "
                     f"(command.101.120 == 150 && pre.100.110 == {capacity}) || "
                     "(command.101.120 == 151 && pre.100.110 == 0)", "AuthoritySubjectRecipient"))
    for identifier, text, kind in laws:
        files["project.zeno"] += f"law {identifier} {text};\n"
        policy["law_kinds"][str(identifier)] = kind
    files["v2/policy.json"] = json.dumps(policy, indent=2) + "\n"
    return files


VARIANTS = {
    "variant-bounded-counter-c2-failure": lambda: counter_variant(2, True, False, False),
    "variant-bounded-counter-c2-refusal": lambda: counter_variant(2, False, True, False),
    "variant-bounded-counter-c2-determining": lambda: counter_variant(2, False, False, True),
    "variant-bounded-counter-c3-all": lambda: counter_variant(3, True, True, True),
}


def subjects(subset: str) -> list[tuple[str, callable]]:
    found = []
    for family in core.FAMILIES:
        for index, parameters in enumerate(core.definition(family)["instances"]):
            if subset == "controls" and index > 0:
                continue
            name = f"{family}-{core.instance_id(parameters)}"
            found.append((name, lambda family=family, parameters=parameters:
                          core_files(family, parameters)))
    found += list(VARIANTS.items())
    return found


def control_subjects() -> list[str]:
    return [f"{family}-{core.instance_id(core.definition(family)['instances'][0])}"
            for family in core.FAMILIES] + list(VARIANTS)


def write(directory: Path, files: dict[str, str]) -> None:
    for relative, text in files.items():
        target = directory / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)


def load(directory: Path) -> tuple[reference.Contract, reference.Machine]:
    contract = reference.Contract((directory / "project.zeno").read_text(),
                                  json.loads((directory / "v2" / "policy.json").read_text()))
    return contract, reference.Machine(contract)


# --- lemmas and scenarios --------------------------------------------------------------------

def lemmas(contract: reference.Contract, machine: reference.Machine) -> list[str]:
    """The state laws together; genesis alone; `true`; genesis excluded; and, where one
    exists, the state laws without an unreachable state that another unreachable state
    they admit commits into, which holds on every reachable state but is not inductive."""
    state_laws = " && ".join(f"({law['text']})" for law in contract.state_laws())
    paths = [position[1].replace("pre.", "post.", 1) for position in contract.positions[:contract.width]]
    equal = lambda state: " && ".join(f"{path} == {value}" for path, value in zip(paths, state))
    found = [state_laws, equal(contract.genesis), "true", f"!({equal(contract.genesis)})"]
    parent = machine.reachable()
    for state in contract.states:
        if state in parent or not contract.valid(state):
            continue
        for x in contract.inputs:
            successor = machine.after(state, x)
            if machine.commits(state, x) and successor != state and successor not in parent:
                return found + [f"({state_laws}) && !({equal(successor)})"]
    return found


def scenario_text(contract: reference.Contract, machine: reference.Machine) -> str:
    """A run from genesis to the farthest reachable state, and one rejected step."""
    parent = machine.reachable()
    farthest = max(parent, key=lambda state: (len(machine.trace(parent, state)),
                                              -contract.state_index[state]))
    lines = ["# Generated from the reference's decisions.", "# scenario farthest"]
    state = contract.genesis
    for step in machine.trace(parent, farthest):
        decision = machine.table[(state, tuple(step))]
        lines.append(step_line(state, tuple(step), decision))
        state = decision[2]
    for x in contract.inputs:
        decision = machine.table[(contract.genesis, x)]
        if decision is not None and decision[0] == "Reject":
            lines += ["# scenario first-reject", step_line(contract.genesis, x, decision)]
            break
    return "\n".join(lines) + "\n"


def step_line(state, command_context, decision) -> str:
    verdict = {"Accept": "accept", "Reject": "reject", "CommittedFailure": "failure"}[decision[0]]
    reason = "-" if decision[1] is None else str(decision[1])
    numbers = lambda values: " ".join(map(str, values))
    return (f"{numbers(state)} | {numbers(command_context)} | {verdict} {reason} "
            f"{numbers(decision[2])} | -")


# --- the reference's answers -------------------------------------------------------------------

def reference_answers(directory: Path) -> dict:
    contract, machine = load(directory)
    texts = lemmas(contract, machine)
    scenario = scenario_text(contract, machine)
    (directory / "tests").mkdir(exist_ok=True)
    (directory / "tests" / "decision-scenarios.txt").write_text(scenario)
    answers = reference.answers(directory, texts, scenario)
    answers["lemma_formulas"] = texts
    answers["positions"] = [position[1] for position in contract.positions]
    answers["first_input"] = list(contract.states[0]) + list(contract.inputs[0])
    # `contract review` exits 1 for a law refusal on a pre-state that satisfies every
    # state law: a finding.
    findings = any(machine.refusals[(state, x)][0] == "law" and contract.valid(state)
                   for state in contract.states for x in contract.inputs
                   if machine.table[(state, x)] is None)
    answers["review_exit"] = 1 if findings else 0
    return answers


# --- the CLI's answers ---------------------------------------------------------------------------

def cli(binary: Path, arguments: list[str], expected: int | None, log: Path) -> dict | None:
    with log.open("ab") as stream:
        stream.write(f"$ zeno-fcis {' '.join(arguments)}\n".encode())
        result = subprocess.run([str(binary), *arguments], stdout=subprocess.PIPE,
                                stderr=stream, check=False)
        stream.write(f"exit {result.returncode}\n".encode())
    if expected is not None:
        require(result.returncode == expected,
                f"`zeno-fcis {' '.join(arguments)}` exited {result.returncode}, expected {expected}")
    return json.loads(result.stdout) if result.stdout.strip() else None


def cli_answers(binary: Path, directory: Path, known: dict, log: Path, strict: bool = True) -> dict:
    """Every compared answer from the CLI; with `strict`, each exit is the one `known` implies."""
    exit_of = lambda code: code if strict else None
    app = str(directory)
    review = cli(binary, ["contract", "review", app, "--gaps", "--max-work", str(CORE_GAP_WORK)],
                 exit_of(known["review_exit"]), log)
    require(isinstance(review, dict) and review.get("schema") == "zeno-fcis/contract-review/4",
            "review packet is not version 4")
    require(review["integer_encoding"] == "safe-number-or-decimal-string/1",
            "review packet has an unsupported integer encoding")
    profile = cli(binary, ["contract", "profile", app], exit_of(0), log)
    arguments = ["contract", "lemma", app]
    for formula in known["lemma_formulas"]:
        arguments += ["--lemma", formula]
    verdicts = [item["verdict"] for item in known["lemmas"]]
    lemma = cli(binary, arguments, exit_of(1 if "false" in verdicts else
                                           2 if "not-inductive" in verdicts else 0), log)
    scenario = cli(binary, ["contract", "scenario", app], exit_of(0), log)
    decisions = cli(binary, ["contract", "probe", "decisions", app], exit_of(0), log)
    depends = {}
    for position in known["positions"]:
        vacuous = known["probes"]["depends"].get(position) == "vacuous"
        answer = cli(binary, ["contract", "probe", "depends", app, "--position", position],
                     exit_of(2 if vacuous else 0), log)
        depends[position] = answer["result"]["answer"]
    first = " ".join(map(str, known["first_input"]))
    explain = cli(binary, ["contract", "probe", "explain", app, "--input", first], exit_of(0), log)
    return {"review": review, "profile": profile, "lemma": lemma, "scenario": scenario,
            "decisions": decisions, "depends": depends, "explain": explain}


def synthesized(known: dict) -> dict:
    """The CLI-shaped answers `compare()` reads, built from `known`: for the self-test."""
    gaps = known["gaps"]
    killed = known["scenario"]["reject_everything_killed"]
    return {
        "profile": {**{key: copy.deepcopy(known["profile"][key])
                       for key in ("states", "memory_bits", "state_laws", "robustness", "tables")}},
        "review": {"gaps": {
            "admissible": copy.deepcopy(gaps["admissible"]),
            "activation": copy.deepcopy(gaps["activation"]),
            "tables": {"admissible_sha256": gaps["admissible_sha256"]},
            "reject_everything": copy.deepcopy(gaps["reject_everything"] or {}),
        }},
        "lemma": {"lemmas": copy.deepcopy(known["lemmas"])},
        "scenario": {"scenarios": copy.deepcopy(known["scenario"]["scenarios"]),
                     "reject_everything": {"status": "withheld"} if killed == "withheld"
                     else {"killed": killed}},
        "decisions": {"result": {"answer": copy.deepcopy(known["probes"]["decisions"])}},
        "depends": copy.deepcopy(known["probes"]["depends"]),
        "explain": {"result": {"case": known["probes"]["explain_first_input_case"]}},
    }


# --- comparison ------------------------------------------------------------------------------------

def compare(name: str, known: dict, measured: dict) -> list[str]:
    """Every field the reference computes, against the CLI's answers."""
    problems = []

    def same(label, expected, actual):
        if expected != actual:
            problems.append(f"{name}: {label}: reference {expected!r}, CLI {actual!r}")

    profile = measured["profile"]
    for key in ("states", "memory_bits", "state_laws", "robustness"):
        same(f"profile {key}", known["profile"][key], profile[key])
    for key in ("decisions_sha256", "states_sha256"):
        same(f"profile {key}", known["profile"]["tables"][key], profile["tables"][key])
    gaps = measured["review"]["gaps"]
    for domain in ("full", "valid"):
        mine = gaps["admissible"][domain]
        same(f"gaps {domain}", known["gaps"]["admissible"][domain],
             {key: mine[key] for key in ("tuples", "reference_refused", "determined", "freedom")})
    same("gaps activation", known["gaps"]["activation"], gaps["activation"])
    same("gaps admissible_sha256", known["gaps"]["admissible_sha256"],
         gaps["tables"]["admissible_sha256"])
    vacuity = gaps["reject_everything"]
    expected = known["gaps"]["reject_everything"]
    same("gaps reject-everything", expected,
         None if expected is None else
         {key: vacuity.get(key) for key in ("reject_reason", "refused_where_the_contract_decides",
                                            "admitted_where_the_contract_decides")})
    for index, (expected, actual) in enumerate(zip(known["lemmas"], measured["lemma"]["lemmas"])):
        for key in sorted((set(expected) | set(actual)) - {"reason"}):
            same(f"lemma {index + 1} {key}", expected.get(key), actual.get(key))
    same("lemma count", len(known["lemmas"]), len(measured["lemma"]["lemmas"]))
    scenario = measured["scenario"]
    same("scenarios", known["scenario"]["scenarios"],
         [{key: item[key] for key in ("name", "steps", "start", "verdict")}
          for item in scenario["scenarios"]])
    core_section = scenario["reject_everything"]
    same("scenario reject-everything killed", known["scenario"]["reject_everything_killed"],
         "withheld" if core_section.get("status") == "withheld" else core_section.get("killed"))
    same("probe decisions", known["probes"]["decisions"], measured["decisions"]["result"]["answer"])
    same("probe depends", known["probes"]["depends"], measured["depends"])
    same("probe explain case", known["probes"]["explain_first_input_case"],
         measured["explain"]["result"]["case"])
    return problems


def differences(expected, actual, path=()) -> list[tuple]:
    """Paths where two JSON values differ."""
    if isinstance(expected, dict) and isinstance(actual, dict):
        found = []
        for key in sorted(set(expected) | set(actual), key=str):
            if key not in expected or key not in actual:
                found.append(path + (key,))
            else:
                found += differences(expected[key], actual[key], path + (key,))
        return found
    if isinstance(expected, list) and isinstance(actual, list) and len(expected) == len(actual):
        return [found for index, (a, b) in enumerate(zip(expected, actual))
                for found in differences(a, b, path + (index,))]
    return [] if expected == actual else [path]


def check_recorded(name: str, recorded: dict | None, known: dict) -> list[str]:
    """The reference's answers against the recorded known answers, field by field."""
    if recorded is None:
        return [f"{name}: no recorded known answers"]
    return [f"{name}: recorded {'.'.join(map(str, path))} differs"
            for path in differences(recorded, known)]


def check_subject(binary: Path, work: Path, subject, recorded: dict | None):
    name, files = subject
    directory = work / "subjects" / name
    write(directory, files())
    known = reference_answers(directory)
    problems = [] if recorded is None else check_recorded(name, recorded.get(name), known)
    log = work / "logs" / f"{name}.log"
    try:
        measured = cli_answers(binary, directory, known, log)
    except Mismatch as error:
        return name, known, None, [f"{name}: {error}"]
    return name, known, measured, problems + compare(name, known, measured)


# --- controls --------------------------------------------------------------------------------------

def positions(count: int, rng: random.Random) -> dict[str, int]:
    return {"first": 0, "last": count - 1, "random": rng.randrange(count)}


def planted_digest(lines: list[str], ordinal: int, fault) -> str:
    planted = list(lines)
    planted[ordinal] = fault(planted[ordinal])
    return reference.digest(planted)


def tables(directory: Path) -> dict[str, list[str]]:
    """The per-tuple and per-state tables the compared digests cover, from the reference."""
    contract, machine = load(directory)
    parent = machine.reachable()
    reachable = [state for state in contract.states if state in parent]
    valid = {state: contract.valid(state) for state in contract.states}
    return {"decisions": reference.decision_lines(contract, machine),
            "states": reference.state_lines(contract, valid, parent,
                                            reference.moore_classes(machine, reachable)),
            "admissible": reference.gaps(contract, machine)["admissible_lines"]}


def bump(value):
    """A different value of the same kind."""
    if isinstance(value, bool):
        return not value
    if isinstance(value, int):
        return value + 1
    if isinstance(value, str):
        return value + "-planted"
    if isinstance(value, list):
        return value + [999]
    if isinstance(value, dict):
        return {**value, "planted": 1}
    return "planted"


def field_faults(known: dict, lines: dict[str, list[str]], rng: random.Random):
    """(label compare() must report, a perturbation of the CLI's measured answers)."""
    def at(*path):
        def perturb(measured):
            target = measured
            for key in path[:-1]:
                target = target[key]
            target[path[-1]] = bump(target.get(path[-1]) if isinstance(target, dict)
                                    else target[path[-1]])
        return perturb

    def digest(path, table, ordinal, fault):
        def perturb(measured):
            target = measured
            for key in path[:-1]:
                target = target[key]
            target[path[-1]] = planted_digest(lines[table], ordinal, fault)
        return perturb

    first_law = sorted(known["gaps"]["activation"]["full"])[0]
    first_position = known["positions"][0]
    first_decision = sorted(known["probes"]["decisions"])[0]
    faults = [
        ("profile states", at("profile", "states", "reachable")),
        ("profile memory_bits", at("profile", "memory_bits", "declared")),
        ("profile state_laws", at("profile", "state_laws")),
        ("profile robustness", at("profile", "robustness", "refused_tuples", "all")),
        ("gaps full", at("review", "gaps", "admissible", "full", "determined")),
        ("gaps valid", at("review", "gaps", "admissible", "valid", "tuples")),
        ("gaps activation", at("review", "gaps", "activation", "full", first_law)),
        ("gaps reject-everything",
         at("review", "gaps", "reject_everything", "admitted_where_the_contract_decides")),
        ("lemma 1 verdict", at("lemma", "lemmas", 0, "verdict")),
        ("lemma 1 holds_sha256", at("lemma", "lemmas", 0, "holds_sha256")),
        ("lemma 2 trace_from_genesis", at("lemma", "lemmas", 1, "trace_from_genesis")),
        ("scenarios", at("scenario", "scenarios", 0, "verdict")),
        ("scenario reject-everything killed", at("scenario", "reject_everything", "killed")),
        ("probe decisions", at("decisions", "result", "answer", first_decision)),
        ("probe depends", at("depends", first_position)),
        ("probe explain case", at("explain", "result", "case")),
    ]
    tables_of = (("profile decisions_sha256", ("profile", "tables", "decisions_sha256"), "decisions",
                  lambda line: "X" if line != "X" else "A - 0"),
                 ("profile states_sha256", ("profile", "tables", "states_sha256"), "states",
                  lambda line: ("0" if line[0] == "1" else "1") + line[1:]),
                 ("gaps admissible_sha256", ("review", "gaps", "tables", "admissible_sha256"),
                  "admissible", lambda line: "-" if line != "-" else "1"))
    for label, path, table, fault in tables_of:
        for place, ordinal in positions(len(lines[table]), rng).items():
            faults.append((label, digest(path, table, ordinal, fault), place))
    return [fault if len(fault) == 3 else (*fault, None) for fault in faults]


def recorded_paths(known: dict) -> list[tuple]:
    """One recorded field per compared family."""
    first_law = sorted(known["gaps"]["activation"]["full"])[0]
    return [("profile", "states", "reachable"), ("profile", "memory_bits", "declared"),
            ("profile", "robustness", "refused_tuples", "all"),
            ("profile", "tables", "decisions_sha256"), ("profile", "tables", "states_sha256"),
            ("gaps", "admissible", "full", "determined"), ("gaps", "admissible", "valid", "freedom"),
            ("gaps", "activation", "full", first_law), ("gaps", "admissible_sha256"),
            ("lemmas", 0, "verdict"), ("lemmas", 1, "holds_sha256"),
            ("scenario", "scenarios", 0, "verdict"), ("scenario", "reject_everything_killed"),
            ("probes", "decisions"), ("probes", "depends"), ("probes", "explain_first_input_case"),
            ("lemma_formulas",), ("review_exit",)]


def comparison_controls(work: Path, answers: dict, measured: dict, recorded: dict) -> list[dict]:
    """Perturbed CLI answers through compare(), perturbed records through check_recorded()."""
    rng = random.Random(SEED)
    controls = []
    for name in control_subjects():
        known = answers[name]
        lines = tables(work / "subjects" / name)
        for label, perturb, place in field_faults(known, lines, rng):
            planted = copy.deepcopy(measured[name])
            perturb(planted)
            problems = compare(name, known, planted)
            controls.append({"control": "a perturbed CLI answer is reported by compare()",
                             "instance": name, "field": label, "place": place,
                             "caught": any(f"{name}: {label}:" in problem for problem in problems)})
        for path in recorded_paths(known):
            planted = copy.deepcopy(recorded[name])
            target = planted
            for key in path[:-1]:
                target = target[key]
            target[path[-1]] = bump(target[path[-1]])
            problems = check_recorded(name, planted, known)
            dotted = ".".join(map(str, path))
            controls.append({"control": "a perturbed recorded answer is reported", "instance": name,
                             "field": dotted,
                             "caught": any(f"recorded {dotted}" in problem for problem in problems)})
    return controls


def planted_contract(directory: Path, target: Path, state, command_context, decision, contract):
    """`directory`'s contract with a first case that decides one tuple differently."""
    files = {"project.zeno": (directory / "project.zeno").read_text()}
    policy = json.loads((directory / "v2" / "policy.json").read_text())
    guard = " && ".join(f"{position[1]} == {value}" for position, value
                        in zip(contract.positions, list(state) + list(command_context)))
    rejects = sorted({case["reason"] for case in contract.cases if case["class"] == "Reject"})
    current = decision[1] if decision is not None and decision[0] == "Reject" else None
    reason = next(reason for reason in rejects if reason != current)
    policy["cases"].insert(0, {"when": guard, "class": "Reject", "reason": reason, "post": {},
                               "outbox": []})
    files["v2/policy.json"] = json.dumps(policy, indent=2) + "\n"
    files["tests/decision-scenarios.txt"] = (directory / "tests" / "decision-scenarios.txt").read_text()
    write(target, files)


def contract_controls(binary: Path, work: Path, answers: dict) -> list[dict]:
    """CLI runs on planted contracts, compared with the original's answers, and verdict checks."""
    rng = random.Random(SEED + 1)
    controls = []
    log = work / "logs" / "controls.log"
    for family in core.FAMILIES:
        name = f"{family}-{core.instance_id(core.definition(family)['instances'][0])}"
        directory = work / "subjects" / name
        contract, machine = load(directory)
        ordinals = [(state, x) for state in contract.states for x in contract.inputs]
        for place, ordinal in positions(len(ordinals), rng).items():
            state, x = ordinals[ordinal]
            target = work / "planted" / f"{name}-{place}"
            planted_contract(directory, target, state, x, machine.table[(state, x)], contract)
            measured = cli_answers(binary, target, answers[name], log, strict=False)
            problems = compare(name, answers[name], measured)
            controls.append({"control": "a contract with a planted single-tuple fault disagrees",
                             "instance": name, "place": place, "ordinal": ordinal,
                             "fields": sorted({problem.split(": ")[1] for problem in problems}),
                             "caught": any(": profile decisions_sha256:" in problem
                                           for problem in problems)})
        # A fault on the scenario's path: the scenario fails at that step, and kills are withheld.
        scenario = (directory / "tests" / "decision-scenarios.txt").read_text()
        steps = reference.scenario_steps(scenario, contract.width)[0]["steps"]
        state, x, _ = steps[-1]
        target = work / "planted" / f"{name}-path"
        planted_contract(directory, target, state, x, machine.table[(state, x)], contract)
        packet = cli(binary, ["contract", "scenario", str(target)], 1, log)
        faulted = load(target)
        reference_failed = reference.replay(faulted[1],
                                            reference.scenario_steps(scenario, faulted[0].width)[0])
        controls.append({"control": "a scenario catches a fault on its path, and kills are withheld",
                         "instance": name, "step": len(steps),
                         "caught": packet["scenarios"][0].get("failure", {}).get("step") == len(steps)
                         and reference_failed == len(steps)
                         and packet["mutants"].get("status") == "withheld"
                         and packet["reject_everything"].get("status") == "withheld"})
        laws = work / "planted" / f"{name}.laws"
        laws.write_text("law 65001 planted_false on any = false;\n")
        packet = cli(binary, ["contract", "review", str(directory), "--gaps", "--gaps-laws",
                              str(laws)], 1, log)
        controls.append({"control": "a false proposed law is a false alarm", "instance": name,
                         "caught": packet["gaps"]["status"] == "proposal-false-alarm"})
    app = str(work / "subjects" / "bounded-counter-c1")
    laws = work / "planted" / "true.laws"
    laws.write_text("law 65001 a on any = true;\nlaw 65002 b on any = true;\n")
    bad = work / "planted" / "malformed.laws"
    bad.write_text("law 65001 a on any = true; law 65002 b on any = true;\n")
    for arguments, code, status in (
            (["contract", "profile", app, "--max-tuples", "1"], 2, "inconclusive"),
            (["contract", "lemma", app, "--lemma", "true", "--max-tuples", "1"], 2, "inconclusive"),
            (["contract", "probe", "decisions", app, "--max-tuples", "1"], 2, "inconclusive"),
            (["contract", "review", app, "--gaps", "--max-tuples", "1"], 2, "inconclusive"),
            (["contract", "review", app, "--gaps", "--max-work", "1"], 2, "inconclusive"),
            (["contract", "review", app, "--gaps", "--gaps-laws", str(bad)], 65, "invalid-proposals"),
            (["contract", "lemma", app, "--lemma", "pre.100.110 == 0"], 65, None),
            (["contract", "probe", "implies", app, "--if", "false", "--then", "true"], 2, "vacuous")):
        packet = cli(binary, arguments, code, log)
        found = None if packet is None else (packet["gaps"]["status"] if arguments[1] == "review"
                                             else packet["status"])
        controls.append({"control": "a limit or a malformed input gets its own verdict",
                         "arguments": arguments[1:2] + arguments[3:], "exit": code,
                         "caught": found == status})
    # The repeat measure with proposals has its own work: just over the declared work, it
    # is inconclusive.
    measured = cli(binary, ["contract", "review", app, "--gaps"], 0, log)
    declared_work = measured["gaps"]["work"]
    packet = cli(binary, ["contract", "review", app, "--gaps", "--gaps-laws", str(laws),
                          "--max-work", declared_work], 2, log)
    controls.append({"control": "proposed laws count against --max-work", "instance": "bounded-counter-c1",
                     "caught": packet["gaps"]["status"] == "inconclusive"
                     and packet["gaps"]["reason"] == "work-with-proposals-over-limit"})
    return controls


# --- self-test -------------------------------------------------------------------------------------

def self_test(binary: Path | None) -> int:
    """Disabling either comparison must make the controls fail."""
    global compare, check_recorded
    with tempfile.TemporaryDirectory(prefix="zeno-fcis-judges-self-test-") as raw:
        work = Path(raw)
        answers, measured = {}, {}
        for name, files in subjects("controls"):
            directory = work / "subjects" / name
            write(directory, files())
            answers[name] = reference_answers(directory)
            measured[name] = synthesized(answers[name])
            require(compare(name, answers[name], measured[name]) == [],
                    f"{name}: synthesized answers disagree")
        controls = comparison_controls(work, answers, measured, answers)
        require(all(control["caught"] for control in controls),
                f"uncaught: {[c for c in controls if not c['caught']][:3]}")
        real_compare, real_recorded = compare, check_recorded
        try:
            compare = lambda name, known, measured: []
            check_recorded = lambda name, recorded, known: []
            disabled = comparison_controls(work, answers, measured, answers)
        finally:
            compare, check_recorded = real_compare, real_recorded
        require(not any(control["caught"] for control in disabled),
                "a control was caught with both comparisons disabled")
        print(f"self-test: {len(controls)} comparison controls caught; with compare() and the "
              f"recorded-answer check disabled, {len(disabled)} of {len(disabled)} fail")
        if binary is not None:
            shim = ("import sys; sys.path.insert(0, 'tools'); import check_contract_judges as c; "
                    "c.compare = lambda name, known, measured: []; "
                    f"sys.argv = ['check', '--cli', {str(binary)!r}, '--controls', '--subset', "
                    f"'controls', '--work', {str(work / 'shim')!r}]; raise SystemExit(c.main())")
            result = subprocess.run([sys.executable, "-c", shim], cwd=core.ROOT,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                                    check=False)
            require(result.returncode == 1,
                    f"the check passed with compare() disabled (exit {result.returncode})")
            print("self-test: the whole check fails with compare() disabled: "
                  + result.stdout.strip().splitlines()[-1])
    return 0


# --- main ------------------------------------------------------------------------------------------

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--cli", type=Path)
    parser.add_argument("--work", type=Path, help="keep subjects and logs here; default: a removed temporary folder")
    parser.add_argument("--jobs", type=int, default=2)
    parser.add_argument("--subset", choices=("all", "controls"), default="all",
                        help="every subject, or only those the controls use")
    parser.add_argument("--controls", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--report", type=Path)
    parser.add_argument("--write-known-answers", action="store_true",
                        help="record the reference's answers; refused unless the CLI agrees")
    arguments = parser.parse_args()
    try:
        if arguments.self_test:
            return self_test(arguments.cli)
        require(arguments.cli is not None, "--cli is required")
        if arguments.work is not None:
            return check(arguments, arguments.work)
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-judges-") as work:
            return check(arguments, Path(work))
    except Mismatch as error:
        print(f"contract judges: FAIL: {error}", file=sys.stderr)
        return 1


def check(arguments, work: Path) -> int:
    (work / "logs").mkdir(parents=True, exist_ok=True)
    recorded = None
    if not arguments.write_known_answers:
        document = json.loads(KNOWN.read_text())
        require(document["schema"] == KNOWN_SCHEMA, "known answers schema")
        recorded = document["instances"]
    items = subjects(arguments.subset)
    require(arguments.subset != "all" or len(items) == 77 + len(VARIANTS),
            "the declared instance range drifted")
    answers, results, problems = {}, {}, []
    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, arguments.jobs)) as pool:
        for name, known, measured, found in pool.map(
                lambda item: check_subject(arguments.cli, work, item, recorded), items):
            answers[name], results[name] = known, measured
            problems += found
            print(f"{'PASS' if not found else 'FAIL'} {name}", flush=True)
    vacuity = {name: (results[name] is not None
                      and results[name]["review"]["gaps"]["reject_everything"].get(
                          "admitted_where_the_contract_decides") is False
                      and results[name]["review"]["gaps"]["reject_everything"].get(
                          "refused_where_the_contract_decides", 0) > 0
                      and results[name]["scenario"]["reject_everything"].get("killed") is True)
               for name in answers if not name.startswith("variant-")}
    problems += [f"{name}: vacuity control failed" for name, held in vacuity.items() if not held]
    partial_vacuity = {name: (results[name] is not None
                             and results[name]["review"]["gaps"]["reject_everything"].get(
                                 "admitted_where_the_contract_decides") is True
                             and results[name]["scenario"]["reject_everything"].get("killed") is True)
                      for name in ("variant-bounded-counter-c2-failure",
                                   "variant-bounded-counter-c2-refusal")}
    problems += [f"{name}: partial-contract vacuity control failed"
                 for name, held in partial_vacuity.items() if not held]
    report = {"subjects": len(answers), "agreeing": sum(
        1 for name in answers if results[name] and not any(p.startswith(name + ":") for p in problems)),
        "vacuity_controls_held": sum(vacuity.values()), "vacuity_subjects": len(vacuity),
        "partial_vacuity_controls_held": sum(partial_vacuity.values()),
        "partial_vacuity_subjects": len(partial_vacuity),
        "problems": problems}
    if arguments.controls and all(results.values()):
        controls = comparison_controls(work, answers, results, recorded or answers)
        controls += contract_controls(arguments.cli, work, answers)
        report["controls"] = controls
        report["controls_caught"] = sum(control["caught"] for control in controls)
        problems += [f"control not caught: {control}" for control in controls if not control["caught"]]
    if arguments.write_known_answers and not problems:
        KNOWN.parent.mkdir(parents=True, exist_ok=True)
        KNOWN.write_text(json.dumps({
            "schema": KNOWN_SCHEMA, "authority": "none",
            "provenance": "Answers of tools/contract_judges_reference.py on the 77 core-component "
                          "instances and four bounded-counter variants, recorded where every field "
                          "compare() reads agreed with the CLI",
            "instances": answers}, indent=1, sort_keys=True) + "\n")
    if arguments.report:
        arguments.report.write_text(json.dumps({**report, "answers": answers}, indent=1,
                                               sort_keys=True) + "\n")
    for problem in problems:
        print(problem, file=sys.stderr)
    summary = (f"contract judges: {report['agreeing']}/{report['subjects']} subjects agree on the "
               f"compared fields; vacuity held on {report['vacuity_controls_held']}/"
               f"{report['vacuity_subjects']} complete core instances and "
               f"{report['partial_vacuity_controls_held']}/{report['partial_vacuity_subjects']} partial controls")
    if "controls" in report:
        summary += f"; controls caught {report['controls_caught']}/{len(report['controls'])}"
    print(summary)
    return 1 if problems else 0


if __name__ == "__main__":
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        raise SystemExit(main())
    run_cli(_entrypoint)
