#!/usr/bin/env python3
"""Independent reference for the contract judges, written from the public docs.

A test oracle, not a shipped judge: it reads an application's `project.zeno`
and `v2/policy.json` by the rules in docs/CONTRACT_RULES.md and
docs/CLI_REFERENCE.md, decides every input of the declared domain itself and
evaluates the declared laws itself, then computes what `contract review
--gaps`, `contract profile`, `contract lemma`, `contract scenario` and
`contract probe` report. It shares no code with the Rust judges, which decide
through the library Authority. It covers the subset the core-component
families use: record or variant roots, `int`, `bool` and variant leaves, no
deliveries. Anything else is refused, never approximated.

Usage: contract_judges_reference.py APP_DIR [--lemma F]... [--scenario FILE]
prints one JSON object of known answers.
"""
from __future__ import annotations

import argparse
import hashlib
import itertools
import json
import re
import sys
from collections import deque
from pathlib import Path


class Unsupported(ValueError):
    """The contract or formula is outside the subset this reference reads."""


class Undefined(Exception):
    """A formula read a value the frame does not hold."""


# --- tokens and the two grammars -----------------------------------------------------------

TOKEN = re.compile(r"\s*(?:(?P<number>\d+)|(?P<name>[A-Za-z_][A-Za-z_0-9]*(?:\.\d+)*)"
                   r"|(?P<symbol>->|&&|\|\||==|!=|<=|>=|[()!<>+\-*,]))")


def tokens(text: str) -> list[tuple[str, str]]:
    found, at = [], 0
    text = text.strip()
    while at < len(text):
        match = TOKEN.match(text, at)
        if match is None or match.end() == at:
            raise Unsupported(f"cannot read `{text[at:at + 16]}`")
        kind = match.lastgroup
        found.append((kind, match.group(kind)))
        at = match.end()
    return found


class Reader:
    def __init__(self, text: str):
        self.items = tokens(text)
        self.at = 0

    def peek(self) -> tuple[str, str]:
        return self.items[self.at] if self.at < len(self.items) else ("end", "")

    def take(self) -> tuple[str, str]:
        item = self.peek()
        self.at += 1
        return item

    def accept(self, value: str) -> bool:
        if self.peek()[1] == value and self.peek()[0] != "end":
            self.at += 1
            return True
        return False

    def expect(self, value: str) -> None:
        if not self.accept(value):
            raise Unsupported(f"expected `{value}`, found `{self.peek()[1]}`")

    def finish(self):
        if self.at != len(self.items):
            raise Unsupported(f"unexpected `{self.peek()[1]}`")


# Rule expressions, loosest first: -> (right), ||, &&, == !=, < <= > >=, + -, *.
LEVELS = [("->",), ("||",), ("&&",), ("==", "!="), ("<", "<=", ">", ">="), ("+", "-"), ("*",)]


def rule_expression(text: str):
    """A rule expression of v2/policy.json as an expression tree."""
    reader = Reader(text)

    def level(index: int):
        if index == len(LEVELS):
            return unary()
        left = level(index + 1)
        if LEVELS[index] == ("->",):
            if reader.accept("->"):
                return ("op", "->", left, level(index))
            return left
        while reader.peek()[0] == "symbol" and reader.peek()[1] in LEVELS[index]:
            operator = reader.take()[1]
            left = ("op", operator, left, level(index + 1))
        return left

    def unary():
        if reader.accept("!"):
            return ("not", unary())
        if reader.accept("-"):
            return ("op", "-", ("int", 0), unary())
        return atom()

    def atom():
        kind, value = reader.take()
        if kind == "number":
            return ("int", int(value))
        if value == "(":
            inner = level(0)
            reader.expect(")")
            return inner
        if kind == "name":
            if value in ("true", "false"):
                return ("int", int(value == "true"))
            if reader.peek()[1] == "(":
                return call(value)
            return ("name", value)
        raise Unsupported(f"unexpected `{value}`")

    def call(name: str):
        reader.expect("(")
        arguments = [level(0)]
        while reader.accept(","):
            arguments.append(level(0))
        reader.expect(")")
        if name == "choose" and len(arguments) == 3:
            return ("choose", *arguments)
        if name in ("div_floor", "div_ceil") and len(arguments) == 2:
            return ("div", name, *arguments)
        raise Unsupported(f"function `{name}`")

    tree = level(0)
    reader.finish()
    return tree


def law_formula(text: str):
    """A project.zeno law formula: a relation of comparisons of values."""
    reader = Reader(text)

    def relation():
        left = disjunction()
        if reader.accept("->"):
            return ("op", "->", left, relation())
        return left

    def disjunction():
        left = conjunction()
        while reader.accept("||"):
            left = ("op", "||", left, conjunction())
        return left

    def conjunction():
        left = negation()
        while reader.accept("&&"):
            left = ("op", "&&", left, negation())
        return left

    def negation():
        if reader.accept("!") or reader.accept("not"):
            return ("not", negation())
        return comparison()

    def comparison():
        if reader.accept("true"):
            return ("int", 1)
        if reader.accept("false"):
            return ("int", 0)
        if reader.accept("("):
            inner = relation()
            reader.expect(")")
            return inner
        left = value()
        operator = reader.take()[1]
        if operator not in ("==", "!=", "<", "<=", ">", ">="):
            raise Unsupported("a comparison is needed")
        return ("op", operator, left, value())

    def value():
        left = term()
        while reader.peek()[1] in ("+", "-") and reader.peek()[0] == "symbol":
            operator = reader.take()[1]
            left = ("op", operator, left, term())
        return left

    def term():
        left = factor()
        while reader.accept("*"):
            left = ("op", "*", left, factor())
        return left

    def factor():
        if reader.accept("-"):
            return ("op", "-", ("int", 0), factor())
        kind, text_ = reader.take()
        if kind == "number":
            return ("int", int(text_))
        if text_ == "(":
            inner = value()
            reader.expect(")")
            return inner
        if kind == "name":
            if text_ in ("div_floor", "div_ceil") and reader.accept("("):
                left = value()
                reader.expect(",")
                right = value()
                reader.expect(")")
                return ("div", text_, left, right)
            return ("name", text_)
        raise Unsupported(f"unexpected `{text_}`")

    tree = relation()
    reader.finish()
    return tree


def compile_tree(tree, slot_of, variables, cache=None):
    """`tree` as a function of a frame, a list of values with None where the frame holds none.

    Names resolve to frame slots, then to variables. Every node is evaluated, both
    sides of `&&`, `||` and `->` and all three arguments of `choose`, as the
    library evaluates a law or a decision program, so reading a value the frame
    lacks anywhere in a formula makes it undefined. A number used as a boolean
    holds when it equals 1."""
    cache = {} if cache is None else cache
    kind = tree[0]
    if kind == "int":
        constant = tree[1]
        return lambda frame: constant
    if kind == "name":
        name = tree[1]
        if name in slot_of:
            slot = slot_of[name]

            def read(frame):
                value = frame[slot]
                if value is None:
                    raise Undefined(name)
                return value
            return read
        if name in variables:
            if name not in cache:
                cache[name] = None
                cache[name] = compile_tree(variables[name], slot_of, variables, cache)
            function = cache[name]
            if function is None:
                raise Unsupported(f"variable `{name}` uses itself")
            return function

        def unknown(frame):
            raise Undefined(name)
        return unknown
    if kind == "not":
        inner = compile_tree(tree[1], slot_of, variables, cache)
        return lambda frame: 0 if inner(frame) == 1 else 1
    if kind == "choose":
        condition, then, otherwise = (compile_tree(part, slot_of, variables, cache)
                                      for part in tree[1:])

        def choose(frame):
            chosen, first, second = condition(frame), then(frame), otherwise(frame)
            return first if chosen == 1 else second
        return choose
    if kind == "div":
        left = compile_tree(tree[2], slot_of, variables, cache)
        right = compile_tree(tree[3], slot_of, variables, cache)
        floor = tree[1] == "div_floor"

        def divide(frame):
            a, b = left(frame), right(frame)
            if b == 0:
                raise Undefined("division by zero")
            return a // b if floor else -((-a) // b)
        return divide
    operator = tree[1]
    left = compile_tree(tree[2], slot_of, variables, cache)
    right = compile_tree(tree[3], slot_of, variables, cache)
    combine = {
        "->": lambda a, b: int(a != 1 or b == 1),
        "||": lambda a, b: int(a == 1 or b == 1),
        "&&": lambda a, b: int(a == 1 and b == 1),
        "==": lambda a, b: int(a == b),
        "!=": lambda a, b: int(a != b),
        "<": lambda a, b: int(a < b),
        "<=": lambda a, b: int(a <= b),
        ">": lambda a, b: int(a > b),
        ">=": lambda a, b: int(a >= b),
        "+": lambda a, b: a + b,
        "-": lambda a, b: a - b,
        "*": lambda a, b: a * b,
    }[operator]
    return lambda frame: combine(left(frame), right(frame))


# --- the contract ----------------------------------------------------------------------------

ROOTS = (("state", "pre", 100), ("command", "command", 101), ("context", "context", 102))
SCOPES = {"any", "accept", "reject", "failure", "commit"}


class Contract:
    """A contract read from its two files."""

    def __init__(self, project: str, policy: dict):
        if policy.get("delivery_laws"):
            raise Unsupported("delivery laws")
        types, records, variants = {}, {}, {}
        self.reasons, self.laws = [], []
        for raw in project.splitlines():
            line = raw.split("//", 1)[0].strip()
            if not line:
                continue
            words = line.rstrip(";").split()
            head = words[0]
            if head == "type":
                identifier, kind = int(words[1]), words[2]
                if kind == "int":
                    low, high = re.fullmatch(r"(-?\d+)\.\.=(-?\d+)", words[-1]).groups()
                    types[identifier] = list(range(int(low), int(high) + 1))
                elif kind == "bool":
                    types[identifier] = [0, 1]
                elif kind in ("data", "state", "command", "context"):
                    types.setdefault(identifier, None)
                else:
                    raise Unsupported(f"type kind `{kind}`")
            elif head == "field":
                records.setdefault(int(words[2]), []).append((int(words[1]), int(words[4])))
            elif head == "variant":
                if words[4] != "none":
                    raise Unsupported("variants with payloads")
                variants.setdefault(int(words[2]), []).append(int(words[1]))
            elif head == "reason":
                self.reasons.append(int(words[1]))
            elif head == "law":
                match = re.fullmatch(r"law\s+(\d+)\s+(\w+)\s+on\s+([a-z0-9:,\s]+?)\s*=\s*(.*);", line)
                if match is None:
                    raise Unsupported(f"law declaration `{line}`")
                scopes = {scope.strip() for scope in match.group(3).split(",")}
                genesis = "genesis" in scopes
                scopes.discard("genesis")
                if len(scopes) != 1 or not all(scope in SCOPES or re.fullmatch(r"reject:[1-9][0-9]*", scope) for scope in scopes):
                    raise Unsupported(f"law scope `{match.group(3)}`")
                self.laws.append({"id": int(match.group(1)), "scope": scopes.pop(),
                                  "genesis": genesis, "text": match.group(4),
                                  "formula": law_formula(match.group(4))})
        for identifier, listed in variants.items():
            types[identifier] = sorted(listed)
        for identifier, binding in policy.get("leaf_bindings", {}).items():
            if binding != ["Bool"]:
                raise Unsupported(f"leaf binding {binding}")
            types[int(identifier)] = [0, 1]
        self.kinds = {int(law): kind for law, kind in policy["law_kinds"].items()}
        self.reasons.sort()
        # Positions in program order: each root's fields by ID, or the root.
        self.positions = []
        for source, prefix, root in ROOTS:
            if int(policy["roots"][source]) != root:
                raise Unsupported("roots other than 100, 101, 102")
            if root in records:
                for field, type_id in sorted(records[root]):
                    domain = types.get(type_id)
                    if not domain:
                        raise Unsupported(f"field {field} type {type_id}")
                    self.positions.append((source, f"{prefix}.{root}.{field}", field, domain))
            elif types.get(root):
                self.positions.append((source, f"{prefix}.{root}", None, types[root]))
            else:
                raise Unsupported(f"root {root}")
        self.width = sum(1 for position in self.positions if position[0] == "state")
        self.state_fields = [position[2] for position in self.positions[:self.width]]
        if None in self.state_fields:
            raise Unsupported("a scalar state root")
        self.variables = {name: rule_expression(str(text)) for name, text in policy["variables"].items()}
        self.cases = []
        for case in policy["cases"]:
            if case["outbox"]:
                raise Unsupported("deliveries")
            post = {int(field): rule_expression(value) if isinstance(value, str)
                    else ("int", int(value)) for field, value in case["post"].items()}
            when = case["when"]
            self.cases.append({"when": rule_expression(when) if isinstance(when, str)
                               else ("int", int(when)),
                               "class": case["class"], "reason": case["reason"], "post": post})
        self.failure = any(case["class"] == "CommittedFailure" for case in self.cases)
        self.genesis = tuple(int(policy["genesis"][str(field)]) for field in self.state_fields)
        self.domains = [position[3] for position in self.positions]
        self.states = list(itertools.product(*self.domains[:self.width]))
        self.inputs = list(itertools.product(*self.domains[self.width:]))
        self.state_index = {state: index for index, state in enumerate(self.states)}
        # A frame is the pre-state, the command and context, then the successor.
        self.slot_of = {position[1]: slot for slot, position in enumerate(self.positions)}
        for slot, position in enumerate(self.positions[:self.width]):
            self.slot_of["post" + position[1][3:]] = len(self.positions) + slot
        for law in self.laws:
            law["holds"] = self.compile(law["formula"])
        for case in self.cases:
            case["guard"] = self.compile(case["when"])
            case["values"] = {field: self.compile(tree) for field, tree in case["post"].items()}

    def compile(self, tree):
        return compile_tree(tree, self.slot_of, self.variables)

    def frame(self, pre=None, command_context=None, post=None) -> list:
        frame = [None] * (len(self.positions) + self.width)
        if pre is not None:
            frame[:self.width] = pre
        if command_context is not None:
            frame[self.width:len(self.positions)] = command_context
        if post is not None:
            frame[len(self.positions):] = post
        return frame

    # -- the case table
    def case_of(self, state, command_context) -> int:
        frame = self.frame(state, command_context)
        for index, case in enumerate(self.cases):
            if case["guard"](frame) == 1:
                return index
        raise Unsupported("no case decides")

    def table_decision(self, state, command_context):
        """(class, reason, successor) the case table makes; a reject keeps the state."""
        index = self.case_of(state, command_context)
        case = self.cases[index]
        if case["class"] == "Reject":
            return ("Reject", case["reason"], tuple(state))
        frame = self.frame(state, command_context)
        post = list(state)
        for slot, field in enumerate(self.state_fields):
            if field in case["values"]:
                post[slot] = case["values"][field](frame)
        return (case["class"], case["reason"], tuple(post))

    def law_applies(self, law, decision_class: str, reason=None) -> bool:
        if self.kinds.get(law["id"]) == "InitialCondition":
            return False
        scope = law["scope"]
        return ((scope == f"reject:{reason}" and decision_class == "Reject")
                or scope == "any"
                or (scope == "accept" and decision_class == "Accept")
                or (scope == "reject" and decision_class == "Reject")
                or (scope == "failure" and decision_class == "CommittedFailure")
                or (scope == "commit" and decision_class != "Reject"))

    def admits(self, state, command_context, decision, laws=None) -> bool:
        """Every declared law that applies holds on the decision's transition frame."""
        decision_class, reason, successor = decision
        if decision_class == "CommittedFailure" and "CommittedFailureEffects" not in self.kinds.values():
            return False
        post = None if decision_class == "Reject" else successor
        frame = self.frame(state, command_context, post)
        for law in self.laws if laws is None else laws:
            if not self.law_applies(law, decision_class, reason):
                continue
            try:
                if law["holds"](frame) != 1:
                    return False
            except Undefined:
                return False
        return True

    def decide(self, state, command_context):
        """The decision, or None when the Authority would refuse it."""
        return None if self.refusal(state, command_context) else self.table_decision(
            state, command_context)

    def refusal(self, state, command_context):
        """Why the Authority refuses the case table's decision: (`domain`, None) for a
        successor outside its declared domain, else (`law`, ID) for the first declared law,
        in declaration order, that applies and does not hold; None when it decides."""
        decision = self.table_decision(state, command_context)
        if decision[0] != "Reject" and any(value not in domain for value, domain
                                           in zip(decision[2], self.domains[:self.width])):
            return ("domain", None)
        for law in self.laws:
            if not self.admits(state, command_context, decision, [law]):
                return ("law", law["id"])
        return None

    def state_holds(self, function, state) -> bool:
        """A compiled formula on a genesis frame, where only the successor is the state."""
        try:
            return function(self.frame(post=state)) == 1
        except Undefined:
            return False

    def state_laws(self):
        return [law for law in self.laws if law["genesis"] and law["scope"] in ("commit", "any")
                and self.kinds.get(law["id"]) != "InitialCondition"]

    def valid(self, state) -> bool:
        return all(self.state_holds(law["holds"], state) for law in self.state_laws())


# --- the machine and the judges --------------------------------------------------------------

def digest(lines) -> str:
    return hashlib.sha256("".join(f"{line}\n" for line in lines).encode()).hexdigest()


class Machine:
    def __init__(self, contract: Contract):
        self.contract = contract
        self.table, self.refusals = {}, {}
        for state in contract.states:
            for command_context in contract.inputs:
                refusal = contract.refusal(state, command_context)
                self.refusals[(state, command_context)] = refusal
                self.table[(state, command_context)] = None if refusal else contract.table_decision(
                    state, command_context)

    def after(self, state, command_context):
        decision = self.table[(state, command_context)]
        return state if decision is None else decision[2]

    def commits(self, state, command_context) -> bool:
        decision = self.table[(state, command_context)]
        return decision is not None and decision[0] != "Reject"

    def reachable(self):
        """Breadth-first from genesis, inputs in order: depth and first parent of each state."""
        genesis = self.contract.genesis
        parent = {genesis: None}
        queue = deque([genesis])
        while queue:
            state = queue.popleft()
            for command_context in self.contract.inputs:
                successor = self.after(state, command_context)
                if successor not in parent:
                    parent[successor] = (state, command_context)
                    queue.append(successor)
        return parent

    @staticmethod
    def trace(parent, state):
        steps = []
        while parent[state] is not None:
            state, command_context = parent[state]
            steps.append(list(command_context))
        return steps[::-1]


def decision_line(decision, state) -> str:
    if decision is None:
        return "X"
    letter = {"Accept": "A", "Reject": "R", "CommittedFailure": "F"}[decision[0]]
    reason = "-" if decision[1] is None else str(decision[1])
    return " ".join([letter, reason, *map(str, decision[2] if decision[0] != "Reject" else state)])


def bits(count: int) -> int:
    return 0 if count <= 1 else (count - 1).bit_length()


def moore_classes(machine: Machine, reachable: list):
    """Classes of reachable states no input sequence tells apart by outcomes, numbered by least state."""
    contract = machine.contract

    def signature(state, command_context):
        decision = machine.table[(state, command_context)]
        return ("X", *machine.refusals[(state, command_context)]) if decision is None else (
            decision[0], decision[1])

    partition = {state: tuple(signature(state, x) for x in contract.inputs) for state in reachable}
    count = len(set(partition.values()))
    while True:
        refined = {state: (partition[state],
                           tuple(partition[machine.after(state, x)] for x in contract.inputs))
                   for state in reachable}
        new_count = len(set(refined.values()))
        partition = refined
        if new_count == count:
            break
        count = new_count
    numbers = {}
    for state in sorted(reachable, key=contract.state_index.get):
        numbers.setdefault(partition[state], len(numbers))
    return {state: numbers[partition[state]] for state in reachable}


def profile(contract: Contract, machine: Machine) -> dict:
    parent = machine.reachable()
    reachable = [state for state in contract.states if state in parent]
    classes = moore_classes(machine, reachable)
    valid = {state: contract.valid(state) for state in contract.states}
    kept = into_valid = into_invalid = refused = refused_reachable = 0
    for state in contract.states:
        for command_context in contract.inputs:
            decision = machine.table[(state, command_context)]
            refused += decision is None
            refused_reachable += decision is None and state in parent
            if valid[state]:
                continue
            if not machine.commits(state, command_context):
                kept += 1
            elif valid[decision[2]]:
                into_valid += 1
            else:
                into_invalid += 1
    stuck = sum(1 for state in reachable
                if not any(machine.commits(state, x) for x in contract.inputs))
    back = {contract.genesis}
    changed = True
    while changed:
        changed = False
        for state in reachable:
            if state not in back and any(machine.after(state, x) in back for x in contract.inputs):
                back.add(state)
                changed = True
    minimal = len(set(classes.values()))
    valid_count = sum(valid.values())
    return {
        "states": {"schema": len(contract.states), "valid": valid_count,
                   "reachable": len(reachable), "minimal": minimal},
        "memory_bits": {"declared": sum(bits(len(domain)) for domain in contract.domains[:contract.width]),
                        "reachable": bits(len(reachable)), "minimal": bits(minimal)},
        "state_laws": [law["id"] for law in contract.state_laws()],
        "robustness": {
            "invalid_prestates": len(contract.states) - valid_count,
            "invalid_prestate_tuples": {"kept": kept, "committed_into_valid": into_valid,
                                        "committed_into_invalid": into_invalid},
            "refused_tuples": {"all": refused, "reachable": refused_reachable},
            "reachable_states_committing_nothing": stuck,
            "genesis_reachable_from_every_reachable_state": all(state in back for state in reachable),
        },
        "tables": {"decisions_sha256": digest(decision_lines(contract, machine)),
                   "states_sha256": digest(state_lines(contract, valid, parent, classes))},
    }


def decision_lines(contract: Contract, machine: Machine) -> list[str]:
    """One line per tuple in ordinal order, as the profile's decisions table."""
    return [decision_line(machine.table[(state, x)], state)
            for state in contract.states for x in contract.inputs]


def state_lines(contract: Contract, valid, parent, classes) -> list[str]:
    """One line per declared state, as the profile's states table."""
    return [f"{int(valid[state])} {int(state in parent)} {classes.get(state, '-')}"
            for state in contract.states]


def candidates(contract: Contract, state):
    # The independent reference derives each reason partition from the case
    # declarations, not from the production judge's candidate list.
    reasons = {kind: sorted({case["reason"] for case in contract.cases
                            if case["class"] == kind})
               for kind in ("Reject", "CommittedFailure")}
    found = [("Reject", reason, tuple(state)) for reason in reasons["Reject"]]
    found += [("Accept", None, successor) for successor in contract.states]
    if contract.failure:
        found += [("CommittedFailure", reason, successor)
                  for reason in reasons["CommittedFailure"] for successor in contract.states]
    return found


def gaps(contract: Contract, machine: Machine) -> dict:
    valid = {state: contract.valid(state) for state in contract.states}
    blank = lambda: {"tuples": 0, "reference_refused": 0, "determined": 0,
                     "freedom": {"class": 0, "reject_reason": 0, "successor": 0}}
    measures = {"full": blank(), "valid": blank()}
    activation = {"full": {str(law["id"]): 0 for law in contract.laws},
                  "valid": {str(law["id"]): 0 for law in contract.laws}}
    lines, vacuous = [], 0
    reject_reason = min((case["reason"] for case in contract.cases if case["class"] == "Reject"),
                        default=None)
    for state in contract.states:
        domains = ["full", "valid"] if valid[state] else ["full"]
        for command_context in contract.inputs:
            reference = machine.table[(state, command_context)]
            if reject_reason is not None and reference is not None and not contract.admits(
                    state, command_context, ("Reject", reject_reason, tuple(state))):
                vacuous += 1
            admitted = [decision for decision in candidates(contract, state)
                        if contract.admits(state, command_context, decision)]
            # Executable refusal and law freedom are separate observations.
            # Retain the digest's refusal marker while measuring alternatives.
            lines.append("-" if reference is None else str(len(admitted)))
            for domain in domains:
                counts = measures[domain]
                counts["tuples"] += 1
                counts["reference_refused"] += reference is None
                if reference is not None:
                    for law in contract.laws:
                        activation[domain][str(law["id"])] += contract.law_applies(law, reference[0], reference[1])
                if len(admitted) == 1:
                    counts["determined"] += 1
                    continue
                freedom = counts["freedom"]
                freedom["class"] += len({decision[0] for decision in admitted}) > 1
                freedom["reject_reason"] += len({decision[1] for decision in admitted
                                                 if decision[0] == "Reject"}) > 1
                freedom["successor"] += len({(decision[0], decision[2]) for decision in admitted
                                             if decision[0] != "Reject"}) > 1
    inactive = [law["id"] for law in contract.laws if activation["full"][str(law["id"])] == 0]
    return {"admissible": measures, "activation": {**activation, "inactive": inactive},
            "admissible_lines": lines, "admissible_sha256": digest(lines),
            "reject_everything": None if reject_reason is None else {
                "reject_reason": reject_reason,
                "refused_where_the_contract_decides": vacuous,
                "admitted_where_the_contract_decides": vacuous == 0}}


def lemma(contract: Contract, machine: Machine, text: str) -> dict:
    formula = contract.compile(law_formula(text))
    admitted = {state: contract.state_holds(formula, state) for state in contract.states}
    parent = machine.reachable()
    genesis = contract.genesis
    out = {"formula": text, "initiation": admitted[genesis],
           "holds_sha256": digest(str(int(admitted[state])) for state in contract.states),
           "states": {"declared": len(contract.states), "admitted": sum(admitted.values()),
                      "reachable": len(parent)}}
    steps = [(state, x) for state in contract.states if admitted[state] for x in contract.inputs
             if machine.commits(state, x) and not admitted[machine.after(state, x)]]
    violations = [state for state in contract.states if state in parent and not admitted[state]]
    if not admitted[genesis]:
        out["verdict"] = "false"
        out["genesis"] = list(genesis)
    elif violations:
        nearest = min(violations, key=lambda state: (len(Machine.trace(parent, state)),
                                                     contract.state_index[state]))
        out["verdict"] = "false"
        out["trace_from_genesis"] = Machine.trace(parent, nearest)
        out["counterexample_to_induction"] = {"state": list(parent[nearest][0]),
                                              "input": list(parent[nearest][1]),
                                              "successor": list(nearest), "count": len(steps)}
    elif steps:
        state, x = steps[0]
        out["verdict"] = "not-inductive"
        out["counterexample_to_induction"] = {"state": list(state), "input": list(x),
                                              "successor": list(machine.after(state, x)),
                                              "count": len(steps)}
    else:
        valid = {state: contract.valid(state) for state in contract.states}
        unreachable = sum(1 for state in contract.states if admitted[state] and state not in parent)
        out["verdict"] = "inductive"
        out["exact"] = unreachable == 0 and sum(admitted.values()) == len(parent)
        out["unreachable_admitted"] = unreachable
        out["states"]["valid"] = sum(valid.values())
        out["implies_state_laws"] = all(valid[state] for state in contract.states if admitted[state])
    return out


def scenario_steps(text: str, width: int):
    """Scenarios of a scenario file: decision-example lines under `# scenario NAME` headers."""
    scenarios, current = [], None
    for number, raw in enumerate(text.splitlines(), start=1):
        line = raw.strip()
        if not line:
            continue
        if line.startswith("#"):
            words = line[1:].split()
            if words and words[0] == "scenario":
                current = {"name": " ".join(words[1:]), "line": number, "steps": []}
                scenarios.append(current)
            continue
        sections = [section.strip() for section in line.split("|")]
        if sections[-1] != "-":
            raise Unsupported("deliveries")
        inputs = [int(word) for section in sections[:-2] for word in section.split()]
        decision = sections[-2].split()
        verdict = {"accept": "Accept", "reject": "Reject", "failure": "CommittedFailure"}[decision[0]]
        reason = None if decision[1] == "-" else int(decision[1])
        current["steps"].append((tuple(inputs[:width]), tuple(inputs[width:]),
                                 (verdict, reason, tuple(int(word) for word in decision[2:]))))
    return scenarios


def replay(machine: Machine, scenario, decide=None) -> int | None:
    """The first failing step, from 1, or None when every step agrees."""
    decide = decide or (lambda state, x: machine.table[(state, x)])
    state = scenario["steps"][0][0]
    for number, (pre, command_context, expected) in enumerate(scenario["steps"], start=1):
        if pre != state:
            return number
        actual = decide(state, command_context)
        if actual is None:
            return number
        shown = actual if actual[0] != "Reject" else (actual[0], actual[1], state)
        if shown != expected:
            return number
        state = actual[2] if actual[0] != "Reject" else state
    return None


def scenarios(contract: Contract, machine: Machine, text: str) -> dict:
    found = scenario_steps(text, contract.width)
    reject_reason = min((case["reason"] for case in contract.cases if case["class"] == "Reject"),
                        default=None)

    def rejects(state, x):
        decision = ("Reject", reject_reason, state)
        return decision if contract.admits(state, x, decision) else None
    results = [{"name": item["name"], "steps": len(item["steps"]),
                "start": "genesis" if item["steps"][0][0] == contract.genesis else "stated",
                "verdict": "passed" if replay(machine, item) is None else "failed"}
               for item in found]
    # Kills count only when every scenario passes on the contract itself.
    if any(result["verdict"] == "failed" for result in results):
        return {"scenarios": results, "reject_everything_killed": "withheld"}
    killed = reject_reason is not None and any(replay(machine, item, rejects) is not None
                                               for item in found)
    return {"scenarios": results, "reject_everything_killed": killed}


def probes(contract: Contract, machine: Machine) -> dict:
    """Decision counts, dependence on each position, and the case of the first input."""
    counts = {}
    for key, decision in machine.table.items():
        if decision is None:
            kind, law = machine.refusals[key]
            name = f"Refused {kind}" if law is None else f"Refused {kind} {law}"
        else:
            name = decision[0] if decision[1] is None else f"{decision[0]} {decision[1]}"
        counts[name] = counts.get(name, 0) + 1

    def outcome(state, x):
        decision = machine.table[(state, x)]
        if decision is None:
            return ("X", *machine.refusals[(state, x)])
        return (decision[0], decision[1], decision[2] if decision[0] != "Reject" else None)
    depends = {}
    for slot, (_, name, _, domain) in enumerate(contract.positions):
        # A position with one value has no two inputs that differ only there.
        answer = "no" if len(domain) > 1 else "vacuous"
        for state in contract.states:
            for x in contract.inputs:
                values = list(state) + list(x)
                if values[slot] != domain[0]:
                    continue
                seen = set()
                for value in domain:
                    values[slot] = value
                    seen.add(outcome(tuple(values[:contract.width]), tuple(values[contract.width:])))
                if len(seen) > 1:
                    answer = "yes"
                    break
            if answer == "yes":
                break
        depends[name] = answer
    first_state, first_input = contract.states[0], contract.inputs[0]
    return {"decisions": dict(sorted(counts.items())), "depends": depends,
            "explain_first_input_case": contract.case_of(first_state, first_input)}


def answers(directory: Path, lemmas: list[str], scenario_text: str | None) -> dict:
    contract = Contract((directory / "project.zeno").read_text(),
                        json.loads((directory / "v2" / "policy.json").read_text()))
    machine = Machine(contract)
    measured_gaps = gaps(contract, machine)
    measured_gaps.pop("admissible_lines")
    result = {"profile": profile(contract, machine), "gaps": measured_gaps,
              "lemmas": [lemma(contract, machine, text) for text in lemmas],
              "probes": probes(contract, machine)}
    if scenario_text is not None:
        result["scenario"] = scenarios(contract, machine, scenario_text)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("app", type=Path)
    parser.add_argument("--lemma", action="append", default=[])
    parser.add_argument("--scenario", type=Path)
    arguments = parser.parse_args()
    text = arguments.scenario.read_text() if arguments.scenario else None
    try:
        print(json.dumps(answers(arguments.app, arguments.lemma, text), sort_keys=True))
    except Unsupported as error:
        print(f"reference refused: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
