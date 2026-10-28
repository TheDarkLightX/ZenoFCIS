#!/usr/bin/env python3
"""Finite paired-execution observation checks for an existing ZAL v2 machine.

The pure check owns its queues and counters. It neither mutates the machine nor
calls a native Authority. A complete reachable product establishes only the
declared synchronous model relation, not a property of a deployed program.
"""
from __future__ import annotations

import argparse
from collections import deque
from dataclasses import dataclass
import hashlib
import itertools
import json
from pathlib import Path
import sys

import behavior2 as b


SCHEMA = "zal/relational-observation/1"
REPORT_SCHEMA = "zal/relational-observation-report/1"
MAX_STATES = 4096
MAX_EDGES = 100000
MAX_WORK = 1000000
MAX_ROOTS = 64
MAX_PROFILE_BYTES = 65536
MAX_REPORT_BYTES = 4 * 1024 * 1024
SCOPE = ("All finite synchronized histories of this ZAL v2 transition model, "
         "from the declared initial secret alternatives; common events and "
         "equal public context, independently varying secret context. "
         "Only the declared observations are compared. No native implementation, "
         "liveness, physical delivery, timing or resource-cost guarantee.")
SOURCES = ("relational_observation.py", "behavior.py", "behavior2.py",
           "diagnostics.py", "symmetry2.py")


def need(condition, message):
    if not condition:
        raise b.Refusal(message)


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True)


def digest(value):
    return hashlib.sha256(canonical(value).encode("ascii")).hexdigest()


@dataclass(frozen=True)
class Profile:
    model_revision: str
    initial_secret_values: tuple
    public_context: tuple
    secret_context: tuple
    observed_variables: tuple
    observed_control: bool

    def wire(self):
        return {"schema": SCHEMA, "model_revision": self.model_revision,
                "initial_secret_values": {name: list(values) for name, values in self.initial_secret_values},
                "public_context": list(self.public_context), "secret_context": list(self.secret_context),
                "observe": {"variables": list(self.observed_variables), "control": self.observed_control}}


def _names(values, allowed, label):
    need(type(values) is list and all(type(x) is str for x in values), label + " names")
    need(values == sorted(set(values)) and set(values) <= set(allowed),
         label + " must use distinct sorted declared names")
    return tuple(values)


def parse_profile(value, model):
    """Admit a complete, immutable observer and coupling; never infer a hidden field."""
    need(type(model) is b.Machine and getattr(model, "profile", b.PROFILE) == b.PROFILE,
         "ZAL v2 machine required")
    b.validate_machine(model)
    need(not any((model.requirements, model.properties, model.fairness, model.games,
                  model.players, model.obligations, model.symmetries)),
         "relational profile supports transitions and state invariants only; "
         "requirements, temporal/game/fairness/symmetry and unresolved declarations are unsupported")
    fields = {"schema", "model_revision", "initial_secret_values", "public_context", "secret_context", "observe"}
    need(type(value) is dict and set(value) == fields, "relational profile fields")
    need(value["schema"] == SCHEMA, "relational profile schema")
    need(type(value["model_revision"]) is str and value["model_revision"] == model.revision,
         "relational model revision mismatch")
    variables = {v.name: v for v in model.variables}
    secrets = value["initial_secret_values"]
    need(type(secrets) is dict and secrets and set(secrets) <= variables.keys(),
         "initial secret variables must be declared and nonempty")
    owned = []
    root_count = 1
    for name in sorted(secrets):
        vals = secrets[name]
        var = variables[name]
        need(type(vals) is list and vals and all(type(v) is int for v in vals), "initial secret values must be integers")
        need(vals == sorted(set(vals)) and all(var.low <= v <= var.high for v in vals),
             "initial secret values must be distinct, sorted and within the declared range")
        root_count *= len(vals)
        need(root_count <= MAX_ROOTS, "initial alternative bound")
        owned.append((name, tuple(vals)))
    need(root_count >= 2, "nonvacuity requires at least two distinct initial secret alternatives")
    context_names = {*model.facts, *(i.name for i in model.inputs)}
    public = _names(value["public_context"], context_names, "public context")
    secret = _names(value["secret_context"], context_names, "secret context")
    need(not set(public) & set(secret) and set(public) | set(secret) == context_names,
         "public and secret context must partition every declared fact and integer input")
    obs = value["observe"]
    need(type(obs) is dict and set(obs) == {"variables", "control"}, "observation fields")
    observed = _names(obs["variables"], variables, "observed variable")
    need(type(obs["control"]) is bool, "control observation must be Boolean")
    # Class and reason are mandatory even when no state field is observed.
    return Profile(model.revision, tuple(owned), public, secret, observed, obs["control"])


class _Limit(Exception):
    pass


class _Budget:
    def __init__(self, states, edges, work):
        for name, value, maximum in (("states", states, MAX_STATES), ("edges", edges, MAX_EDGES),
                                     ("work", work, MAX_WORK)):
            need(type(value) is int and 0 <= value <= maximum, "invalid " + name + " budget")
        self.states, self.edges, self.maximum = states, edges, work
        self.work = 0

    def charge(self, amount=1):
        if self.work + amount > self.maximum:
            raise _Limit("work bound")
        self.work += amount


def check(model, profile, *, max_states=MAX_STATES, max_edges=MAX_EDGES, max_work=MAX_WORK):
    """Breadth-first complete paired closure, or a witness/explicit UNKNOWN.

    Malformed input raises Refusal. Supported models with invalid initial or
    reached invariants, undefined decisions or exhausted budgets yield UNKNOWN;
    no invalid state or input is silently removed from the quantified domain.
    Work counts checker operations, not machine instructions or elapsed time.
    """
    need(type(profile) is Profile, "parsed relational profile required")
    profile = parse_profile(profile.wire(), model)
    budget = _Budget(max_states, max_edges, max_work)
    variable_names = tuple(v.name for v in model.variables)
    initial = model.initial_values()
    secrets = tuple(name for name, _ in profile.initial_secret_values)
    domains = {name: (False, True) for name in model.facts}
    domains.update({i.name: tuple(range(i.low, i.high + 1)) for i in model.inputs})
    public_domains = tuple(domains[name] for name in profile.public_context)
    secret_domains = tuple(domains[name] for name in profile.secret_context)
    public_count = 1
    secret_count = 1
    for values in public_domains:
        public_count *= len(values)
    for values in secret_domains:
        secret_count *= len(values)
    input_pairs = len(model.events) * public_count * secret_count * secret_count
    root_count = 1
    for _, values in profile.initial_secret_values:
        root_count *= len(values)
    report = {"schema": REPORT_SCHEMA, "authority": "none", "scope": SCOPE,
              "model_revision": model.revision, "profile": profile.wire(),
              "profile_revision": digest(profile.wire()),
              "observer": {"mandatory": ["class", "reason"], "control": profile.observed_control,
                           "variables": list(profile.observed_variables)},
              "coupling": "same event; equal public context; all pairs of secret context values",
              "nonvacuity": {"initial_alternatives": root_count,
                             "distinct_ordered_initial_pairs": root_count * (root_count - 1)},
              "limits": {"states": max_states, "edges": max_edges, "work": max_work},
              "measured": {"states": 0, "edges": 0, "decisions": 0, "work": 0},
              "input_pairs_per_state": input_pairs,
              "status": "UNKNOWN", "reason": None, "counterexample": None}
    seen = {}
    queue = deque()
    edges = decisions = 0

    def state_wire(state):
        return {"control": state[0], "variables": dict(zip(variable_names, state[1]))}

    def invariant(state):
        values = dict(zip(variable_names, state[1]))
        step = b._Step(state[0], values, model.events[0], {})
        for name, formula in model.invariants:
            budget.charge()
            if not b.holds(formula, step):
                raise _Limit("state invariant violated: " + name)

    def observation(state):
        values = dict(zip(variable_names, state[1]))
        return {"control": state[0] if profile.observed_control else None,
                "variables": {name: values[name] for name in profile.observed_variables}}

    def witness(pair, last=None):
        steps = [] if last is None else [last]
        current = pair
        while seen[current] is not None:
            before, step = seen[current]
            steps.append(step)
            current = before
        steps.reverse()
        return {"initial": {"left": state_wire(current[0]), "right": state_wire(current[1])},
                "steps": steps, "length": len(steps)}

    def finish(status, reason=None, counterexample=None):
        report["status"], report["reason"], report["counterexample"] = status, reason, counterexample
        report["measured"] = {"states": len(seen), "edges": edges, "decisions": decisions, "work": budget.work}
        return report

    def intern(pair, parent=None):
        if pair not in seen:
            if len(seen) >= budget.states:
                raise _Limit("paired state bound")
            budget.charge()
            seen[pair] = parent
            queue.append(pair)

    def inputs():
        for event in model.events:
            for public_values in itertools.product(*public_domains):
                common = dict(zip(profile.public_context, public_values))
                for left_values in itertools.product(*secret_domains):
                    for right_values in itertools.product(*secret_domains):
                        yield event, {**common, **dict(zip(profile.secret_context, left_values))}, \
                            {**common, **dict(zip(profile.secret_context, right_values))}

    try:
        # Refuse expansion before creating roots or a Cartesian context table.
        if root_count * root_count > max_states:
            raise _Limit("initial paired state bound")
        if input_pairs > max_edges:
            raise _Limit("paired input expansion exceeds edge bound")
        roots = []
        for values in itertools.product(*(v for _, v in profile.initial_secret_values)):
            budget.charge()
            state = dict(initial)
            state.update(zip(secrets, values))
            item = (model.initial, tuple(state[n] for n in variable_names))
            invariant(item)
            roots.append(item)
        for left in roots:
            for right in roots:
                intern((left, right))
        # Check every zero-event observation before exploring positive lengths.
        for pair in queue:
            budget.charge()
            if observation(pair[0]) != observation(pair[1]):
                return finish("COUNTEREXAMPLE", "initial observation differs", witness(pair))
        while queue:
            pair = queue.popleft()
            for event, left_context, right_context in inputs():
                if edges >= max_edges:
                    raise _Limit("paired edge bound")
                budget.charge()
                outcomes = []
                targets = []
                for state, context in zip(pair, (left_context, right_context)):
                    budget.charge()
                    decisions += 1
                    try:
                        outcome = model.decide(state[0], dict(zip(variable_names, state[1])), event, context)
                    except b.Refusal as error:
                        raise _Limit("undefined model decision: " + str(error)) from error
                    target = (outcome["state"], tuple(outcome["variables"][n] for n in variable_names))
                    invariant(target)
                    outcomes.append(outcome)
                    targets.append(target)
                edges += 1
                step = {"event": event,
                        "left": {"context": left_context, "outcome": outcomes[0]},
                        "right": {"context": right_context, "outcome": outcomes[1]}}
                signatures = [{"class": out["class"], "reason": out["reason"],
                               **observation(target)} for out, target in zip(outcomes, targets)]
                if signatures[0] != signatures[1]:
                    item = witness(pair, step)
                    item["observations"] = {"left": signatures[0], "right": signatures[1]}
                    return finish("COUNTEREXAMPLE", "step observation differs", item)
                intern(tuple(targets), (pair, step))
        return finish("CHECKED", "complete reachable paired product")
    except (_Limit, RecursionError) as error:
        return finish("UNKNOWN", str(error) or "recursion bound")


def _unique(items):
    result = {}
    for key, value in items:
        need(key not in result, "duplicate JSON key")
        result[key] = value
    return result


def _number(_):
    raise b.Refusal("JSON noninteger numbers are unsupported")


def load_profile(path):
    raw = _read(path, MAX_PROFILE_BYTES)
    return json.loads(raw, object_pairs_hook=_unique, parse_float=_number, parse_constant=_number), raw


def _read(path, maximum):
    need(not path.is_symlink(), "symbolic-link input refused")
    with path.open("rb") as stream:
        raw = stream.read(maximum + 1)
    need(len(raw) <= maximum, "input byte bound")
    return raw


def identities():
    here = Path(__file__).resolve().parent
    return {name: hashlib.sha256(_read(here / name, 1024 * 1024)).hexdigest() for name in SOURCES}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("model", type=Path)
    parser.add_argument("profile", type=Path)
    parser.add_argument("--out", type=Path, help="fresh report file; existing files and symbolic links refused")
    parser.add_argument("--max-states", type=int, default=MAX_STATES)
    parser.add_argument("--max-edges", type=int, default=MAX_EDGES)
    parser.add_argument("--max-work", type=int, default=MAX_WORK)
    args = parser.parse_args(argv)
    try:
        before = identities()
        raw_model = _read(args.model, b.MAX_SOURCE)
        raw_profile, profile_bytes = load_profile(args.profile)
        model = b.parse(raw_model.decode("utf-8"))
        profile = parse_profile(raw_profile, model)
        result = check(model, profile, max_states=args.max_states, max_edges=args.max_edges, max_work=args.max_work)
        need(before == identities(), "checker source changed during check")
        result["binding"] = {"model_sha256": hashlib.sha256(raw_model).hexdigest(),
                             "profile_sha256": hashlib.sha256(profile_bytes).hexdigest(),
                             "checker_sources": before, "python": list(sys.version_info[:3])}
        encoded = json.dumps(result, sort_keys=True, indent=2) + "\n"
        need(len(encoded.encode("utf-8")) <= MAX_REPORT_BYTES, "report byte bound")
        if args.out is not None:
            need(not any(p.is_symlink() for p in (args.out, *args.out.absolute().parents)),
                 "symbolic-link output refused")
            with args.out.open("x", encoding="utf-8") as stream:
                stream.write(encoded)
        print(encoded, end="")
        return {"CHECKED": 0, "COUNTEREXAMPLE": 1, "UNKNOWN": 2}[result["status"]]
    except (b.Refusal, OSError, UnicodeError, ValueError, TypeError, KeyError, RecursionError) as error:
        print(json.dumps({"schema": REPORT_SCHEMA, "status": "UNKNOWN", "authority": "none",
                          "scope": SCOPE, "reason": str(error), "counterexample": None}))
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
