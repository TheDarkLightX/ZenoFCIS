"""Construct registry for zal/finite-fsm/1 and the zal/help/2 guide generated from it.

Each construct's meaning is written once, here, beside its forms. Forms backed
by the parser's statement registry are derived from `grammar_patterns`; the
drift test binds every other form to the live parser. No language model.
"""

from __future__ import annotations

from dataclasses import dataclass
import re

from behavior import FUTURE, NAME, PROFILE, Formula, Refusal, grammar_patterns, legend


GUIDE_SCHEMA = "zal/help/2"


@dataclass(frozen=True)
class Construct:
    id: str
    kind: str  # statement, outcome, syntax, operator, atom or unsupported
    title: str
    meaning: str
    symbolic: str = ""
    english: str = ""
    slots: tuple = ()  # pattern-backed: one slot per capture group, in order
    aliases: tuple = ()
    formula_op: str | None = None


STATEMENTS = (
    Construct("header", "statement", "Language version",
              "The language version selects this finite-FSM grammar and semantics."),
    Construct("name", "statement", "Machine name",
              "The machine name is part of the canonical model identity.", slots=("NAME",), aliases=("machine",)),
    Construct("states", "statement", "Control states",
              "Exactly one declared control-state label is current at a time.", slots=("NAME, NAME",), aliases=("state",)),
    Construct("events", "statement", "Events",
              "Events are input labels; their ordering is supplied by the caller.", slots=("NAME, NAME",), aliases=("event",)),
    Construct("facts", "statement", "Context observations",
              "Context facts are fresh Boolean observations on every step. Naming a fact does not authenticate it.",
              slots=("NAME, NAME",), aliases=("context",)),
    Construct("initial", "statement", "Initial state",
              "The declared initial state is the single start of finite reachability checking.", slots=("STATE",)),
    Construct("frame", "statement", "Step and frame",
              "An Accept may change only the control state. Reject preserves it. This profile has no effects."),
    Construct("default", "statement", "Default rejection",
              "If no rule is enabled, the explicit default rejects and preserves the control state.", slots=("REASON",)),
    Construct("rule", "statement", "Rule",
              "A rule requires its source state, event and guard together. Source order gives no precedence; overlap is refused.",
              "rule ID: STATE + EVENT [GUARD] -> OUTCOME;",
              "Rule ID: when the state is STATE and the event is EVENT, if GUARD, OUTCOME.", aliases=("rules",)),
    Construct("invariant", "statement", "Invariant",
              "An invariant constrains control-state labels. Reachable safety and all raw Accept successors are checked separately.",
              "invariant ID: STATE_FORMULA;", "Invariant ID: STATE_FORMULA.", aliases=("invariants",)),
    Construct("unresolved", "statement", "Unresolved requirement",
              "This records an unsupported requirement. It remains visible and blocks factory lowering.",
              "unresolved KIND: TEXT;", "Unresolved KIND: TEXT."),
)

OUTCOMES = (
    Construct("accept", "outcome", "Rule outcome: accept",
              "Sets the control state to the declared target; it grants no publication authority.",
              "accept TARGET", "move to TARGET"),
    Construct("reject", "outcome", "Rule outcome: reject",
              "Returns a declared reason and keeps the control state unchanged.",
              "reject REASON unchanged", "reject because REASON and keep the state unchanged"),
)

SYNTAX = (
    Construct("+", "syntax", "Rule syntax: +",
              "Separates a rule's source state and input event; it is not addition.",
              "STATE + EVENT", "when the state is STATE and the event is EVENT"),
    Construct("->", "syntax", "Rule syntax: ->",
              "Separates a guard from its complete outcome; it is not logical implication.",
              "[GUARD] -> OUTCOME", "if GUARD, OUTCOME"),
)

OPERATORS = (
    Construct("!", "operator", "Boolean NOT", "True exactly when its one Boolean operand is false.",
              "!P", "not P", aliases=("not",), formula_op="not"),
    Construct("&", "operator", "Boolean AND", "True exactly when every Boolean operand is true.",
              "P & Q", "P and Q", aliases=("and",), formula_op="and"),
    Construct("|", "operator", "Boolean OR", "True exactly when at least one Boolean operand is true.",
              "P | Q", "P or Q", aliases=("or",), formula_op="or"),
    Construct("==", "operator", "Control-state equality",
              "Compares the current control label with one declared state.",
              "state == STATE", "state is STATE", aliases=("is",), formula_op="state"),
    Construct("true", "operator", "Boolean true", "The constant true predicate.", "true", "true", formula_op="true"),
    Construct("false", "operator", "Boolean false", "The constant false predicate.", "false", "false", formula_op="false"),
    Construct("fact", "atom", "Context fact in a guard",
              "A declared context fact used as a predicate: true exactly when this step's supplied observation is true. "
              "Invariants cannot read facts.", "FACT", "FACT", formula_op="fact"),
)

UNSUPPORTED = {
    "effects": "External effects such as payments, messages or storage writes.",
    "liveness": "Requirements that something eventually happens.",
    "fairness": "Assumptions that enabled events are eventually taken.",
    "arithmetic": "Integer quantities, counters and their overflow rules.",
    "concurrency": "Steps that overlap or interleave.",
}

REGISTRY = (*STATEMENTS, *OUTCOMES, *SYNTAX, *OPERATORS, *(
    Construct(kind, "unsupported", "Unsupported in this profile: " + kind,
              UNSUPPORTED[kind] + " Not expressible in this profile: record it as an unresolved requirement. "
              "It stays visible, keeps the check status unsupported and blocks factory lowering.",
              f"unresolved {kind}: TEXT;", f"Unresolved {kind}: TEXT.") for kind in sorted(FUTURE)))

BY_ID = {construct.id: construct for construct in REGISTRY}
ALIASES = {alias: construct.id for construct in REGISTRY for alias in construct.aliases}
OBJECT_CONSTRUCT = {"state": "states", "event": "events", "context": "facts", "rule": "rule", "invariant": "invariant"}


def pattern_form(pattern: str, slots: tuple) -> str:
    """Turn one parser statement pattern into its readable form, slot by slot."""
    groups = re.findall(r"\((?!\?)", pattern)
    if len(groups) != len(slots):
        raise Refusal("Construct registry slots drifted from the parser pattern")
    form = pattern
    for slot in slots:
        start = form.index("(")
        depth, end = 0, start
        for end in range(start, len(form)):
            depth += {"(": 1, ")": -1}.get(form[end], 0)
            if depth == 0:
                break
        form = form[:start] + slot + form[end + 1:]
    return re.sub(r"\\(.)", r"\1", form)


def forms(construct: Construct) -> dict:
    """Symbolic and English forms; statement-registry constructs are derived, not stored."""
    if construct.kind == "statement" and construct.id in grammar_patterns():
        return {"symbolic": pattern_form(grammar_patterns()[construct.id], construct.slots),
                "english": pattern_form(grammar_patterns(True)[construct.id], construct.slots)}
    return {"symbolic": construct.symbolic, "english": construct.english}


def formula_ops(formula: Formula) -> set:
    return {formula.op}.union(*(formula_ops(arg) for arg in formula.args if isinstance(arg, Formula)))


def uses(model, construct: Construct) -> list:
    """Typed object IDs whose formulas contain this operator or atom."""
    if construct.formula_op is None:
        return []
    found = [("rule:" + rule.id) for rule in model.rules if construct.formula_op in formula_ops(rule.guard)]
    return found + [("invariant:" + name) for name, formula in model.invariants if construct.formula_op in formula_ops(formula)]


def example(model, construct: Construct):
    """One small formula built from the current model's names; never free text."""
    state = Formula("state", (model.initial,))
    fact = Formula("fact", (model.facts[0],)) if model.facts else None
    nodes = {"not": Formula("not", (state,)), "state": state, "true": Formula("true"), "false": Formula("false"),
             "and": Formula("and", (fact, state)) if fact else None, "or": Formula("or", (fact, state)) if fact else None,
             "fact": fact}
    node = nodes.get(construct.formula_op)
    return None if node is None else {"symbolic": node.render(), "english": node.render(True)}


def guide(model, topic="", renderer_identity="unbound") -> dict:
    """Resolve help like zal/help/1, but assemble every answer from the registry."""
    if not isinstance(topic, str) or len(topic) > 128:
        raise Refusal("Help topic must be text of at most 128 characters")
    topic = topic.strip()
    pairs = legend(model)["entries"]
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
    explicit_grammar = topic.startswith("grammar:")
    if explicit_grammar:
        topic = topic.removeprefix("grammar:")
    if topic and topic not in objects and not explicit_grammar:
        matches = [key for key in objects if key.partition(":")[2] == topic]
        if len(matches) > 1 or matches and (topic in BY_ID or topic in ALIASES):
            raise Refusal("Ambiguous help topic; use a typed object ID or grammar:TOPIC")
        if matches:
            topic = matches[0]
    result = {"kind": "language-help", "schema": GUIDE_SCHEMA, "profile": PROFILE, "revision": model.revision,
              "renderer": renderer_identity}
    if topic in objects and not explicit_grammar:
        construct = BY_ID[objects[topic]]
        name = topic.partition(":")[2]
        if construct.id in {"rule", "invariant"}:
            entries = [entry for entry in pairs if entry["symbolic"].startswith(construct.id + " " + name + ":")]
        else:
            entries = [entry for entry in pairs if entry["concept"] == construct.id]
        result.update(topic=topic, entry_kind="object", construct=construct.id, title="Meaning of " + topic)
    elif topic:
        resolved = ALIASES.get(topic, topic)
        if resolved not in BY_ID:
            raise Refusal("Unknown help topic; use guide for topics or an exact typed object ID")
        construct = BY_ID[resolved]
        if construct.kind in {"statement", "unsupported"}:
            concept = "unresolved" if construct.kind == "unsupported" else construct.id
            entries = [entry for entry in pairs if entry["concept"] == concept and
                       (construct.kind != "unsupported" or entry["symbolic"].startswith(f"unresolved {construct.id}:"))]
        elif construct.kind in {"outcome", "syntax"}:
            entries = [entry for entry in pairs if entry["concept"] in {"rule", "default"}][:2]
        else:
            entries = [example(model, construct)] if example(model, construct) else []
        result.update(topic=resolved, entry_kind=construct.kind, construct=construct.id, title=construct.title)
    else:
        result.update(topic="overview", entry_kind="overview", construct=None, title="ZAL construct guide",
                      meaning="One finite typed model has exact symbolic and controlled-English views.",
                      forms=None, used_in=[], entries=[],
                      constructs=[{"id": c.id, "kind": c.kind, "title": c.title, **forms(c)} for c in REGISTRY])
        notes.append("Use guide TOPIC for one construct or an exact typed object ID for one declared object.")
    if result["entry_kind"] != "overview":
        result.update(meaning=construct.meaning, forms=forms(construct), used_in=uses(model, construct), entries=entries)
    return {**result, "notes": notes,
            "available_topics": sorted(set(BY_ID) | set(objects)),
            "method": "generated from the construct registry, the parser's statement patterns and the current model; no language model",
            "authority": "none"}


def human_text(result):
    lines = [f"{result['title']} | {result['schema']} | revision {result['revision'][:12]}", result["meaning"]]
    if result["forms"]:
        lines.extend(["Symbolic form: " + result["forms"]["symbolic"], "English form: " + result["forms"]["english"]])
    for entry in result["entries"]:
        lines.extend([entry["english"], "  Symbolic: " + entry["symbolic"]])
    if result["used_in"]:
        lines.append("Used in: " + ", ".join(result["used_in"]))
    for item in result.get("constructs", []):
        lines.append(f"{item['id']:<12} {item['kind']:<12} {item['symbolic']}  |  {item['english']}")
    lines.extend(result["notes"])
    lines.append("Generated from the construct registry; no LLM, agreement or production authority.")
    return "\n".join(lines)
