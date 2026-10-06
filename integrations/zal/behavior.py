"""Experimental ZAL finite-FSM profile: pure owned values and bounded checks.

No eval, model calls, I/O, authority capabilities or production effect execution.
The symbolic and controlled-English parsers build exactly the same value.
"""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
import itertools
import json
import re


PROFILE = "zal/finite-fsm/1"
MAX_SOURCE = 64 * 1024
NAME = r"[a-z][a-z0-9_]{0,31}"
FUTURE = {"effects", "liveness", "fairness", "arithmetic", "concurrency"}


class Refusal(ValueError):
    """Invalid source or an unsupported operation; never a business rejection."""


@dataclass(frozen=True)
class Formula:
    op: str
    args: tuple = ()

    def __post_init__(self):
        if not isinstance(self.args, tuple) or self.op not in {"true", "false", "fact", "state", "not", "and", "or"}:
            raise Refusal("Unknown or mutable formula node")
        expected = 0 if self.op in {"true", "false"} else 1 if self.op in {"fact", "state", "not"} else None
        if expected is not None and len(self.args) != expected or expected is None and len(self.args) < 2:
            raise Refusal("Wrong formula arity")
        if self.op in {"fact", "state"}:
            if not isinstance(self.args[0], str) or not re.fullmatch(NAME, self.args[0]):
                raise Refusal("Invalid formula reference")
        elif any(not isinstance(arg, Formula) for arg in self.args):
            raise Refusal("Formula children must be owned typed nodes")

    def value(self, state: str, context: dict[str, bool]) -> bool:
        if self.op == "true":
            return True
        if self.op == "false":
            return False
        if self.op == "fact":
            return context[self.args[0]]
        if self.op == "state":
            return state == self.args[0]
        if self.op == "not":
            return not self.args[0].value(state, context)
        values = [arg.value(state, context) for arg in self.args]
        return all(values) if self.op == "and" else any(values)

    def render(self, english=False) -> str:
        if self.op in {"true", "false", "fact"}:
            return self.args[0] if self.op == "fact" else self.op
        if self.op == "state":
            return f"state {'is' if english else '=='} {self.args[0]}"
        if self.op == "not":
            return f"({'not ' if english else '!'}{self.args[0].render(english)})"
        join = f" {'and' if self.op == 'and' else 'or'} " if english else f" {'&' if self.op == 'and' else '|'} "
        return "(" + join.join(x.render(english) for x in self.args) + ")"

    def data(self):
        return [self.op, *[a.data() if isinstance(a, Formula) else a for a in self.args]]


def combine(op: str, args: list[Formula]) -> Formula:
    flat = []
    for arg in args:
        flat.extend(arg.args if arg.op == op else [arg])
    # Boolean operations are total here; canonical commutativity does not
    # license reordering arithmetic/trapping operations in a later profile.
    unique = {json.dumps(arg.data(), separators=(",", ":")): arg for arg in flat}
    values = tuple(unique[key] for key in sorted(unique))
    return values[0] if len(values) == 1 else Formula(op, values)


def parse_formula(text: str, facts: tuple[str, ...], states: tuple[str, ...], english=False,
                  invariant=False) -> Formula:
    if len(text) > 2048:
        raise Refusal("Formula exceeds 2048 characters")
    tokens = re.findall(r"==|[!&|()]|[a-z][a-z0-9_]*|\S", text)
    if len(tokens) > 512:
        raise Refusal("Formula exceeds 512 lexical tokens")
    if english:
        tokens = [{"not": "!", "and": "&", "or": "|", "is": "=="}.get(t, t) for t in tokens]
    index = 0

    def atom(depth=0):
        nonlocal index
        if depth > 64 or index >= len(tokens):
            raise Refusal("Expected a bounded Boolean formula")
        token = tokens[index]
        index += 1
        if token == "!":
            return Formula("not", (atom(depth + 1),))
        if token == "(":
            result = expr(0, depth + 1)
            if index >= len(tokens) or tokens[index] != ")":
                raise Refusal("Expected closing parenthesis")
            index += 1
            return result
        if token in {"true", "false"}:
            return Formula(token)
        if token == "state" and index + 1 < len(tokens) and tokens[index] == "==":
            selected = tokens[index + 1]
            index += 2
            if selected not in states:
                raise Refusal(f"Unknown state {selected}")
            return Formula("state", (selected,))
        if not invariant and token in facts:
            return Formula("fact", (token,))
        raise Refusal(f"Unknown {'state predicate' if invariant else 'context observation'} {token}")

    def expr(level, depth=0):
        nonlocal index
        if level == 2:
            return atom(depth)
        left = expr(level + 1, depth)
        operator = "|" if level == 0 else "&"
        terms = [left]
        while index < len(tokens) and tokens[index] == operator:
            index += 1
            terms.append(expr(level + 1, depth))
        return combine("or" if level == 0 else "and", terms)

    result = expr(0)
    if index != len(tokens):
        raise Refusal(f"Unexpected formula token {tokens[index]}")
    def size(formula):
        children = [a for a in formula.args if isinstance(a, Formula)]
        sizes = [size(child) for child in children]
        return (1 + sum(n for n, _ in sizes), 1 + max([d for _, d in sizes], default=0))
    nodes, depth = size(result)
    if nodes > 128 or depth > 16 or any(len(result.render(e)) > 2048 for e in [False, True]):
        raise Refusal("Canonical formula exceeds 128 nodes, depth 16 or 2048 characters")
    return result


@dataclass(frozen=True)
class Rule:
    id: str
    source: str
    event: str
    guard: Formula
    target: str | None
    reason: str | None

    def data(self):
        return {"id": self.id, "source": self.source, "event": self.event,
                "guard": self.guard.data(), "target": self.target, "reason": self.reason}


@dataclass(frozen=True)
class Model:
    name: str
    states: tuple[str, ...]
    events: tuple[str, ...]
    facts: tuple[str, ...]
    initial: str
    default: str
    rules: tuple[Rule, ...]
    invariants: tuple[tuple[str, Formula], ...]
    obligations: tuple[tuple[str, str], ...] = ()

    def __post_init__(self):
        validate_model(self)

    def data(self):
        return {"profile": PROFILE, "name": self.name, "states": self.states,
                "events": self.events, "context": self.facts, "initial": self.initial,
                "default": self.default, "rules": [r.data() for r in self.rules],
                "invariants": [[name, formula.data()] for name, formula in self.invariants],
                "obligations": self.obligations, "atomic": True, "frame": "control-only"}

    @property
    def revision(self):
        return hashlib.sha256(canonical(self.data()).encode()).hexdigest()

    def render(self, english=False):
        if english:
            lines = [f"ZAL version 1.", f"The machine is {self.name}.",
                     f"The states are {', '.join(self.states)}.", f"The events are {', '.join(self.events)}.",
                     f"The context observations are {', '.join(self.facts) or 'none'}.",
                     f"Initially the state is {self.initial}.",
                     "Each step is atomic. Only the control state may change.",
                     f"Otherwise reject because {self.default} and keep the state unchanged."]
            for rule in self.rules:
                outcome = f"move to {rule.target}" if rule.target else f"reject because {rule.reason} and keep the state unchanged"
                lines.append(f"Rule {rule.id}: when the state is {rule.source} and the event is {rule.event}, if {rule.guard.render(True)}, {outcome}.")
            lines.extend(f"Invariant {name}: {formula.render(True)}." for name, formula in self.invariants)
            lines.extend(f"Unresolved {kind}: {text}." for kind, text in self.obligations)
        else:
            lines = ["zal 1;", f"machine {self.name};", f"states {', '.join(self.states)};",
                     f"events {', '.join(self.events)};", f"context {', '.join(self.facts) or 'none'};",
                     f"initial {self.initial};", "step atomic; frame control_only;",
                     f"default reject {self.default} unchanged;"]
            for rule in self.rules:
                outcome = f"accept {rule.target}" if rule.target else f"reject {rule.reason} unchanged"
                lines.append(f"rule {rule.id}: {rule.source} + {rule.event} [{rule.guard.render()}] -> {outcome};")
            lines.extend(f"invariant {name}: {formula.render()};" for name, formula in self.invariants)
            lines.extend(f"unresolved {kind}: {text};" for kind, text in self.obligations)
        return "\n".join(lines) + "\n"

    def inputs(self):
        for state, event, values in itertools.product(self.states, self.events, itertools.product([False, True], repeat=len(self.facts))):
            yield state, event, dict(zip(self.facts, values))

    def enabled(self, state, event, context):
        return [r for r in self.rules if r.source == state and r.event == event and r.guard.value(state, context)]

    def decide(self, state, event, context):
        if not isinstance(state, str) or not isinstance(event, str) or not isinstance(context, dict) or state not in self.states or event not in self.events or set(context) != set(self.facts) or any(type(v) is not bool for v in context.values()):
            raise Refusal("Input outside the declared state/event/context domain")
        enabled = self.enabled(state, event, context)
        if len(enabled) > 1:
            raise Refusal("Overlapping transitions; no precedence is implied")
        if not enabled:
            return {"class": "Reject", "state": state, "reason": self.default, "rule": "default"}
        rule = enabled[0]
        return {"class": "Accept" if rule.target else "Reject", "state": rule.target or state,
                "reason": rule.reason, "rule": rule.id}


def canonical(value):
    return json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":"))


def validate_model(model: Model):
    """Validate direct IR construction as strictly as surface elaboration."""
    reserved = {"none", "true", "false", "state", "not", "and", "or", "is", "default"}
    def valid_name(value):
        return isinstance(value, str) and re.fullmatch(NAME, value) and value not in reserved
    if not valid_name(model.name) or not valid_name(model.default):
        raise Refusal("Invalid machine or default reason name")
    for label, values, maximum, minimum in [("states", model.states, 8, 1), ("events", model.events, 8, 1), ("facts", model.facts, 4, 0)]:
        if not isinstance(values, tuple) or not minimum <= len(values) <= maximum or any(not valid_name(x) for x in values) or tuple(sorted(set(values))) != values:
            raise Refusal(f"IR {label} must be bounded, distinct, canonically sorted owned names")
    if model.initial not in model.states:
        raise Refusal("Initial set is empty or outside the declared states")
    for values, maximum in [(model.rules, 32), (model.invariants, 8), (model.obligations, 8)]:
        if not isinstance(values, tuple) or len(values) > maximum:
            raise Refusal("IR collection is mutable or exceeds the profile bound")
    identifiers = []
    def formula(node, invariant=False, depth=1):
        if not isinstance(node, Formula) or depth > 16:
            raise Refusal("IR formula has an invalid type or depth")
        if node.op in {"and", "or"}:
            keys = [canonical(arg.data()) for arg in node.args]
            if any(arg.op == node.op for arg in node.args) or keys != sorted(set(keys)):
                raise Refusal("IR Boolean children must be flattened, distinct and canonically ordered")
        if node.op == "fact" and (invariant or node.args[0] not in model.facts) or node.op == "state" and node.args[0] not in model.states:
            raise Refusal("IR formula has an unbound reference")
        return 1 + sum(formula(arg, invariant, depth + 1) for arg in node.args if isinstance(arg, Formula))
    for rule in model.rules:
        if not isinstance(rule, Rule) or not valid_name(rule.id) or rule.source not in model.states or rule.event not in model.events:
            raise Refusal("Invalid IR transition")
        if (rule.target is None) == (rule.reason is None) or rule.target is not None and rule.target not in model.states or rule.reason is not None and not valid_name(rule.reason):
            raise Refusal("IR transition needs exactly one valid target or rejection reason")
        if formula(rule.guard) > 128 or any(len(rule.guard.render(e)) > 2048 for e in [False, True]):
            raise Refusal("IR guard exceeds canonical bounds")
        identifiers.append(rule.id)
    if [r.id for r in model.rules] != sorted(r.id for r in model.rules):
        raise Refusal("IR transition IDs must be canonically sorted")
    for item in model.invariants:
        if not isinstance(item, tuple) or len(item) != 2 or not valid_name(item[0]) or formula(item[1], True) > 128 or any(len(item[1].render(e)) > 2048 for e in [False, True]):
            raise Refusal("Invalid IR invariant")
        identifiers.append(item[0])
    if len(set(identifiers)) != len(identifiers) or [n for n, _ in model.invariants] != sorted(n for n, _ in model.invariants):
        raise Refusal("IR object IDs are duplicated or unordered")
    for item in model.obligations:
        if not isinstance(item, tuple) or len(item) != 2 or item[0] not in FUTURE or not isinstance(item[1], str) or not re.fullmatch(r"[a-zA-Z0-9_ ,!?()/-]{1,200}", item[1]):
            raise Refusal("Invalid unresolved obligation")
    if tuple(sorted(model.obligations)) != model.obligations:
        raise Refusal("Unresolved obligations must be canonically sorted")
    if any(len(model.render(english).encode("ascii")) > MAX_SOURCE for english in [False, True]):
        raise Refusal("Canonical symbolic or English model exceeds 64 KiB")


def grammar_patterns(english=False):
    """The parser's active statement registry, also used by the live legend."""
    return {
        "header": r"ZAL version 1\." if english else r"zal 1;",
        "name": rf"The machine is ({NAME})\." if english else rf"machine ({NAME});",
        "states": r"The states are (.+)\." if english else r"states (.+);",
        "events": r"The events are (.+)\." if english else r"events (.+);",
        "facts": r"The context observations are (.+)\." if english else r"context (.+);",
        "initial": rf"Initially the state is ({NAME})\." if english else rf"initial ({NAME});",
        "frame": r"Each step is atomic\. Only the control state may change\." if english else r"step atomic; frame control_only;",
        "default": rf"Otherwise reject because ({NAME}) and keep the state unchanged\." if english else rf"default reject ({NAME}) unchanged;",
    }


def legend(model: Model):
    patterns = grammar_patterns()
    entries = []
    for symbolic, english in zip(model.render().splitlines(), model.render(True).splitlines()):
        kind = next((key for key, pattern in patterns.items() if re.fullmatch(pattern, symbolic)), symbolic.split()[0])
        entries.append({"concept": kind, "symbolic": symbolic, "english": english})
    return {"profile": PROFILE, "entries": entries,
            "operators": [{"symbol": "!", "english": "not"}, {"symbol": "&", "english": "and"},
                          {"symbol": "|", "english": "or"}, {"symbol": "state == name", "english": "state is name"}],
            "unsupported": sorted(FUTURE), "identifiers": "ASCII lowercase letters, digits and underscores; max 32 characters"}


def help_topic(model: Model, topic="", renderer_identity="unbound"):
    """Resolve canonical language/object help without interpreting free prose."""
    if not isinstance(topic, str) or len(topic) > 128:
        raise Refusal("Help topic must be text of at most 128 characters")
    topic = topic.strip()
    pairs = legend(model)["entries"]
    concepts = {
        "header": "The language version selects this finite-FSM grammar and semantics.",
        "name": "The machine name is part of the canonical model identity.",
        "states": "Exactly one declared control-state label is current at a time.",
        "events": "Events are input labels; their ordering is supplied by the caller.",
        "facts": "Context facts are fresh Boolean observations on every step. Naming a fact does not authenticate it.",
        "initial": "The declared initial state is the single start of finite reachability checking.",
        "frame": "An Accept may change only the control state. Reject preserves it. This profile has no effects.",
        "default": "If no rule is enabled, the explicit default rejects and preserves the control state.",
        "rule": "A rule requires its source state, event and guard together. Source order gives no precedence; overlap is refused.",
        "invariant": "An invariant constrains control-state labels. Reachable safety and all raw Accept successors are checked separately.",
        "unresolved": "This records an unsupported requirement. It remains visible and blocks factory lowering.",
    }
    aliases = {"machine": "name", "state": "states", "event": "events", "context": "facts",
               "rules": "rule", "invariants": "invariant", "not": "!", "and": "&", "or": "|", "is": "=="}
    operators = {
        "!": ("Boolean NOT", "True exactly when its one Boolean operand is false.", Formula("not", (Formula("true"),))),
        "&": ("Boolean AND", "True exactly when every Boolean operand is true.", combine("and", [Formula("true"), Formula("state", (model.initial,))])),
        "|": ("Boolean OR", "True exactly when at least one Boolean operand is true.", combine("or", [Formula("false"), Formula("state", (model.initial,))])),
        "==": ("Control-state equality", "Compares the current control label with one declared state.", Formula("state", (model.initial,))),
        "true": ("Boolean true", "The constant true predicate.", Formula("true")),
        "false": ("Boolean false", "The constant false predicate.", Formula("false")),
    }
    syntax_topics = {"+", "->", "accept", "reject"}
    objects = {**{"state:" + name: "states" for name in model.states},
               **{"event:" + name: "events" for name in model.events},
               **{"context:" + name: "facts" for name in model.facts},
               **{"rule:" + rule.id: "rule" for rule in model.rules},
               **{"invariant:" + name: "invariant" for name, _ in model.invariants},
               "initial": "initial", "default": "default", "frame": "frame"}
    notes = ["Help describes the current canonical model, not arbitrary editor text or source spans.",
             "Context values are assumptions, not authenticated external truth."]
    if not model.invariants:
        notes.append("No application invariants declared. Consistency checks do not establish an unstated authorization or safety policy.")
    entries, title, meaning, kind = [], "ZAL language help", "One finite typed model has exact symbolic and controlled-English views.", "grammar"
    resolved = topic
    explicit_grammar = topic.startswith("grammar:")
    if explicit_grammar:
        topic = topic.removeprefix("grammar:")
    if topic and topic not in objects and not explicit_grammar:
        matches = [key for key in objects if key.partition(":")[2] == topic]
        if len(matches) > 1 or matches and (topic in concepts or topic in operators or topic in aliases or topic in syntax_topics):
            raise Refusal("Ambiguous help topic; use a typed object ID or grammar:TOPIC")
        if matches:
            topic = matches[0]
    if topic in objects and not explicit_grammar:
        kind, resolved = "object", topic
        category = objects[topic]
        title, meaning = "Meaning of " + topic, concepts[category]
        name = topic.partition(":")[2]
        if category in {"rule", "invariant"}:
            entries = [entry for entry in pairs if entry["symbolic"].startswith(category + " " + name + ":")]
        else:
            entries = [entry for entry in pairs if entry["concept"] == category]
    elif topic:
        resolved = aliases.get(topic, topic)
        if resolved in operators:
            title, meaning, formula = operators[resolved]
            entries = [{"symbolic": formula.render(), "english": formula.render(True)}]
            notes.append("Operator help applies inside predicates. Words such as 'and' and 'is' also occur as fixed grammar text in rule headers.")
            notes.append("Boolean operators are total and eager in this profile; this says nothing about future arithmetic/traps.")
        elif resolved in syntax_topics:
            title = "Rule syntax: " + resolved
            meaning = {"+": "Separates a rule's source state and input event; it is not addition.",
                       "->": "Separates a guard from its complete outcome; it is not logical implication.",
                       "accept": "Sets the control state to the declared target; it grants no publication authority.",
                       "reject": "Returns a declared reason and keeps the control state unchanged."}[resolved]
            entries = [entry for entry in pairs if entry["concept"] in {"rule", "default"}][:2]
            notes.append("The terminal's accept command is a separate human review action, not a rule outcome.")
        elif resolved in concepts:
            title, meaning = "Language concept: " + resolved, concepts[resolved]
            entries = [entry for entry in pairs if entry["concept"] == resolved]
        else:
            raise Refusal("Unknown help topic; use ? for topics or an exact typed object ID")
    else:
        resolved = "overview"
        notes.append("Use ? TOPIC or man TOPIC. For a concrete input use why STATE EVENT fact=yes.")
    return {"kind": "language-help", "schema": "zal/help/1", "profile": PROFILE,
            "revision": model.revision, "renderer": renderer_identity, "topic": resolved,
            "entry_kind": kind, "title": title, "meaning": meaning, "entries": entries,
            "notes": notes, "available_topics": sorted(set(concepts) | set(operators) | set(objects) | {"+", "->", "accept", "reject"}),
            "method": "built-in grammar/model metadata; no language model", "authority": "none"}


def parse(text: str, syntax="symbolic") -> Model:
    if not isinstance(text, str) or len(text.encode()) > MAX_SOURCE or not text.isascii():
        raise Refusal("Source must be ASCII text at most 64 KiB")
    if syntax not in {"symbolic", "english"}:
        raise Refusal("Choose symbolic or english syntax")
    english = syntax == "english"
    lines = [line.strip() for line in text.splitlines() if line.strip() and not line.lstrip().startswith("#")]
    if len(lines) > 80:
        raise Refusal("Source exceeds 80 statements")
    patterns = grammar_patterns(english)
    fields = {}
    raw_rules, raw_invariants, obligations = [], [], []
    terminator = r"\." if english else ";"
    for number, line in enumerate(lines, 1):
        for key, pattern in patterns.items():
            match = re.fullmatch(pattern, line)
            if match:
                if key in fields:
                    raise Refusal(f"Statement {number}: duplicate {key}")
                fields[key] = match.group(1) if match.groups() else True
                break
        else:
            pattern = (rf"Rule ({NAME}): when the state is ({NAME}) and the event is ({NAME}), if (.+), (.+)\." if english else
                       rf"rule ({NAME}): ({NAME}) \+ ({NAME}) \[(.+)\] -> (.+);")
            match = re.fullmatch(pattern, line)
            if match:
                raw_rules.append(match.groups())
                continue
            match = re.fullmatch(rf"{'Invariant' if english else 'invariant'} ({NAME}): (.+){terminator}", line)
            if match:
                raw_invariants.append(match.groups())
                continue
            match = re.fullmatch(rf"{'Unresolved' if english else 'unresolved'} ({NAME}): ([a-zA-Z0-9_ ,!?()/-]{{1,200}}){terminator}", line)
            if match and match[1] in FUTURE:
                obligations.append(match.groups())
                continue
            raise Refusal(f"Statement {number}: not in the controlled grammar; clarify this statement")
    missing = set(patterns) - set(fields)
    if missing:
        raise Refusal("Missing explicit declarations: " + ", ".join(sorted(missing)))

    def names(key, maximum, empty=False):
        raw = [] if empty and fields[key] == "none" else [x.strip() for x in fields[key].split(",")]
        if not (0 if empty else 1) <= len(raw) <= maximum or len(set(raw)) != len(raw) or any(not re.fullmatch(NAME, x) for x in raw):
            raise Refusal(f"{key} needs {'0' if empty else '1'}..{maximum} distinct names")
        return tuple(sorted(raw))

    states, events, facts = names("states", 8), names("events", 8), names("facts", 4, True)
    reserved = {"none", "true", "false", "state", "not", "and", "or", "is", "default"}
    if reserved & (set(states) | set(events) | set(facts)):
        raise Refusal("A declaration uses a reserved word")
    if fields["initial"] not in states:
        raise Refusal("Initial state is undeclared or empty")
    if len(raw_rules) > 32 or len(raw_invariants) > 8 or len(obligations) > 8:
        raise Refusal("Bounds: 32 rules, 8 invariants, 8 unresolved obligations")
    rules = []
    for rid, source, event, guard, outcome in raw_rules:
        if source not in states or event not in events:
            raise Refusal(f"Rule {rid} refers to an undeclared state or event")
        accepted = re.fullmatch(rf"{'move to' if english else 'accept'} ({NAME})", outcome)
        rejected = re.fullmatch(rf"reject {'because ' if english else ''}({NAME}) {'and keep the state unchanged' if english else 'unchanged'}", outcome)
        if not accepted and not rejected:
            raise Refusal(f"Rule {rid} needs an explicit outcome and frame")
        target = accepted[1] if accepted else None
        if target is not None and target not in states:
            raise Refusal(f"Rule {rid} targets an undeclared state")
        rules.append(Rule(rid, source, event, parse_formula(guard, facts, states, english), target, rejected[1] if rejected else None))
    identifiers = [r.id for r in rules] + [name for name, _ in raw_invariants]
    if len(set(identifiers)) != len(identifiers) or "default" in identifiers:
        raise Refusal("Rule and invariant IDs must be distinct and not default")
    invariants = tuple(sorted((name, parse_formula(formula, (), states, english, invariant=True)) for name, formula in raw_invariants))
    return Model(fields["name"], states, events, facts, fields["initial"], fields["default"],
                 tuple(sorted(rules, key=lambda r: r.id)), invariants, tuple(sorted(obligations)))


def witness(state, event, context):
    return {"state": state, "event": event, "context": context}


def check(model: Model, checker_identity="unbound") -> dict:
    rows, defaults = [], []
    metadata = {"invariant_count": len(model.invariants), "advisories": [] if model.invariants else [
        {"kind": "no-application-invariants", "message": "No application invariants declared; no unstated application safety or authorization policy has been proved."}]}
    checks = {"initial_nonempty": "pass-with-scope", "typing": "pass-with-scope",
              "determinism": "not-run", "coverage_with_explicit_default": "not-run",
              "reachable_invariants": "not-run", "raw_commit_invariants": "not-run"}
    for state, event, context in model.inputs():
        enabled = model.enabled(state, event, context)
        if len(enabled) > 1:
            return {**metadata, "status": "counterexample", "revision": model.revision, "profile": PROFILE,
                    "checker": checker_identity, "obligation": "determinism",
                    "witness": witness(state, event, context), "rules": [r.id for r in enabled],
                    "checks": {**checks, "determinism": "counterexample"}}
        result = model.decide(state, event, context)
        row = {**witness(state, event, context), "outcome": result}
        rows.append(row)
        if result["rule"] == "default":
            defaults.append(row)
    checks.update(determinism="pass-with-scope", coverage_with_explicit_default="pass-with-scope")
    paths = {model.initial: []}
    todo = [model.initial]
    while todo:
        state = todo.pop(0)
        for name, formula in model.invariants:
            if not formula.value(state, {}):
                return {**metadata, "status": "counterexample", "revision": model.revision, "profile": PROFILE,
                        "checker": checker_identity, "obligation": "invariant", "invariant": name,
                        "witness": {"state": state, "trace": paths[state]},
                        "checks": {**checks, "reachable_invariants": "counterexample"}}
        for row in rows:
            if row["state"] == state:
                target = row["outcome"]["state"]
                if target not in paths:
                    paths[target] = [*paths[state], row]
                    todo.append(target)
    used = {row["outcome"]["rule"] for row in rows if row["state"] in paths}
    checks["reachable_invariants"] = "pass-with-scope"
    for row in rows:
        if row["outcome"]["class"] == "Accept":
            for name, formula in model.invariants:
                if not formula.value(row["outcome"]["state"], {}):
                    return {**metadata, "status": "counterexample", "revision": model.revision, "profile": PROFILE,
                            "checker": checker_identity, "obligation": "invariant-preservation",
                            "invariant": name, "witness": row,
                            "checks": {**checks, "raw_commit_invariants": "counterexample"}}
    checks["raw_commit_invariants"] = "pass-with-scope"
    advisories = list(metadata["advisories"])
    possible_rules = {row["outcome"]["rule"] for row in rows}
    for rule in model.rules:
        if rule.id not in possible_rules:
            advisories.append({"kind": "guard-never-true-at-source", "rule": rule.id,
                               "message": f"Rule {rule.id}: guard is never true at its declared source across all Boolean context valuations."})
        if rule.source not in paths:
            advisories.append({"kind": "unreachable-source", "rule": rule.id,
                               "message": f"Rule {rule.id}: its source state is unreachable from this initial state under the declared finite model."})
    for name, formula in model.invariants:
        if all(formula.value(state, {}) for state in model.states):
            advisories.append({"kind": "invariant-domain-tautology", "invariant": name,
                               "message": f"Invariant {name} is true on every declared control state and excludes none of them."})
    return {"status": "unsupported" if model.obligations else "pass-with-scope",
            "revision": model.revision, "profile": PROFILE, "checker": checker_identity,
            "scope": "Finite atomic control-state safety; context ranges over fresh Boolean inputs on every step",
            "checks": checks,
            "input_tuples": len(rows), "reachable_states": sorted(paths),
            "unreachable_rules": sorted({r.id for r in model.rules} - used),
            "invariant_count": len(model.invariants), "advisories": advisories,
            "default_count": len(defaults), "default_witness": defaults[0] if defaults else None,
            "unresolved": list(model.obligations), "factory": "not-run", "authority": "none"}


def semantic_diff(old: Model, new: Model) -> dict:
    domain_same = (old.states, old.events, old.facts) == (new.states, new.events, new.facts)
    report = {"domain_same": domain_same, "changed_objects": [], "witnesses": [],
              "observations": ["business class", "control state", "reason", "initial state"],
              "excluded_observations": ["Step usage", "budget refusals", "publication identity", "delivery"],
              "old_revision": old.revision, "new_revision": new.revision}
    a = {r.id: r.data() for r in old.rules}
    b = {r.id: r.data() for r in new.rules}
    report["changed_objects"] = sorted(key for key in a.keys() | b.keys() if a.get(key) != b.get(key))
    for key in ["initial", "default", "invariants", "obligations"]:
        if old.data()[key] != new.data()[key]:
            report["changed_objects"].append(key)
    if domain_same:
        if old.initial != new.initial:
            report["witnesses"].append({"kind": "initial-state", "trace": [],
                                        "before": old.initial, "after": new.initial})
        for state, event, context in old.inputs():
            try:
                before, after = old.decide(state, event, context), new.decide(state, event, context)
            except Refusal:
                report["comparison"] = "invalid-overlap"
                break
            if any(before[key] != after[key] for key in ["class", "state", "reason"]):
                report["witnesses"].append({**witness(state, event, context), "before": before, "after": after})
        report.setdefault("comparison", "different" if report["witnesses"] else "same-functional-observations-on-declared-domain")
    else:
        report["comparison"] = "domain-changed"
    return report


def replay(model: Model, inputs: list[dict]) -> list[dict]:
    if not isinstance(inputs, list):
        raise Refusal("Trace inputs must be an owned array")
    if len(inputs) > 64:
        raise Refusal("Trace exceeds 64 steps")
    state = model.initial
    trace = []
    for item in inputs:
        if not isinstance(item, dict) or set(item) != {"event", "context"}:
            raise Refusal("A trace step needs exactly event and context")
        outcome = model.decide(state, item["event"], item["context"])
        trace.append({"pre": state, "event": item["event"], "context": dict(item["context"]), "outcome": outcome})
        state = outcome["state"]
    return trace


def explain(model: Model, state: str, event: str, context: dict[str, bool], checker_identity="unbound") -> dict:
    """Explain one exact decision using only the admitted Boolean AST."""
    outcome = model.decide(state, event, context)  # Validate the complete original input first.
    def tree(formula):
        node = {"operator": formula.op, "value": formula.value(state, context)}
        if formula.op == "fact":
            node.update(name=formula.args[0], observed=context[formula.args[0]])
        elif formula.op == "state":
            node.update(expected=formula.args[0], observed=state)
        else:
            node["operands"] = [tree(child) for child in formula.args]
        return node
    rules = []
    for rule in model.rules:
        guard = tree(rule.guard)
        state_matches, event_matches = rule.source == state, rule.event == event
        rules.append({"id": rule.id, "source": rule.source, "event": rule.event,
                      "state_matches": state_matches, "event_matches": event_matches,
                      "guard_english": rule.guard.render(True), "guard": guard,
                      "enabled": state_matches and event_matches and guard["value"]})
    result = {"kind": "decision-explanation", "profile": PROFILE, "revision": model.revision,
              "checker": checker_identity, "input": witness(state, event, dict(context)),
              "outcome": outcome, "rules": rules, "used_default": outcome["rule"] == "default",
              "frame": "control unchanged" if outcome["class"] == "Reject" else "only control state may change",
              "effects": "none in this profile", "default_reason": model.default,
              "scope": "Deterministic explanation of one supplied input; context facts are assumptions, not authenticated external truth.",
              "method": "typed AST evaluation; no language model", "authority": "none"}
    if len(canonical(result).encode()) > 512 * 1024:
        raise Refusal("Explanation exceeds the 512 KiB output bound")
    return result
