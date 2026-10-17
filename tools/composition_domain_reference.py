#!/usr/bin/env python3
"""Independent reference for the composed-domain composition profile.

A test oracle, not a shipped judge. It reads a `zeno-fcis/checked-composition/3`
manifest and each component's `project.zeno` and `v2/policy.json`, and it gets
every component decision from `tools/contract_judges_reference.py`, which decides
from the public contract rules and shares no code with the Rust checker. From the
profile's public description it implements:

- the joint step: components decide in merge order on one pre-state; a component
  whose `participates` formula does not hold is skipped; bindings read request
  fields and earlier components' pre-state; an Accept successor stays provisional;
  a Reject or any refusal (an Authority refusal, a binding with no value or outside
  its field's domain, a read-only component's committing decision that changes its
  state) ends the step with no change; the first CommittedFailure commits the
  successors up to and including its own and ends the step; otherwise every
  successor commits;
- a breadth-first product oracle over the reachable global product from genesis,
  which checks every component invariant, every coupling, every field domain and
  every delivery (domains, assumptions and read-only states);
- the three obligations, `local`, `assume` and `couple`, by direct enumeration.

Formulas are parsed and evaluated here by the expression rules of
docs/CONTRACT_RULES.md: every operand is evaluated, a number is true exactly when
it is 1, and a formula with no value (a division by zero) counts as false. A
binding is a rule expression: no multiplication or division, and checked 64-bit
arithmetic, where an overflow has no value.

Usage: composition_domain_reference.py MANIFEST [--max-pairs N]
prints one JSON object: the oracle's findings and the obligations' verdict.
"""
from __future__ import annotations

import argparse
import itertools
import json
import re
import sys
from collections import deque
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import contract_judges_reference as judges  # noqa: E402  (read-only import)

SCHEMA = "zeno-fcis/checked-composition/3"


class Refused(ValueError):
    """The manifest is outside the profile."""


class NoValue(Exception):
    """A formula has no value here."""


# --- formulas ----------------------------------------------------------------------------------

TOKEN = re.compile(r"\s*(?:(?P<number>\d+)|(?P<name>[A-Za-z_][A-Za-z_0-9]*(?:\.[A-Za-z_][A-Za-z_0-9]*)?)"
                   r"|(?P<symbol>->|&&|\|\||==|!=|<=|>=|[()!<>+\-*,]))")
BINARY = [("->",), ("||",), ("&&",), ("==", "!="), ("<", "<=", ">", ">="), ("+", "-"), ("*",)]


def parse(text: str):
    items, at = [], 0
    while at < len(text):
        if text[at:].strip() == "":
            break
        match = TOKEN.match(text, at)
        if match is None or match.end() == at:
            raise Refused(f"cannot read `{text[at:at + 16]}` in `{text}`")
        items.append((match.lastgroup, match.group(match.lastgroup)))
        at = match.end()
    position = [0]

    def peek():
        return items[position[0]] if position[0] < len(items) else ("end", "")

    def take():
        item = peek()
        position[0] += 1
        return item

    def expect(symbol):
        if take()[1] != symbol:
            raise Refused(f"expected `{symbol}` in `{text}`")

    def level(index):
        if index == len(BINARY):
            return unary()
        left = level(index + 1)
        if BINARY[index] == ("->",):
            if peek() == ("symbol", "->"):
                take()
                return ("->", left, level(index))
            return left
        while peek()[0] == "symbol" and peek()[1] in BINARY[index]:
            operator = take()[1]
            left = (operator, left, level(index + 1))
        return left

    def unary():
        if peek() == ("symbol", "!"):
            take()
            return ("not", unary())
        if peek() == ("symbol", "-"):
            take()
            return ("-", ("int", 0), unary())
        kind, value = take()
        if kind == "number":
            return ("int", int(value))
        if value == "(":
            inner = level(0)
            expect(")")
            return inner
        if kind == "name":
            if value in ("true", "false"):
                return ("int", int(value == "true"))
            if peek() == ("symbol", "("):
                take()
                arguments = [level(0)]
                while peek() == ("symbol", ","):
                    take()
                    arguments.append(level(0))
                expect(")")
                if value == "choose" and len(arguments) == 3:
                    return ("choose", *arguments)
                if value in ("div_floor", "div_ceil") and len(arguments) == 2:
                    return (value, *arguments)
                raise Refused(f"function `{value}`")
            return ("name", value)
        raise Refused(f"unexpected `{value}` in `{text}`")

    tree = level(0)
    if position[0] != len(items):
        raise Refused(f"unexpected text in `{text}`")
    return tree


def operators(tree) -> set[str]:
    if tree[0] in ("int", "name"):
        return set()
    return {tree[0]}.union(*(operators(part) for part in tree[1:]))


def names_of(tree) -> set[str]:
    if tree[0] == "name":
        return {tree[1]}
    if tree[0] == "int":
        return set()
    return set().union(*(names_of(part) for part in tree[1:]))


def value(tree, env: dict) -> int:
    kind = tree[0]
    if kind == "int":
        return tree[1]
    if kind == "name":
        return env[tree[1]]
    if kind == "not":
        return 0 if value(tree[1], env) == 1 else 1
    if kind == "choose":
        condition, then, otherwise = (value(part, env) for part in tree[1:])
        return then if condition == 1 else otherwise
    left, right = value(tree[1], env), value(tree[2], env)
    if kind in ("div_floor", "div_ceil"):
        if right == 0:
            raise NoValue("division by zero")
        return left // right if kind == "div_floor" else -((-left) // right)
    return {
        "->": lambda: int(left != 1 or right == 1),
        "||": lambda: int(left == 1 or right == 1),
        "&&": lambda: int(left == 1 and right == 1),
        "==": lambda: int(left == right),
        "!=": lambda: int(left != right),
        "<": lambda: int(left < right),
        "<=": lambda: int(left <= right),
        ">": lambda: int(left > right),
        ">=": lambda: int(left >= right),
        "+": lambda: left + right,
        "-": lambda: left - right,
        "*": lambda: left * right,
    }[kind]()


def holds(tree, env: dict) -> bool:
    try:
        return value(tree, env) == 1
    except NoValue:
        return False


I64 = (-(1 << 63), (1 << 63) - 1)


def evaluate(tree, env: dict):
    """A binding's value: a rule expression in checked 64-bit arithmetic, so a value or
    intermediate outside the 64-bit range has no value."""
    def checked(node):
        result = value(node, env)
        if not I64[0] <= result <= I64[1]:
            raise NoValue("64-bit overflow")
        if node[0] not in ("int", "name"):
            for part in node[1:]:
                checked(part)
        return result
    try:
        return checked(tree)
    except NoValue:
        return None


# --- components ------------------------------------------------------------------------------

FIELD = re.compile(r"^field\s+(\d+)\s+(\d+)\s+(\w+)\s+(\d+);")


class Component:
    """One component: its contract, layout and formulas."""

    def __init__(self, entry: dict, directory: Path):
        self.name = entry["name"]
        project = (directory / "project.zeno").read_text()
        policy = json.loads((directory / "v2" / "policy.json").read_text())
        self.contract = judges.Contract(project, policy)
        names_by_id = {}
        for line in project.splitlines():
            match = FIELD.match(line.split("//", 1)[0].strip())
            if match:
                names_by_id[(int(match.group(2)), int(match.group(1)))] = match.group(3)
        roots = {"state": 100, "command": 101, "context": 102}
        self.fields = []
        for source, _, field, domain in self.contract.positions:
            name = names_by_id[(roots[source], field)] if field is not None else source
            self.fields.append((name, source, list(domain)))
        self.width = self.contract.width
        self.state_names = [name for name, _, _ in self.fields[:self.width]]
        self.input_names = [name for name, _, _ in self.fields[self.width:]]
        self.domains = {name: domain for name, _, domain in self.fields}
        self.owns = bool(entry["owns"])
        self.participates = parse(entry["participates"])
        self.bind = {field: parse(text) for field, text in entry["bind"].items()}
        self.assume = parse(entry["assume"])
        self.invariant = parse(entry["invariant"])
        self.failure_case = self.contract.failure
        self.genesis = tuple(self.contract.genesis)
        self._decisions = {}

    def decide(self, state: tuple, inputs: tuple):
        """(class, reason, successor), or ("Refused", kind) when the Authority refuses."""
        key = (state, inputs)
        if key not in self._decisions:
            refusal = self.contract.refusal(state, inputs)
            self._decisions[key] = (("Refused", refusal[0]) if refusal
                                    else self.contract.table_decision(state, inputs))
        return self._decisions[key]

    def invariant_holds(self, state) -> bool:
        return holds(self.invariant, dict(zip(self.state_names, state)))

    def states(self):
        return itertools.product(*(self.domains[name] for name in self.state_names))


class Composition:
    def __init__(self, manifest_path: Path):
        manifest = json.loads(manifest_path.read_text())
        if manifest.get("schema") != SCHEMA:
            raise Refused("not a composed-domain manifest")
        self.request = []
        for field in manifest["request"]:
            domain = field["domain"]
            values = [0, 1] if domain["kind"] == "bool" else list(range(domain["min"], domain["max"] + 1))
            self.request.append((field["name"], values))
        base = manifest_path.parent
        self.components = [Component(entry, base / entry["path"]) for entry in manifest["components"]]
        self.by_name = {component.name: component for component in self.components}
        self.couplings = [(coupling["name"], list(coupling["members"]), parse(coupling["invariant"]))
                          for coupling in manifest["couplings"]]
        limits = manifest.get("limits") or {}
        self.max_obligation = limits.get("max_obligation", 1 << 22)
        self.max_total = limits.get("max_total", 1 << 25)
        self.owned = [component for component in self.components if component.owns]
        self.manifest = manifest
        self.validate()

    def validate(self) -> None:
        """The profile's scope rules: refused manifests raise `Refused`."""
        names = [component.name for component in self.components]
        if len(set(names)) != len(names):
            raise Refused("duplicate component name")
        owners = {}
        for entry, component in zip(self.manifest["components"], self.components):
            for owned in entry["owns"]:
                if owned in owners:
                    raise Refused(f"two components own {owned}")
                owners[owned] = component.name
                owner, _, field = owned.partition(".")
                if owner != component.name or field not in component.state_names:
                    raise Refused(f"{component.name} owns {owned}, not one of its own state fields")
            if entry["owns"] and len(entry["owns"]) != len(component.state_names):
                raise Refused(f"{component.name} owns part of its state")
        request = {name for name, _ in self.request}
        for index, component in enumerate(self.components):
            earlier = {c.name: c for c in self.components[:index] if c.owns}
            expected = set(component.input_names) | (set() if component.owns else set(component.state_names))
            if set(component.bind) != expected:
                raise Refused(f"{component.name} binds {sorted(component.bind)}, not {sorted(expected)}")
            for tree in component.bind.values():
                if {"*", "div_floor", "div_ceil"} & operators(tree):
                    raise Refused(f"{component.name} binds a value with multiplication or division")
            for field, tree in [(None, component.participates)] + list(component.bind.items()):
                for name in names_of(tree):
                    if "." in name:
                        owner, _, state_field = name.partition(".")
                        if owner not in earlier or state_field not in earlier[owner].state_names:
                            raise Refused(f"{component.name} reads {name}, not an earlier owned state field")
                    elif name not in request or (field in component.state_names and not component.owns):
                        raise Refused(f"{component.name} reads {name}")
            if not names_of(component.assume) <= set(component.input_names):
                raise Refused(f"{component.name}'s assumption reads outside its inputs")
            if not names_of(component.invariant) <= set(component.state_names):
                raise Refused(f"{component.name}'s invariant reads outside its state")
        for name, members, formula in self.couplings:
            for read in names_of(formula):
                owner, _, field = read.partition(".")
                if owner not in members or field not in self.by_name[owner].state_names:
                    raise Refused(f"coupling {name} reads {read}, not a member's state field")

    # -- environments
    @staticmethod
    def environment(request: dict, states: dict) -> dict:
        env = dict(request)
        for name, state in states.items():
            for field, value_ in state.items():
                env[f"{name}.{field}"] = value_
        return env

    def named_states(self, states: dict) -> dict:
        return {name: dict(zip(self.by_name[name].state_names, state)) for name, state in states.items()}

    def delivered(self, component: Component, env: dict):
        """(state, inputs) the composition delivers, or a refusal kind."""
        state = []
        if not component.owns:
            for name in component.state_names:
                delivered_value = evaluate(component.bind[name], env)
                if delivered_value is None:
                    return "binding-undefined"
                if delivered_value not in component.domains[name]:
                    return "binding-outside-domain"
                state.append(delivered_value)
        inputs = []
        for name in component.input_names:
            delivered_value = evaluate(component.bind[name], env)
            if delivered_value is None:
                return "binding-undefined"
            if delivered_value not in component.domains[name]:
                return "binding-outside-domain"
            inputs.append(delivered_value)
        return tuple(state), tuple(inputs)

    def joint_step(self, states: dict, request: dict):
        """(outcome, successor states); states map each owning component to its state tuple."""
        env = self.environment(request, self.named_states(states))
        after = dict(states)
        for component in self.components:
            if not holds(component.participates, env):
                continue
            delivered = self.delivered(component, env)
            if isinstance(delivered, str):
                return ("Refused", component.name, delivered), states
            state, inputs = delivered
            if component.owns:
                state = states[component.name]
            decision = component.decide(state, inputs)
            if decision[0] == "Refused":
                return ("Refused", component.name, decision[1]), states
            kind, reason, successor = decision
            if kind == "Reject":
                return ("Reject", component.name, reason), states
            if not component.owns and tuple(successor) != tuple(state):
                return ("Refused", component.name, "read-only-state-changed"), states
            if component.owns:
                after[component.name] = tuple(successor)
            if kind == "CommittedFailure":
                return ("CommittedFailure", component.name, reason), after
        return ("Accept",), after

    def requests(self):
        names = [name for name, _ in self.request]
        for values in itertools.product(*(values for _, values in self.request)):
            yield dict(zip(names, values))

    def genesis(self) -> dict:
        return {component.name: component.genesis for component in self.owned}

    # -- the product oracle
    def oracle(self, max_pairs: int) -> dict:
        genesis = self.genesis()
        freeze = lambda states: tuple(states[c.name] for c in self.owned)  # noqa: E731
        seen = {freeze(genesis)}
        frontier = [genesis]
        rows, violations, outcomes = [], [], {}
        pairs = 0
        requests = list(self.requests())
        while frontier:
            following = []
            for states in frontier:
                outside = [c.name for c in self.owned
                           if any(v not in c.domains[n] for n, v in zip(c.state_names, states[c.name]))]
                if outside:
                    violations.append({"state-domain": outside, "states": self.named_states(states)})
                    continue
                for component in self.owned:
                    if not component.invariant_holds(states[component.name]):
                        violations.append({"invariant": component.name, "states": self.named_states(states)})
                named = self.named_states(states)
                for name, _members, formula in self.couplings:
                    if not holds(formula, self.environment({}, named)):
                        violations.append({"coupling": name, "states": named})
                for request in requests:
                    pairs += 1
                    if pairs > max_pairs:
                        return {"complete": False, "pairs": pairs - 1, "violations": violations,
                                "rows": None, "outcomes": outcomes}
                    env = self.environment(request, named)
                    for component in self.components:
                        if not holds(component.participates, env):
                            continue
                        delivered = self.delivered(component, env)
                        if isinstance(delivered, str):
                            violations.append({"delivery": component.name, "failed": delivered,
                                               "states": named, "request": request})
                            continue
                        state, inputs = delivered
                        if not holds(component.assume, dict(zip(component.input_names, inputs))):
                            violations.append({"assumption": component.name, "states": named, "request": request})
                        if not component.owns and not component.invariant_holds(state):
                            violations.append({"read-only-invariant": component.name, "states": named,
                                               "request": request})
                    outcome, after = self.joint_step(states, request)
                    outcomes[outcome[0]] = outcomes.get(outcome[0], 0) + 1
                    rows.append((freeze(states), tuple(request[n] for n, _ in self.request), outcome, freeze(after)))
                    key = freeze(after)
                    if key not in seen:
                        seen.add(key)
                        following.append(after)
            frontier = following
        return {"complete": True, "pairs": pairs, "reachable_states": len(seen), "violations": violations,
                "rows": rows, "outcomes": outcomes}

    # -- the obligations, by direct enumeration
    def reads(self, component: Component):
        trees = [component.participates] + [component.bind[name] for name in component.bind]
        requested = {name for tree in trees for name in names_of(tree) if "." not in name}
        providers = {name.split(".")[0] for tree in trees for name in names_of(tree) if "." in name}
        cone = [(name, values) for name, values in self.request if name in requested]
        return cone, [c for c in self.components if c.name in providers]

    def sizes(self) -> list[tuple[str, str, int]]:
        out = []
        size_of = lambda domains: _product(len(d) for d in domains)  # noqa: E731
        for component in self.components:
            out.append(("local", component.name, size_of(d for _, _, d in component.fields)))
            cone, providers = self.reads(component)
            out.append(("assume", component.name, size_of(v for _, v in cone)
                        * _product(size_of(p.domains[n] for n in p.state_names) for p in providers)))
        for name, members, _ in self.couplings:
            cone, spaces = self.coupling_spaces(members)
            out.append(("couple", name, size_of(v for _, v in cone)
                        * _product(size_of(c.domains[n] for n in c.state_names) for c in spaces)))
        return out

    def coupling_spaces(self, members):
        cone_names, spaces = set(), set(members)
        for member in members:
            cone, providers = self.reads(self.by_name[member])
            cone_names |= {name for name, _ in cone}
            spaces |= {p.name for p in providers}
        cone = [(name, values) for name, values in self.request if name in cone_names]
        return cone, [c for c in self.components if c.name in spaces]

    def obligations(self) -> dict:
        sizes = self.sizes()
        if any(size > self.max_obligation for _, _, size in sizes) or sum(s for _, _, s in sizes) > self.max_total:
            return {"verdict": "refused", "sizes": [[k, n, str(s)] for k, n, s in sizes]}
        results = []
        for kind, name, _ in sizes:
            if kind == "local":
                results.append((kind, name, self.check_local(self.by_name[name])))
            elif kind == "assume":
                results.append((kind, name, self.check_assume(self.by_name[name])))
            else:
                results.append((kind, name, self.check_couple(name)))
        violated = [(kind, name, witness) for kind, name, witness in results if witness is not None]
        return {"verdict": "violated" if violated else "proved",
                "first": None if not violated else {"kind": violated[0][0], "subject": violated[0][1],
                                                    "witness": violated[0][2]},
                "sizes": [[k, n, str(s)] for k, n, s in sizes]}

    def check_local(self, component: Component):
        if component.owns:
            genesis = component.genesis
            if any(v not in component.domains[n] for n, v in zip(component.state_names, genesis)):
                return {"genesis": "outside the field domains"}
            if not component.contract.valid(genesis):
                return {"genesis": "a genesis law fails"}
            if not component.invariant_holds(genesis):
                return {"genesis": "the invariant fails"}
        for state in component.states():
            if not component.invariant_holds(state):
                continue
            for inputs in itertools.product(*(component.domains[n] for n in component.input_names)):
                if not holds(component.assume, dict(zip(component.input_names, inputs))):
                    continue
                decision = component.decide(tuple(state), tuple(inputs))
                if decision[0] in ("Refused", "Reject"):
                    continue
                if not component.owns and tuple(decision[2]) != tuple(state):
                    return {"state": list(state), "inputs": list(inputs), "failed": "read-only state changed"}
                if not component.invariant_holds(decision[2]):
                    return {"state": list(state), "inputs": list(inputs), "post": list(decision[2])}
        return None

    def check_assume(self, component: Component):
        cone, providers = self.reads(component)
        for request_values in itertools.product(*(v for _, v in cone)):
            request = dict(zip((n for n, _ in cone), request_values))
            for combo in itertools.product(*(list(p.states()) for p in providers)):
                if not all(p.invariant_holds(s) for p, s in zip(providers, combo)):
                    continue
                states = {p.name: dict(zip(p.state_names, s)) for p, s in zip(providers, combo)}
                env = self.environment(request, states)
                if not holds(component.participates, env):
                    continue
                delivered = self.delivered(component, env)
                if isinstance(delivered, str):
                    return {"request": request, "providers": states, "failed": delivered}
                state, inputs = delivered
                if not component.owns and not component.invariant_holds(state):
                    return {"request": request, "providers": states, "failed": "read-only invariant"}
                if not holds(component.assume, dict(zip(component.input_names, inputs))):
                    return {"request": request, "providers": states, "inputs": list(inputs)}
        return None

    def check_couple(self, name: str):
        _, members, formula = next(c for c in self.couplings if c[0] == name)
        order = [c.name for c in self.components]
        members = sorted(members, key=order.index)
        cone, spaces = self.coupling_spaces(members)

        def coupled(states: dict) -> bool:
            return holds(formula, self.environment({}, self.named_states(states)))

        genesis = {m: self.by_name[m].genesis for m in members}
        if not coupled(genesis):
            return {"genesis": genesis}
        # Outsiders with a CommittedFailure case between the previous member and each member.
        cutters, pending = {}, []
        for component in self.components:
            if component.name in members:
                cutters[component.name], pending = pending, []
            elif component.failure_case:
                pending.append(component.name)
        for combo in itertools.product(*(list(c.states()) for c in spaces)):
            states = {c.name: tuple(s) for c, s in zip(spaces, combo)}
            if not all(c.invariant_holds(states[c.name]) for c in spaces) or not coupled(states):
                continue
            named = self.named_states(states)
            for request_values in itertools.product(*(v for _, v in cone)):
                request = dict(zip((n for n, _ in cone), request_values))
                env = self.environment(request, named)
                after = dict(states)
                vetoed, failed = False, None
                for member in members:
                    component = self.by_name[member]
                    if cutters[member] and not coupled(after):
                        return {"states": states, "request": request, "after": after, "cut": cutters[member]}
                    if not holds(component.participates, env):
                        continue
                    delivered = self.delivered(component, env)
                    if isinstance(delivered, str):
                        vetoed = True
                        break
                    decision = component.decide(states[member], delivered[1])
                    if decision[0] in ("Refused", "Reject"):
                        vetoed = True
                        break
                    after[member] = tuple(decision[2])
                    if decision[0] == "CommittedFailure":
                        failed = member
                        break
                if not vetoed and not coupled(after):
                    return {"states": states, "request": request, "after": after, "failed_at": failed}
        return None


def _product(values) -> int:
    out = 1
    for item in values:
        out *= item
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--max-pairs", type=int, default=1 << 20)
    args = parser.parse_args()
    composition = Composition(args.manifest)
    oracle = composition.oracle(args.max_pairs)
    print(json.dumps({"oracle": {"complete": oracle["complete"], "pairs": oracle["pairs"],
                                 "violations": len(oracle["violations"]),
                                 "first_violation": oracle["violations"][0] if oracle["violations"] else None,
                                 "outcomes": oracle["outcomes"]},
                      "obligations": composition.obligations()}, sort_keys=True, default=list))
    return 0


if __name__ == "__main__":
    sys.exit(main())
