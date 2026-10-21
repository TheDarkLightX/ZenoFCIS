#!/usr/bin/env python3
"""Independent finite-table reference for representation refinement.

Test oracle only. This does not call Rust, model its Authority, or establish that
an executable implements a table. The reservation model independently specifies
three identities, capacity two and permanent retirement. Contract execution and
these semantic expectations are compared by check_contract_refinement.py.
"""
# SPDX-License-Identifier: MIT OR Apache-2.0
# LLM-generated implementation; checked by the repository's qualification gates.
from dataclasses import dataclass
from itertools import product


@dataclass(frozen=True)
class Observation:
    kind: str
    reason: int | None
    deliveries: tuple = ()


@dataclass(frozen=True)
class Table:
    states: int
    inputs: int
    initial: int
    rows: tuple  # state-major (successor ordinal, Observation)


@dataclass(frozen=True)
class Answer:
    verdict: str
    checked_rows: int
    obligation: str | None = None
    state: int | None = None
    input: int | None = None


def check(abstract, concrete, abstraction, malformed, max_checks=262144):
    """Specification by the set of violated obligations, with a prefix budget.

    Structural validation is a test-input guard, not FCIS intake validation.
    Computing the finite set is intentionally different from the Rust streaming
    implementation. A fault beyond the permitted prefix cannot decide a verdict.
    """
    if type(max_checks) is not int or not 0 <= max_checks <= 262144:
        raise ValueError("comparison budget")
    if (not 1 <= abstract.states <= 4096 or not 1 <= concrete.states <= 4096
            or abstract.inputs != concrete.inputs or not 1 <= abstract.inputs <= 64
            or len(abstraction) != concrete.states
            or any(type(x) is not int or not 0 <= x < abstract.states
                   for x in abstraction if x is not None)):
        raise ValueError("domain or abstraction")
    for table in (abstract, concrete):
        if not 0 <= table.initial < table.states or len(table.rows) != table.states * table.inputs:
            raise ValueError("initial or rows")
        if any(type(s) is not int or not 0 <= s < table.states for s, _ in table.rows):
            raise ValueError("successor")
    if abstraction[concrete.initial] != abstract.initial:
        return Answer("counterexample", 0, "initial-state", concrete.initial)
    if set(abstraction) - {None} != set(range(abstract.states)):
        return Answer("counterexample", 0, "abstract-state-coverage")
    failures = []
    prefix = concrete.rows[:max_checks]
    for ordinal, (post, observed) in enumerate(prefix):
        state, command = divmod(ordinal, concrete.inputs)
        before = abstraction[state]
        if before is None:
            if (post, observed) != (state, malformed):
                failures.append((ordinal, 0, "malformed-state"))
            continue
        expected_post, expected = abstract.rows[before * abstract.inputs + command]
        if observed.kind in ("Reject", "Refused") and post != state:
            failures.append((ordinal, 0, "reject-is-no-op"))
        if expected.kind in ("Reject", "Refused") and expected_post != before:
            failures.append((ordinal, 0, "reject-is-no-op"))
        if abstraction[post] != expected_post:
            failures.append((ordinal, 1, "successor-refinement"))
        if observed != expected:
            failures.append((ordinal, 2, "observation-refinement"))
    if failures:
        ordinal, _, obligation = min(failures)
        state, command = divmod(ordinal, concrete.inputs)
        return Answer("counterexample", ordinal + 1, obligation, state, command)
    return Answer("refines" if len(prefix) == len(concrete.rows) else "inconclusive", len(prefix))


def reservation():
    """Independent mathematical example, not a translation of the Rust checker."""
    states = tuple(s for s in product(range(3), repeat=3) if s.count(1) <= 2)
    malformed = Observation("Refused", 500)

    def accepted(command):
        # ordinal, channel, destination, payload fields, idempotency ordinal
        return Observation("Accept", None, ((0, 300, b"reservation", ((140, command),), 0),))

    abstract_rows = []
    for state in states:
        for command in range(6):
            who, release = command % 3, command >= 3
            successor = list(state)
            if release:
                reason = 203 if state[who] != 1 else None
                if reason is None:
                    successor[who] = 2
            else:
                reason = 200 if state[who] == 1 else 201 if state[who] == 2 else 202 if state.count(1) == 2 else None
                if reason is None:
                    successor[who] = 1
            observation = Observation("Reject", reason) if reason is not None else accepted(command)
            abstract_rows.append((states.index(tuple(successor)), observation))

    concrete_rows, abstraction = [], []
    for raw in range(64):
        active = {i for i in range(3) if raw & (1 << i)}
        seen = {i for i in range(3) if raw & (1 << (i + 3))}
        valid = active <= seen and len(active) <= 2
        view = tuple(1 if i in active else 2 if i in seen else 0 for i in range(3))
        abstraction.append(states.index(view) if valid else None)
        for command in range(6):
            who, release = command % 3, command >= 3
            if not valid:
                concrete_rows.append((raw, malformed))
                continue
            after_active, after_seen = set(active), set(seen)
            if release:
                reason = 203 if who not in active else None
                if reason is None:
                    after_active.remove(who)
            else:
                reason = 200 if who in active else 201 if who in seen else 202 if len(active) == 2 else None
                if reason is None:
                    after_active.add(who)
                    after_seen.add(who)
            successor = sum(1 << i for i in after_active) + sum(1 << (i + 3) for i in after_seen)
            concrete_rows.append((successor, Observation("Reject", reason) if reason is not None else accepted(command)))
    return (Table(len(states), 6, 0, tuple(abstract_rows)), Table(64, 6, 0, tuple(concrete_rows)),
            tuple(abstraction), malformed)


def fixture_table(project, policy):
    """Interpret the literal-row example subset, independently of its generator.

    Supports one integer state field 110, action field 120 and singleton context;
    literal successor/payload expressions; and the single pre-state membership
    law 500. Refuses other rules/laws instead of approximating general FCIS.
    Byte encodings, compiler admission and resource accounting remain Rust gates.
    """
    import re
    limit = re.search(r"type 105 int \w+ in 0\.\.=(\d+);", project)
    if limit is None or not 0 <= int(limit[1]) < 4096:
        raise ValueError("state domain outside fixture subset")
    if ("type 106 int Action in 0..=5;" not in project
            or "type 107 int Unit in 0..=0;" not in project
            or policy["variables"] != {"state": "pre.100.110", "action": "command.101.120"}):
        raise ValueError("input layout outside fixture subset")
    count = int(limit[1]) + 1
    admitted = set(range(count))
    laws = [line for line in project.splitlines() if line.startswith("law ")]
    bounds = f"law 501 state_bounds on commit, genesis = post.100.110 >= 0 && post.100.110 <= {count - 1};"
    if bounds not in laws:
        raise ValueError("missing fixture state invariant")
    laws.remove(bounds)
    if laws:
        if len(laws) != 1 or not laws[0].startswith("law 500 valid_representation on any = "):
            raise ValueError("law outside fixture subset")
        formula = laws[0].split(" = ", 1)[1].removesuffix(";")
        if formula != "true":
            terms = formula.split(" || ")
            if any(re.fullmatch(r"pre\.100\.110 == \d+", term) is None for term in terms):
                raise ValueError("law formula outside fixture subset")
            admitted = {int(term.rsplit(" ", 1)[1]) for term in terms}
    rows = []
    for state, command in product(range(count), range(6)):
        selected = None
        for rule in policy["cases"]:
            condition = rule["when"]
            match = re.fullmatch(r"state == (\d+) && action == (\d+)", condition)
            if condition != "true" and match is None:
                raise ValueError("guard outside fixture subset")
            if condition == "true" or (int(match[1]), int(match[2])) == (state, command):
                selected = rule
                break
        if selected is None:
            raise ValueError("non-total case list")
        kind, reason = selected["class"], selected["reason"]
        if kind not in ("Accept", "Reject", "CommittedFailure"):
            raise ValueError("unknown class")
        if kind == "Reject":
            if selected["post"] or selected["outbox"]:
                raise ValueError("reject cannot carry a successor or delivery")
            successor = state
        else:
            if set(selected["post"]) != {"110"}:
                raise ValueError("post outside fixture subset")
            successor = int(selected["post"]["110"])
        deliveries = tuple((event["ordinal"], event["channel"], event["destination"].encode("ascii"),
                            tuple((int(k), int(v)) for k, v in sorted(event["payload"].items())),
                            event["idempotency_ordinal"]) for event in selected["outbox"])
        if not 0 <= successor < count:
            raise ValueError("post outside fixture domain")
        rows.append((state, Observation("Refused", 500)) if state not in admitted else
                    (successor, Observation(kind, reason, deliveries)))
    return Table(count, 6, int(policy["genesis"]["110"]), tuple(rows))
