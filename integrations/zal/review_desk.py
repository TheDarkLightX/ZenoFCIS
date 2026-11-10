"""Read-only review desk for the finite ZAL profiles. It shows ZAL's own results and records no agreement.

    python3 integrations/zal/review_desk.py serve SOURCE.zal [--port 8770]
    python3 integrations/zal/review_desk.py snapshot SOURCE.zal [MORE.zal ...] --out PAGE.html
    python3 integrations/zal/review_desk.py data SOURCE.zal

`serve` opens one source file on a loopback page and notices when the file changes on disk. `snapshot`
writes one self-contained page with every result computed beforehand. `data` prints what the page reads.

Every status, sentence, decision and comparison on the page comes from the modules run.py dispatches to:
parse, check, legend, explain, why-not, the counterexample story and the semantic diff. The desk adds four
things, and the page labels each as the desk's own: planted rule edits that ZAL re-checks, the breakdown of
which claim ZAL names for each edit, a walk of ZAL's decisions from the initial configuration, and each line
of the statement spelled in the symbols of ZAL's ladder, which ZAL's renderer, parser and evaluator check.

The desk never writes a source, a workspace or an agreement, and it has no accept, export or provider route.
This file and review_desk.html are outside the source closure that workflow.checker_identity hashes.
"""
from __future__ import annotations

import argparse
import hashlib
import itertools
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import re
import secrets
import sys
import threading
import time
from types import SimpleNamespace
from urllib.parse import urlsplit

import behavior
import behavior2
import constructs
import diagnostics
import explain
import explain2
import json_numbers
import ladder
from behavior import Refusal
from workflow import checker_identity

MAX_SOURCE = behavior2.MAX_SOURCE   # ZAL's own source bound
MAX_TABLE_TUPLES = 2500             # a page holds a decision for every input up to this many input tuples
MAX_NEAREST_TUPLES = 600            # and the nearest opposite decision for every input up to this many
MAX_BREAKDOWN_TUPLES = 4096         # above this the planted edits are not broken down by claim
MAX_WITNESSES = 8                   # distinguishing inputs kept for one compared source
SENTENCE_SAMPLES = 12               # inputs on which the composed decision sentence is compared with why-not
MAX_FORMAL_INPUTS = 20000           # declared inputs on which every rule's step formula is evaluated
PAGE = Path(__file__).with_name("review_desk.html")
DATA_SLOT, TOKEN_SLOT = "/*__ZAL_REVIEW_DESK_DATA__*/null", "__ZAL_REVIEW_DESK_TOKEN__"
OTHER_PROFILES = frozenset({"zal effects 1;", "ZAL effects version 1.", "zal typed 1;", "ZAL typed version 1."})
STRUCTURAL_CHECKS = frozenset({"initial_nonempty", "typing", "determinism", "update_ranges",
                               "coverage_with_explicit_default"})
CLAIM_CONCEPTS = ("require", "property", "game")
PASSING = ("pass-with-scope", "unsupported")
INVARIANT_OBLIGATIONS = ("invariant", "invariant-preservation")
NAMED = re.compile(r"(?:rule|invariant|require|property|fairness|player|symmetry) (\w+)")
RULE_FIELDS = ("state_matches", "event_matches", "enabled", "guard", "updates")
JSON_TYPE = "application/json"
REQUEST_BOUND = 2 * MAX_SOURCE + 4096   # a candidate source as a JSON string, with room for its escapes
RESPONSE_HEADERS = (("Cache-Control", "no-store"), ("X-Content-Type-Options", "nosniff"), ("Referrer-Policy", "no-referrer"),
                    ("Content-Security-Policy", "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; "
                                                "connect-src 'self'; img-src data:; base-uri 'none'; form-action 'none'; "
                                                "frame-ancestors 'none'"))


class Disagreement(Refusal):
    """A line the desk words or spells itself no longer agrees with ZAL: the desk's own error, never ZAL refusing a source."""


# ---- ZAL's own operations, selected the way run.py selects them -----------------------------------------------------

def read_statement(text):
    """Parse one finite-profile source; returns the model and the module of its profile."""
    first = ""
    if isinstance(text, str):
        first = next((line.strip() for line in text.splitlines() if line.strip() and not line.lstrip().startswith("#")), "")
    if first in OTHER_PROFILES:
        raise Refusal("The review desk reads the finite profiles zal/finite-fsm/1, 2 and 3; "
                      "this source is a typed or effects profile")
    syntax = "english" if first.startswith("ZAL ") and first.endswith(".") else "symbolic"
    model = behavior2.parse_any(text, syntax)
    return model, (behavior2 if isinstance(model, behavior2.Machine) else behavior)


def explain_input(model, profile, given, checker):
    if profile is behavior2:
        return profile.explain(model, given["state"], given["variables"], given["event"], given["context"], checker)
    return profile.explain(model, given["state"], given["event"], given["context"], checker)


def why_not(model, profile, given, want, scope, checker):
    if profile is behavior2:
        return explain2.why_not(model, given["state"], given["variables"], given["event"], given["context"],
                                want, scope, checker)
    return explain.why_not(model, given["state"], given["event"], given["context"], want, scope, checker)


def input_tuples(model, profile):
    """Every declared input tuple, in ZAL's own enumeration order."""
    if profile is behavior2:
        for state, values in model.configurations():
            for event in model.events:
                for context in model.contexts():
                    yield {"state": state, "variables": dict(values), "event": event, "context": dict(context)}
    else:
        for state, event, context in model.inputs():
            yield {"state": state, "variables": {}, "event": event, "context": dict(context)}


def decision_sentence(model, profile, given):
    """ZAL's one-line account of a decision: the first line of its why-not answer.

    why_not scans the whole declared domain on every call, and a page needs this line for every input. So the
    line is composed here from the phrase functions why_not itself calls. check_sentences compares the two.
    """
    state, variables, event, context = given["state"], given["variables"], given["event"], given["context"]
    if profile is behavior2:
        current, overlap = explain2.outcome_of(model, state, variables, event, context)
        if overlap:
            raise Refusal("Overlapping transitions; no precedence is implied")
        return (f"From {explain2.configuration(state, variables)}, event {event} with "
                f"{explain2.observations(context)}: {explain2.decider_text(current)} {explain2.outcome_text(current)}.")
    current = model.decide(state, event, context)
    return (f"From {state}, event {event} with {explain.observations(context)}: "
            f"{explain.decider_text(current)} {explain.outcome_text(current)}.")


def check_sentences(model, profile, count, checker):
    """Compare decision_sentence with why-not on evenly spaced inputs; a difference refuses the statement."""
    stride, compared = max(1, count // SENTENCE_SAMPLES), 0
    for given in itertools.islice(input_tuples(model, profile), 0, None, stride):
        if compared == SENTENCE_SAMPLES:
            break
        try:
            expected = why_not(model, profile, given, {"class": "Accept"}, "context", checker)["summary"][0]
        except ValueError:                                             # overlapping rules: why-not gives no line
            continue
        compared += 1
        if decision_sentence(model, profile, given) != expected:
            raise Disagreement("The review desk's decision sentence no longer matches ZAL's why-not answer; "
                          "update review_desk.decision_sentence")
    return compared


def nearest_opposite(model, profile, given, outcome, checker):
    """ZAL's nearest declared input that is decided the other way."""
    want = {"class": "Reject" if outcome["class"] == "Accept" else "Accept"}
    answer = why_not(model, profile, given, want, "input", checker)
    first = answer["minimal_changes"][0] if answer.get("minimal_changes") else None
    return {"want": want["class"], "status": answer["status"], "distance": answer.get("distance"),
            "count": answer.get("minimal_count"),
            "first": {key: first[key] for key in ("changes", "input", "outcome")} if first else None}


def counterexample_story(model, report, checker):
    try:
        told = explain.narrate_check(model, report, checker)
    except (ValueError, KeyError, TypeError) as error:
        return {"status": "not-narrated", "reason": str(error)[:300]}
    return {key: told.get(key) for key in ("status", "obligation", "english", "symbolic", "method")}


# ---- formal symbols: the desk's own spelling of ZAL's lines ---------------------------------------------------------
# ZAL prints a finite-profile statement in English and in its typed syntax. Its ladder writes formulas with
# symbols. Here each line of a statement is spelled with those symbols: the symbols come from the ladder's
# registry, the structure from the formula ZAL parsed, and the reading and meaning of each symbol from ZAL's
# construct registries. check_formal compares the spelling with ZAL's renderer, parser and evaluator.
#
# A spelled line is a list of segments [text], [text, key] or [text, key, flag]. The key names the row of the
# reading key that explains the text; the flag is "main" for the connective that joins the two halves of a line
# and "note" for words that are not part of the formula.

TYPED = {"not": "!", "and": " & ", "or": " | ", "implies": " => ", "iff": " <=> ", "==": " == ", "!=": " != ",
         "<": " < ", "<=": " <= ", ">": " > ", ">=": " >= ", "add": " + ", "sub": " - ", "prime": "'"}
COMPARISON_KEY = {"==": "eq", "!=": "ne", "<": "lt", "<=": "le", ">": "gt", ">=": "ge"}
CONNECTIVES = ("and", "or", "implies", "iff")
ATOMS = ("true", "false", "accepted", "fact")
MEMBER, FORCES = "∈", ("⟨⟨", "⟩⟩")     # as ZAL's game readings write "can force"


def ladder_symbols():
    """ZAL's operators in the symbols of its ladder: what a formula template writes between or before operands."""
    formula, temporal = ladder.REGISTRY["templates"]["formula"], ladder.REGISTRY["temporal_legend"]
    between = {"and": "and", "or": "or", "implies": "=>", "iff": "<=>", "==": "=", "!=": "!=", "<": "<", "<=": "<=",
               ">": ">", ">=": ">=", "add": "+", "sub": "-"}
    symbols = {op: formula[name].replace("{a}", "").replace("{b}", "") for op, name in between.items()}
    symbols["not"] = formula["not"].replace("{a}", "")
    symbols["prime"] = re.sub(r"\{\w+\}|[()]", "", formula["prime_call"])
    symbols.update({op: temporal[op][0] for op in ("always", "eventually", "next", "until", "within")})
    if any(not text.strip() or "{" in text.replace("{k}", "") or "(" in text for text in symbols.values()):
        raise Disagreement("The ladder's formula templates no longer have the shape the review desk reads")
    return symbols


FORMAL = ladder_symbols()
OFTEN = FORMAL["always"] + FORMAL["eventually"]                  # "infinitely often", as ZAL's game readings write it


def _mark(text, key):
    """Segments for one operator: its symbol carries the key, the spaces around it do not."""
    core, lead, trail = text.strip(), text[:len(text) - len(text.lstrip())], text[len(text.rstrip()):]
    return [*([[lead]] if lead else []), [core, key], *([[trail]] if trail else [])]


def _bare(segments):
    """Without the one pair of parentheses that encloses the whole formula, if there is one, as ZAL renders operands."""
    depth = 0
    for index, segment in enumerate(segments):
        depth += (segment[0] == "(") - (segment[0] == ")")
        if depth == 0:
            return segments[1:-1] if index and index == len(segments) - 1 and segments[0][0] == "(" else segments
    return segments


def plain(segments):
    return "".join(segment[0] for segment in segments or [])


def spell(node, way, exact=False):
    """One formula as segments. With ZAL's typed spelling and exact=True the text is ZAL's own rendering.

    Without exact, a negation stands before its operand with no parentheses around the pair, and a comparison
    under it is parenthesized. ZAL's grammar reads that form as the same formula; check_formal has ZAL parse it back.
    """
    op, args, again = node.op, node.args, lambda child: spell(child, way, exact)
    if op in ("true", "false", "accepted"):
        return [[op, op]]
    if op in ("fact", "var", "input"):
        return [[args[0], "name:" + args[0]]]
    if op == "int":
        return [[str(args[0])]]
    if op == "var_next":
        return [[args[0], "name:" + args[0]], [way["prime"], "prime"]]
    if op in ("state", "state_next", "event"):
        word = "event" if op == "event" else "state"
        return [[word, word], *([[way["prime"], "prime"]] if op == "state_next" else []), *_mark(way["=="], "eq"), [args[0]]]
    if op == "not":
        inner = again(args[0])
        if exact:
            return [["("], [way["not"], "not"], *inner, [")"]]
        grouped = args[0].op in ATOMS or args[0].op in CONNECTIVES
        return [[way["not"], "not"], *(inner if grouped else [["("], *inner, [")"]])]
    if op in CONNECTIVES:
        out = [["("]]
        for index, child in enumerate(args):
            out += (_mark(way[op], op) if index else []) + again(child)
        return [*out, [")"]]
    if op == "cmp":
        return [*again(args[1]), *_mark(way[args[0]], COMPARISON_KEY[args[0]]), *again(args[2])]
    if op in ("add", "sub"):
        return [*again(args[0]), *_mark(way[op], op), *again(args[1])]
    operands = [_bare(again(arg)) for arg in args if not isinstance(arg, int)]
    if way is TYPED:                                                  # ZAL's own temporal templates
        if op == "within":
            return [[op, op], ["("], [str(args[0])], [", "], *operands[0], [")"]]
        if op == "until":
            return [[op, op], ["("], *operands[0], [", "], *operands[1], [")"]]
        return [[op, op], ["("], *operands[0], [")"]]
    if op == "within":
        return [[way[op].replace("{k}", str(args[0])), op], ["("], *operands[0], [")"]]
    if op == "until":
        return [["("], ["("], *operands[0], [")"], [" "], [way[op], op], [" "], ["("], *operands[1], [")"], [")"]]
    if op == "never":                                                 # the ladder: never(P) is always(not P)
        return [[way["always"], "always"], ["("], [way["not"], "not"], ["("], *operands[0], [")"], [")"]]
    return [[way[op], op], ["("], *operands[0], [")"]]


def whole(node, way=None):
    """One whole formula: no outer parentheses, and its top connective flagged as the main one."""
    segments, depth = _bare(spell(node, way or FORMAL)), 0
    for segment in segments:
        depth += (segment[0] == "(") - (segment[0] == ")")
        if depth == 0 and node.op in CONNECTIVES and segment[1:] == [node.op]:
            segment.append("main")
    return segments


def as_expr(formula):
    """A profile 1 formula as the profile 2 node of the same shape, so one parser and one evaluator read both."""
    if isinstance(formula, behavior2.Expr):
        return formula
    return behavior2.Expr(formula.op, tuple(as_expr(arg) if isinstance(arg, behavior.Formula) else arg for arg in formula.args))


def _sum(base, term, adding):
    """base + term or base - term as ZAL's left-nested sum, whatever shape the term has."""
    if term.op in ("add", "sub"):
        return _sum(_sum(base, term.args[0], adding), term.args[1], adding == (term.op == "add"))
    return behavior2.Expr("add" if adding else "sub", (base, term))


def rule_formula(model, rule):
    """One rule as a step formula: what holds before the step, what the step then is, and what it leaves alone.

    Three lists of formulas. ZAL refuses overlapping rules, so the first list holds on exactly the steps this
    rule decides, and on those steps the other two hold. check_formal evaluates that on the declared inputs.
    """
    node, guard = behavior2.Expr, as_expr(rule.guard)
    before = [node("state", (rule.source,)), node("event", (rule.event,)),
              *(guard.args if guard.op == "and" else [] if guard.op == "true" else [guard])]
    same = lambda name: node("cmp", ("==", node("var_next", (name,)), node("var", (name,))))
    names, updates = [variable.name for variable in getattr(model, "variables", ())], getattr(rule, "updates", ())
    if rule.target is None:
        return before, [node("not", (node("accepted"),)), node("state_next", (rule.source,))], [same(name) for name in names]
    then = [node("accepted"), node("state_next", (rule.target,))]
    for update in updates:
        after = update.term if update.op == "=" else _sum(node("var", (update.variable,)), update.term, update.op == "+=")
        then.append(node("cmp", ("==", node("var_next", (update.variable,)), after)))
    return before, then, [same(name) for name in names if name not in {update.variable for update in updates}]


def rule_segments(before, then, unchanged, way):
    """`before => then` as one line, and the conjuncts for what the rule leaves unchanged as its continuation."""
    group = lambda parts: _bare(spell(behavior2.Expr("and", tuple(parts)), way)) if len(parts) > 1 else spell(parts[0], way)
    arrow = [[*segment, "main"] if len(segment) > 1 else segment for segment in _mark(way["implies"], "implies")]
    return [*group(before), *arrow, *group(then)], [*_mark(way["and"], "and"), *group(unchanged)] if unchanged else None


def _goal(formula):
    return [["("], *_bare(spell(formula, FORMAL)), [")"]]


def _goal_as_zal_reads_it(formula):
    """One goal as behavior2.game_readings writes it: the typed rendering with two operators respelled."""
    return [["("], [formula.render().replace(" == ", " = ").replace(" != ", " ≠ ")], [")"]]


def game_segments(player, objective, payload, goal=_goal):
    """A game property in the form of ZAL's own formula rung; `goal` writes each goal formula."""
    force = [[FORCES[0], "can_force"], [player], [FORCES[1], "can_force"], [" "]]
    if objective == "within":
        return [*force, [f"within {payload.args[0]}", "within_steps"], [" "], *goal(payload.args[1])]
    if isinstance(payload, tuple):
        out = [*force, ["("]]
        for index, formula in enumerate(payload):
            out += (_mark(FORMAL["and"], "and") if index else []) + [[OFTEN, "infinitely_often"], [" "], *goal(formula)]
        return [*out, [")"]]
    symbol = {"eventually": FORMAL["eventually"], "always": FORMAL["always"], "infinitely often": OFTEN}[objective]
    return [*force, [symbol, objective.replace(" ", "_")], [" "], *goal(payload)]


def decided_steps(model, profile):
    """Every declared input ZAL decides, as the step its evaluator reads. An overlap or a range trap decides nothing."""
    if profile is behavior2:
        for state, values, event, context in model.inputs_domain():
            result = model.outcome(state, values, event, context)
            if result[0] == "step":
                yield result[1]
        return
    for state, event, context in model.inputs():
        if len(model.enabled(state, event, context)) > 1:
            continue
        told = model.decide(state, event, context)
        yield SimpleNamespace(state=state, values={}, event=event, context=context, accepted=told["class"] == "Accept",
                              post_state=told["state"], post_values={}, rule=told["rule"])


def formal_lines(model, profile):
    """Each line of the statement in symbols, keyed (concept, name) like its legend entry, and what a rule line leaves out.

    A line that is absent here has no formula form: the frame, a player, a symmetry and an unresolved obligation.
    """
    le, eq = (lambda: _mark(FORMAL["<="], "le")), (lambda: _mark(FORMAL["=="], "eq"))
    named = lambda name: [name, "name:" + name]
    listed = lambda names, keyed: [part for index, name in enumerate(names)
                                   for part in ([[", "]] if index else []) + [named(name) if keyed else [name]]]
    lines = {("states", None): [["state", "state"], [" "], [MEMBER, "in"], [" {"], *listed(model.states, False), ["}"]],
             ("events", None): [["event", "event"], [" "], [MEMBER, "in"], [" {"], *listed(model.events, False), ["}"]],
             ("initial", None): [["initially", "initially"], [" "], ["state", "state"], *eq(), [model.initial]],
             ("default", None): [["otherwise", "otherwise"], *[[*s, "main"] if len(s) > 1 else s for s in _mark(FORMAL["implies"], "implies")],
                                 [FORMAL["not"], "not"], ["accepted", "accepted"], *_mark(FORMAL["and"], "and"),
                                 ["unchanged", "unchanged"], ["reason " + model.default, None, "note"]]}
    if model.facts:
        lines["facts", None] = [*listed(model.facts, True), [" "], [MEMBER, "in"], [" {false, true}"]]
    for item in (model.inputs if profile is behavior2 else ()):
        lines["input", item.name] = [[str(item.low)], *le(), named(item.name), *le(), [str(item.high)]]
    for item in getattr(model, "variables", ()):
        lines["var", item.name] = [[str(item.low)], *le(), named(item.name), *le(), [str(item.high)], [", "],
                                   ["initially", "initially"], [" "], named(item.name), *eq(), [str(item.initial)]]
    more = {}
    for rule in model.rules:
        before, then, unchanged = rule_formula(model, rule)
        lines["rule", rule.id], more["rule", rule.id] = rule_segments(before, then, unchanged, FORMAL)
        if rule.reason:
            lines["rule", rule.id].append(["reason " + rule.reason, None, "note"])
    for concept, items in (("invariant", model.invariants), ("require", getattr(model, "requirements", ())),
                           ("property", getattr(model, "properties", ()))):
        for name, formula in items:
            lines[concept, name] = whole(formula)
    for name, formula in getattr(model, "fairness", ()):
        lines["fairness", name] = [[OFTEN, "infinitely_often"], [" "], *_goal(formula)]
    rungs = {reading["property"]: reading["formula_rung"] for reading in (behavior2.game_readings(model) if profile is behavior2 else [])}
    for name, player, objective, payload in getattr(model, "games", ()):
        lines["game", name] = game_segments(player, objective, payload)
        said = rungs[name][len(plain(game_segments(player, objective, payload, _goal_as_zal_reads_it))):].strip()
        if said:                                                      # ZAL's own qualifiers, in its words
            lines["game", name].append([said, None, "note"])
    return lines, more


def check_formal(model, profile, count):
    """Compare the desk's spelling with ZAL's renderer, parser and evaluator; a difference refuses the statement.

    Every formula of the model: the typed spelling is exactly ZAL's rendering, and ZAL parses the form without a
    negation's parentheses back to the same formula. Every rule: its right side states the outcome, the next
    state and the next value of every variable; ZAL parses the typed form of the step formula to the formula
    evaluated here; the left side holds on exactly the declared inputs that rule decides; and the right side
    holds on those. A game's line starts as ZAL's own formula rung does.
    """
    refuse = lambda what: Disagreement(f"The review desk's symbols no longer match ZAL ({what}); update review_desk.spell")
    names = (model.declarations if profile is behavior2 else
             {"facts": set(model.facts), "inputs": set(), "variables": set(), "states": set(model.states), "events": set(model.events)})
    def reparse(text, scope):
        if profile is behavior2 or scope == "step":
            return behavior2.parse_formula(text, names, scope).data()
        return behavior.parse_formula(text, model.facts, model.states, invariant=scope == "invariant").data()
    formulas = [(rule.guard, "guard") for rule in model.rules] + [(formula, "invariant") for _, formula in model.invariants]
    if profile is behavior2:
        formulas += [(formula, "step") for _, formula in (*model.requirements, *model.fairness)]
        formulas += [(formula, "property") for _, formula in model.properties]
        for _, _, objective, payload in model.games:
            formulas += [(goal, "invariant") for goal in (payload if isinstance(payload, tuple) else
                                                          [payload.args[1] if objective == "within" else payload])]
    for formula, scope in formulas:
        if plain(spell(formula, TYPED, exact=True)) != formula.render():
            raise refuse("typed spelling of " + formula.render()[:80])
        if reparse(plain(_bare(spell(formula, TYPED))), scope) != formula.data():
            raise refuse("grouping of " + formula.render()[:80])
    steps, parsed, variables = [], 0, sorted(variable.name for variable in getattr(model, "variables", ()))
    for rule in model.rules:
        before, then, unchanged = rule_formula(model, rule)
        pinned = sorted(part.args[1].args[0] for part in [*then[2:], *unchanged]
                        if part.op == "cmp" and part.args[0] == "==" and part.args[1].op == "var_next")
        if pinned != variables or len(pinned) != len(then) - 2 + len(unchanged) or [part.op for part in then[:2]] != [
                "accepted" if rule.target else "not", "state_next"]:
            raise refuse("step formula of rule " + rule.id)             # the right side must fix the whole outcome
        line, more = rule_segments(before, then, unchanged, TYPED)
        node, expected = behavior2.Expr, None
        try:
            read = reparse(plain(line) + plain(more), "step")
        except Refusal as error:
            if "exceeds" not in str(error):                           # ZAL bounds one parsed formula; evaluation still covers this rule
                raise refuse("step formula of rule " + rule.id) from error
        else:
            expected = node("implies", (behavior2.combine("and", before), behavior2.combine("and", [*then, *unchanged])))
            if read != expected.data():
                raise refuse("step formula of rule " + rule.id)
            parsed += 1
        steps.append((rule.id, node("and", tuple(before)), node("and", (*then, *unchanged))))
    stride, evaluated = max(1, -(-count // MAX_FORMAL_INPUTS)), 0
    for step in itertools.islice(decided_steps(model, profile), 0, None, stride):
        evaluated += 1
        for rule_id, before, after in steps:
            decides = behavior2.holds(before, step)
            if decides != (step.rule == rule_id) or decides and not behavior2.holds(after, step):
                raise refuse("step formula of rule " + rule_id)
        if step.rule == "default" and (step.accepted or step.post_state != step.state or step.post_values != step.values):
            raise refuse("the default line")
    rungs = {reading["property"]: reading["formula_rung"] for reading in (behavior2.game_readings(model) if profile is behavior2 else [])}
    for name, player, objective, payload in getattr(model, "games", ()):
        if not rungs[name].startswith(plain(game_segments(player, objective, payload, _goal_as_zal_reads_it))):
            raise refuse("game property " + name)
    return {"formulas": len(formulas), "rules": len(steps), "rules_parsed": parsed, "inputs": evaluated, "of": count}


ZAL_OPERATOR = {"not": "!", "and": "&", "or": "|", "implies": "=>", "iff": "<=>", "eq": "==", "ne": "!=", "lt": "<",
                "le": "<=", "gt": ">", "ge": ">=", "add": "+", "sub": "-", "prime": "'", "accepted": "accepted",
                "always": "always", "eventually": "eventually", "next": "next", "within": "within", "until": "until"}
WORDING = {"rule": "In symbols a rule is a step formula: on a step where everything left of the main arrow holds, this rule "
                   "decides the step and everything right of the arrow holds.",
           "default": "In symbols, “otherwise” stands for a step on which the left side of no rule holds.",
           "require": "Which steps the check covered is in its scope sentence, under Evidence. With an invariant declared, "
                      "that can be fewer than every declared pre-state."}


def reading_key(model, profile, declared):
    """How to read the symbols of one statement: each symbol, kind of line and name with ZAL's own words for it.

    Readings and meanings are quoted from behavior2.OPERATORS and from the construct registry of the statement's
    profile. A row the desk words itself carries "desk": true, and the page says so. Only what the statement
    uses is kept.
    """
    used = {segment[1] for entry in declared for segment in (entry["formal"] or []) + (entry["more"] or []) if len(segment) > 1 and segment[1]}
    words = behavior2.CONSTRUCTS
    entries = {word: {"symbol": word, "typed": word, "read": word, "meaning": constructs.BY_ID[word].meaning} for word in ("true", "false")}
    for key, operator in ZAL_OPERATOR.items():
        read, _, meaning = behavior2.OPERATORS[operator]
        symbol = FORMAL.get({name: op for op, name in COMPARISON_KEY.items()}.get(key, key), key).strip().replace("{k}", "K")
        typed = behavior2.TEMPORAL_SYMBOLIC[key].format(a="P", b="Q", k="K") if key in behavior2.TEMPORAL_SYMBOLIC else operator
        entries[key] = {"symbol": symbol, "typed": typed, "read": read, "meaning": meaning}
    entries.update({
        "state": {"symbol": "state", "typed": "state", "read": "the control state", "meaning": words["states"][2]},
        "event": {"symbol": "event", "typed": "event", "read": "the event of this step", "meaning": words["events"][2]},
        "initially": {"symbol": "initially", "typed": "initial", "read": "initially", "meaning": words["initial"][2]},
        "can_force": {"symbol": FORCES[0] + "p" + FORCES[1], "typed": "NAME can force", "read": "player p can force", "meaning": words["game"][2]},
        "within_steps": {"symbol": "within K", "typed": "within K", "read": "to happen within K steps", "meaning": words["game"][2]},
        "infinitely_often": {"symbol": OFTEN, "typed": "infinitely often (P)", "read": "infinitely often",
                             "meaning": "Always eventually: P holds at infinitely many steps.", "desk": True},
        "in": {"symbol": MEMBER, "typed": "", "read": "is one of",
               "meaning": "The name on the left takes exactly one of the listed values.", "desk": True},
        "otherwise": {"symbol": "otherwise", "typed": "default", "read": "otherwise",
                      "meaning": "On a step where the left side of no rule holds.", "desk": True},
        "unchanged": {"symbol": "unchanged", "typed": "unchanged", "read": "unchanged",
                      "meaning": "The control state and every variable are the same after the step as before it.", "desk": True}})
    names = {}
    for entry in declared:
        found = re.match(r"(input|var) (\w+) in ", entry["symbolic"])
        if found:
            names[found.group(2)] = {"role": "input" if found.group(1) == "input" else "variable", "english": entry["english"]}
        elif entry["concept"] == "facts":
            names.update({fact: {"role": "context fact", "english": entry["english"]} for fact in model.facts})
    if any(key not in entries and key.removeprefix("name:") not in names for key in used):
        raise Disagreement("The review desk wrote a symbol its reading key does not explain; update review_desk.reading_key")
    kinds = ({concept: text for concept, (_, _, text) in words.items()} if profile is behavior2 else
             {construct.id: construct.meaning for construct in constructs.STATEMENTS})
    concepts = {entry["concept"] for entry in declared}
    return {"entries": {key: entry for key, entry in entries.items() if key in used},
            "spelling": {op: text for op, text in FORMAL.items() if op in TYPED},
            "names": {name: told for name, told in names.items() if "name:" + name in used},
            "kinds": {concept: text for concept, text in kinds.items() if concept in concepts},
            "wording": {concept: text for concept, text in WORDING.items() if concept in concepts}}


# ---- what the page reads about one statement --------------------------------------------------------------------------

class DistinctValues:
    """Keeps each distinct JSON value once; a decision table repeats the same evaluations many times."""

    def __init__(self):
        self.items, self.index = [], {}

    def add(self, value):
        key = json.dumps(value, sort_keys=True, separators=(",", ":"))
        if key not in self.index:
            self.index[key] = len(self.items)
            self.items.append(value)
        return self.index[key]


def declarations(model, profile):
    """ZAL's legend entries, one per rendered line, each with the name its line declares and the line in symbols."""
    (lines, more), out, shown = formal_lines(model, profile), [], set()
    for entry in profile.legend(model)["entries"]:
        concept = entry["concept"]
        found = NAMED.match(entry["symbolic"]) if concept not in ("header", "name") else None
        named = found or (re.match(r"(?:input|var) (\w+) in ", entry["symbolic"]) if concept in ("input", "var") else None)
        key = (concept, named.group(1) if named else None)
        shown.add(key)
        out.append({"concept": concept, "symbolic": entry["symbolic"], "english": entry["english"],
                    "id": found.group(1) if found else None, "formal": lines.get(key), "more": more.get(key)})
    if set(lines) - shown:
        raise Disagreement("The review desk wrote a line in symbols that ZAL's legend does not list; update review_desk.formal_lines")
    return out


def input_axes(data):
    return {"states": data["states"], "events": data["events"], "facts": data.get("context", []),
            "inputs": [{"name": name, "low": low, "high": high} for name, low, high in data.get("inputs", [])],
            "variables": [{"name": name, "low": low, "high": high, "initial": initial}
                          for name, low, high, initial in data.get("variables", [])],
            "initial": data["initial"]}


def input_key(axes, given):
    """The page's name for one input tuple: state, variables, event, facts and integer inputs in declared order."""
    parts = [given["state"]] + [given["variables"][v["name"]] for v in axes["variables"]] + [given["event"]]
    parts += [int(given["context"][fact]) for fact in axes["facts"]] + [given["context"][i["name"]] for i in axes["inputs"]]
    return "|".join(str(part) for part in parts)


def decision_table(model, profile, axes, count, checker):
    """ZAL's explanation of every declared input, or the reason a page does not hold one."""
    if count > MAX_TABLE_TUPLES:
        return {"status": "not-precomputed",
                "reason": f"it has {count:,} input tuples, and a page holds a table for at most {MAX_TABLE_TUPLES:,}"}
    kept = {name: DistinctValues() for name in ("outcome", "rule", "claim", "sentence", "nearest")}
    static, index, rows, with_nearest = None, {}, [], count <= MAX_NEAREST_TUPLES
    for given in input_tuples(model, profile):
        told = explain_input(model, profile, given, checker)
        if static is None:
            static = {"rules": [{key: rule[key] for key in ("id", "source", "event", "guard_english")} for rule in told["rules"]],
                      "requirements": [{"id": r["id"], "english": r["english"]} for r in told.get("requirements", [])],
                      "invariants": [{"id": r["id"], "english": r.get("english")} for r in told.get("invariants", [])]}
        # A rule written for another state or event is only counted by the page, so its guard evaluation is left out.
        rules = [kept["rule"].add({key: rule[key] for key in (RULE_FIELDS if rule["state_matches"] and rule["event_matches"]
                                                             else RULE_FIELDS[:3]) if key in rule}) for rule in told["rules"]]
        nearest = nearest_opposite(model, profile, given, told["outcome"], checker) if with_nearest else None
        index[input_key(axes, given)] = len(rows)
        rows.append([kept["outcome"].add(told["outcome"]), int(bool(told.get("admissible", True))), rules,
                     [kept["claim"].add({"value": r["value"], "tree": r.get("tree")}) for r in told.get("requirements", [])],
                     [kept["claim"].add({key: value for key, value in r.items() if key not in ("id", "english")})
                      for r in told.get("invariants", [])],
                     kept["sentence"].add(decision_sentence(model, profile, given)), kept["nearest"].add(nearest)])
    return {"status": "complete", "count": len(rows), "static": static, "index": index, "rows": rows,
            "kept": {name: values.items for name, values in kept.items()}}


def reachable_configurations(model, profile, axes):
    """The desk's own walk of ZAL's decisions from the initial configuration; compared with ZAL's count."""
    start = (axes["initial"], tuple(v["initial"] for v in axes["variables"]))
    contexts = list(model.contexts()) if profile is behavior2 else None
    seen, frontier = {start}, [start]
    while frontier:
        state, values = frontier.pop()
        named = {v["name"]: values[i] for i, v in enumerate(axes["variables"])}
        for event in axes["events"]:
            if profile is behavior2:
                outcomes = [model.decide(state, named, event, context) for context in contexts]
            else:
                outcomes = [model.decide(state, event, context) for s, e, context in model.inputs() if (s, e) == (state, event)]
            for outcome in outcomes:
                after = (outcome["state"], tuple(outcome.get("variables", {}).get(v["name"], named[v["name"]])
                                                 for v in axes["variables"]))
                if after not in seen:
                    seen.add(after)
                    frontier.append(after)
    return sorted("|".join([state, *map(str, values)]) for state, values in seen)


def compare_candidate(base, base_profile, text, checker, with_story=True):
    """Parse, check and semantically compare one candidate source with the base model.

    Returns what ZAL said and the candidate model; the model is None when ZAL refuses the candidate.
    """
    try:
        model, profile = read_statement(text)
    except ValueError as error:
        located = None
        try:
            located = diagnostics.describe(error, text, Path("candidate.zal"))
        except Exception:                                              # the located form is a courtesy
            pass
        return {"status": "refused", "diagnostic": str(error), "source_diagnostic": located}, None
    try:
        report = profile.check(model, checker)
    except ValueError as error:                                        # for example a declared symmetry that no longer holds
        return {"status": "refused", "diagnostic": str(error), "source_diagnostic": None}, None
    shown = report.get("witness") or {}
    told = {"status": report["status"], "revision": model.revision, "obligation": report.get("obligation"),
            "advisories": report.get("advisories", []), "checks": report.get("checks"),
            # The check ZAL reports as failed. Checks after it are "not-run", so "not passing" would name the wrong one.
            "failed_check": next((name for name, value in (report.get("checks") or {}).items() if value == "counterexample"), None),
            "failed_claim": shown.get("english") or report.get("english"),
            "failed_name": report.get("requirement") or report.get("invariant") or report.get("property")}
    if report["status"] == "counterexample" and with_story:
        told["story"] = counterexample_story(model, report, checker)
    if profile is base_profile:
        diff = profile.semantic_diff(base, model)
        told["diff"] = {"comparison": diff["comparison"], "changed_objects": diff["changed_objects"],
                        "domain_same": diff["domain_same"], "witness_count": len(diff["witnesses"]),
                        "witnesses": [{**witness, "sentences": witness_sentences(base, model, profile, witness)}
                                      for witness in diff["witnesses"][:MAX_WITNESSES]]}
    else:
        told["diff"] = {"comparison": "different-profile", "changed_objects": [], "witness_count": 0, "witnesses": []}
    return told, model


def witness_sentences(old, new, profile, witness):
    """ZAL's sentence for one distinguishing input, before and after; None for a changed initial configuration."""
    if "event" not in witness:
        return None
    given = {"state": witness["state"], "variables": witness.get("variables", {}), "event": witness["event"],
             "context": witness.get("context", {})}
    try:
        return {"before": decision_sentence(old, profile, given), "after": decision_sentence(new, profile, given)}
    except (ValueError, KeyError, TypeError):
        return None


# ---- planted rule edits: the desk's own instrument ------------------------------------------------------------------

def rule_edits(lines, declared):
    """Delete each rule, replace its guard with true, and remove its updates: one small edit at a time."""
    for at, entry in enumerate(declared):
        if entry["concept"] != "rule":
            continue
        line, name = entry["symbolic"], entry["id"]
        head, opened, rest = line.partition(" [")
        guard, arrow, outcome = rest.rpartition("] -> ")
        if not (name and opened and arrow and outcome.endswith(";")):
            raise Refusal(f"Rule line is not in the rendered form the review desk edits: {line[:120]}")
        edits = [("delete", f"Delete rule {name}", None)]
        if guard != "true":
            edits.append(("open", f"Replace the guard of rule {name} with true", f"{head} [true] -> {outcome}"))
        if outcome.startswith("accept") and "," in outcome:
            edits.append(("strip", f"Remove the updates of rule {name}", f"{head} [{guard}] -> {outcome.split(',')[0]};"))
        for kind, label, after in edits:
            yield ({"kind": kind, "rule": name, "label": label, "line": at, "after": after},
                   lines[:at] + ([after] if after else []) + lines[at + 1:])


def report_of(lines, checker):
    """ZAL's check report for a statement given as its rendered lines; None when ZAL refuses it."""
    try:
        model, profile = read_statement("\n".join(lines) + "\n")
        return profile.check(model, checker)
    except ValueError:
        return None


def failing_claim(report):
    """The claim a report names as failed: a requirement or property name, "invariants", or None."""
    if report is None or report["status"] != "counterexample":
        return None
    if report.get("obligation") in INVARIANT_OBLIGATIONS:
        return "invariants"
    return report.get("requirement") or report.get("property")


def report_under_one_claim(lines, declared_concepts, keep, checker):
    """ZAL's report for the same lines with every other requirement and property removed.

    `keep` is the line of one requirement, property or game property, or None for the invariants alone.
    Invariants always stay, because they decide which configurations the other checks cover.
    """
    kept_concept = next((concept for line, concept in zip(lines, declared_concepts) if line == keep), None)

    def chosen(narrow):
        out = []
        for line, concept in zip(lines, declared_concepts):
            if concept in CLAIM_CONCEPTS and line != keep:
                continue
            if narrow and concept == "fairness" and kept_concept not in ("property", "game"):
                continue
            if narrow and concept == "player" and kept_concept != "game":
                continue
            out.append(line)
        return out

    report = report_of(chosen(True), checker)
    return report_of(chosen(False), checker) if report is None else report


def assumption_probes(lines, declared_concepts, data, checker):
    """For a two-player game with fairness: can the other player always bring each assumption about?

    The probe restates a fairness formula as that player's own game objective. ZAL admits only post-state
    fairness beside a game, so removing the primes gives the same formula over configurations. Nothing is said
    about an assumption unless ZAL decides its probe.
    """
    players = [player[0] for player in data.get("players", [])]
    claimants = sorted({game[1] for game in data.get("games", [])})
    fair = [(line, concept) for line, concept in zip(lines, declared_concepts) if concept == "fairness"]
    if len(players) != 2 or not claimants or not fair:
        return []
    bare = [line for line, concept in zip(lines, declared_concepts) if concept not in ("fairness", "property", "game")]
    taken = set(NAMED.findall("\n".join(lines)))
    probe = next(name for name in (f"desk_probe{n}" for n in itertools.count()) if name not in taken)
    out = []
    for claimant in claimants:
        other = next(player for player in players if player != claimant)
        for line, _ in fair:
            name = NAMED.match(line).group(1)
            prefix = f"fairness {name}: infinitely often ("
            if not (line.startswith(prefix) and line.endswith(");")):
                continue
            formula = line[len(prefix):-2].replace("'", "")
            report = report_of(bare + [f"property {probe}: {other} can force infinitely often ({formula});"], checker)
            if report is not None and report["status"] in ("pass-with-scope", "counterexample"):
                out.append({"assumption": name, "claimant": claimant, "other": other, "status": report["status"]})
    return out


def planted_edits(model, profile, count, checker):
    """Every rule edit, ZAL's re-check of it, what ZAL's semantic diff reports, and which claims ZAL names as failing."""
    declared = declarations(model, profile)
    lines, concepts = [entry["symbolic"] for entry in declared], [entry["concept"] for entry in declared]
    if lines != model.render().splitlines():
        raise Refusal("The legend no longer lists the rendered source line by line; the review desk cannot edit it")
    columns = [{"id": "invariants", "label": "the invariants together", "line": None}] if "invariant" in concepts else []
    columns += [{"id": entry["id"], "label": entry["id"], "line": entry["symbolic"]}
                for entry in declared if entry["concept"] in CLAIM_CONCEPTS]
    breakdown = count <= MAX_BREAKDOWN_TUPLES and bool(columns)
    if breakdown:                                                      # a column is usable only if the base passes under it
        for column in columns:
            alone = report_under_one_claim(lines, concepts, column["line"], checker)
            column["usable"] = alone is not None and alone["status"] in PASSING
    data = model.data()
    probes = assumption_probes(lines, concepts, data, checker)
    rows = []
    for edit, edited in rule_edits(lines, declared):
        edited_concepts = concepts[:edit["line"]] + (["rule"] if edit["after"] else []) + concepts[edit["line"] + 1:]
        told, candidate = compare_candidate(model, profile, "\n".join(edited) + "\n", checker, with_story=False)
        row = {**edit, "before": lines[edit["line"]], "status": told["status"], "noticed_by": [], "lost_assumptions": []}
        if candidate is None:
            rows.append({**row, "class": "refused", "diagnostic": told["diagnostic"]})
            continue
        row.update({key: told[key] for key in ("obligation", "failed_check", "failed_claim", "failed_name", "diff")})
        if told["status"] == "counterexample":
            row["class"] = "structure" if row["failed_check"] in STRUCTURAL_CHECKS else "claim"
        else:
            row["class"] = "unnoticed" if told["diff"]["comparison"] == "different" else "no-change"
        if breakdown and row["class"] == "claim":                     # an edit that passes whole fails under no single claim
            row["noticed_by"] = [column["id"] for column in columns if column["usable"] and failing_claim(
                report_under_one_claim(edited, edited_concepts, column["line"], checker)) == column["id"]]
        if row["class"] == "unnoticed" and probes:
            after = {(p["assumption"], p["claimant"]): p["status"]
                     for p in assumption_probes(edited, edited_concepts, candidate.data(), checker)}
            row["lost_assumptions"] = [p for p in probes if p["status"] == "pass-with-scope"
                                       and after.get((p["assumption"], p["claimant"])) == "counterexample"]
            if row["lost_assumptions"]:
                row["class"] = "vacuous"
        rows.append(row)
    note = None
    if not breakdown:
        note = ("the statement makes no claim to break the edits down by" if not columns else
                f"the statement has {count:,} input tuples, and the breakdown by claim stops at {MAX_BREAKDOWN_TUPLES:,}")
    return {"rows": rows, "columns": [{key: value for key, value in column.items() if key != "line"} for column in columns],
            "breakdown": breakdown, "breakdown_note": note, "probes": probes,
            "method": "Each rule is deleted, has its guard replaced with true, and has its updates removed. ZAL re-checks "
                      "every edited statement. A claim's column re-checks the edit with the other requirements and "
                      "properties removed and the invariants kept, and is marked when ZAL names that claim as the one "
                      "that fails. When an invariant fails ZAL stops there, so the other claims are not asked about that "
                      "edit. A game's assumption is probed by restating it as the other player's own objective."}


def statement_bundle(text, name, with_edits=True):
    """Everything the page shows about one source; returns the bundle, the model and its profile module."""
    checker = checker_identity()
    model, profile = read_statement(text)
    report = profile.check(model, checker)
    data = model.data()
    axes = input_axes(data)
    count = report.get("input_tuples") or sum(1 for _ in input_tuples(model, profile))
    legend, declared = profile.legend(model), declarations(model, profile)
    bundle = {"name": data["name"], "file": name, "profile": data["profile"], "revision": model.revision,
              "comment": [line.lstrip("# ").rstrip() for line in text.splitlines() if line.lstrip().startswith("#")][:6],
              "symbolic": model.render(), "model": data, "axes": axes, "check": report,
              "declared": declared, "key": reading_key(model, profile, declared), "readings": legend.get("readings", []),
              "checker": checker, "python": sys.version.split()[0]}
    if report["status"] == "counterexample":
        bundle["story"] = counterexample_story(model, report, checker)
    passing = report["status"] in PASSING
    formal = check_formal(model, profile, count)
    compared = check_sentences(model, profile, count, checker)
    try:
        bundle["decisions"] = decision_table(model, profile, axes, count, checker)
    except (ValueError, KeyError, TypeError) as error:                 # a statement with overlapping rules has no table
        bundle["decisions"] = {"status": "not-precomputed", "reason": f"ZAL could not explain every input ({str(error)[:200]})"}
    walked = reachable_configurations(model, profile, axes) if passing and count <= MAX_TABLE_TUPLES else None
    expected = report.get("reachable_configurations", len(report["reachable_states"]) if "reachable_states" in report else None)
    bundle["reachable"] = walked
    tabled = bundle["decisions"].get("count")                          # None when the page holds no table: nothing to compare
    bundle["cross_checks"] = {"tuples": None if tabled is None else tabled == count, "sentences_compared": compared,
                              "reachable": None if walked is None or expected is None else len(walked) == expected,
                              "formal": formal}
    bundle["edits"] = planted_edits(model, profile, count, checker) if with_edits and passing else None
    return bundle, model, profile


# ---- the page ---------------------------------------------------------------------------------------------------------

def page(payload, token=""):
    """review_desk.html with its data and page token filled in."""
    template = PAGE.read_text(encoding="utf-8")
    if template.count(DATA_SLOT) != 1 or template.count(TOKEN_SLOT) != 1:
        raise Refusal("review_desk.html does not have exactly one data slot and one token slot")
    # Source text reaches the page inside this object. No "<" survives, so no source can close the script element.
    data = json.dumps(payload, separators=(",", ":"), ensure_ascii=True).replace("<", "\\u003c")
    return template.replace(TOKEN_SLOT, token).replace(DATA_SLOT, data)


def read_bounded(path):
    """At most one byte more than ZAL's source bound, so an oversized file is noticed without being read in full."""
    with open(path, "rb") as handle:
        return handle.read(MAX_SOURCE + 1)


def source_text(data):
    if len(data) > MAX_SOURCE or not data.isascii():
        raise Refusal("Source must be ASCII text at most 64 KiB")
    return data.decode("ascii")


def write_snapshot(sources, out):
    bundles = []
    for path in sources:
        started = time.perf_counter()
        try:
            bundle = statement_bundle(source_text(read_bounded(path)), path.name)[0]
        except (OSError, ValueError, KeyError, TypeError) as error:    # unreadable, refused by ZAL or in error: say so and go on
            print(f"{path.name}: left out, {error}", file=sys.stderr)
            continue
        bundles.append(bundle)
        print(f"{path.name}: {bundle['check']['status']}, {len((bundle['edits'] or {}).get('rows', []))} planted edits, "
              f"{time.perf_counter() - started:.1f}s", file=sys.stderr)
    if not bundles:
        raise SystemExit("no source could be read; nothing written")
    out.write_text(page({"mode": "snapshot", "bundles": bundles}), encoding="utf-8")
    print(f"wrote {out} ({out.stat().st_size} bytes)", file=sys.stderr)


# ---- the loopback page for one file ---------------------------------------------------------------------------------

class OpenStatement:
    """One source file as last read from disk, with ZAL's results for it. The file is never written."""

    def __init__(self, path):
        self.path, self.lock, self.digest = path, threading.Lock(), ""
        self.load()

    def load(self):
        self.bundle, self.model, self.profile, self.error = None, None, None, None
        text = ""
        try:
            data = read_bounded(self.path)
            self.digest = hashlib.sha256(data).hexdigest()             # recorded first, so a refused file is not re-read forever
            text = source_text(data)
            self.bundle, self.model, self.profile = statement_bundle(text, self.path.name)
        except OSError as error:
            self.digest = ""
            self.error = {"reason": f"The file could not be read: {error.strerror or error}"}
        except Disagreement as error:                                  # the desk's own fault, so not shown as ZAL refusing the source
            self.error = {"reason": str(error), "unexpected": True}
        except ValueError as error:                                    # ZAL refuses the source
            self.error = {"reason": str(error)}
            try:
                self.error = diagnostics.describe(error, text, Path(self.path.name))
            except Exception:                                          # the located form is a courtesy
                pass
        except (KeyError, TypeError) as error:                         # not a refusal: shown as an error, not hidden
            self.error = {"reason": f"{type(error).__name__}: {error}", "unexpected": True}

    def changed(self):
        try:
            return hashlib.sha256(read_bounded(self.path)).hexdigest() != self.digest
        except OSError:
            return bool(self.digest)                                   # a file that was readable and no longer is

    def act(self, route, data):
        if self.model is None:
            raise Refusal("ZAL refuses the file on disk; correct it and reload")
        if data.get("revision") != self.model.revision:
            raise Refusal("This page shows an older revision; reload")
        checker = checker_identity()
        if route == "explain":
            told = explain_input(self.model, self.profile, data["input"], checker)
            told["sentence"] = decision_sentence(self.model, self.profile, data["input"])
            told["nearest"] = nearest_opposite(self.model, self.profile, data["input"], told["outcome"], checker)
            return told
        if route == "candidate":
            return compare_candidate(self.model, self.profile, data["source"], checker)[0]
        raise Refusal("Unknown review desk action")

    def view(self):
        base = {"mode": "live", "file": self.path.name}
        return {**base, "bundles": [], "error": self.error} if self.bundle is None else {**base, "bundles": [self.bundle]}


def answer(statement, token, port, method, target, headers, read):
    """Decide one exchange with the loopback page: (status, body, content type). No socket is touched here.

    `headers` maps names to values and `read(n)` returns the request body; at most REQUEST_BOUND bytes are read.
    A request is the page's own only if it names this loopback server as its host, carries the page's token
    and, when it states an origin, states this server. A page on another origin can arrange none of the three.
    """
    here = f"127.0.0.1:{port}"
    local = headers.get("Host") == here
    own = (local and headers.get("Origin", f"http://{here}") == f"http://{here}"
           and secrets.compare_digest(headers.get("X-ZAL-Review-Desk", "").encode("utf-8", "replace"), token.encode()))
    if method == "GET":
        if not local:
            return 403, {"error": "Open the review desk at its 127.0.0.1 address"}, JSON_TYPE
        if target == "/":
            with statement.lock:
                if statement.changed():                                # a browser refresh shows the file as it is now
                    statement.load()
                return 200, page(statement.view(), token), "text/html"
        if target == "/favicon.ico":
            return 204, "", "image/x-icon"
        if target == "/api/changed" and own:
            return 200, {"changed": statement.changed()}, JSON_TYPE
        return 404, {"error": "Unknown or unauthorized route"}, JSON_TYPE
    if method != "POST" or not own:
        return 403, {"error": "The review desk page's own token is required"}, JSON_TYPE
    try:
        length = int(headers.get("Content-Length", "0"))
        if not 0 < length <= REQUEST_BOUND:
            raise Refusal("Request is empty or exceeds the review desk bound")
        data = json.loads(read(length), **json_numbers.hooks(json_numbers.raising(Refusal, "request: ")))
        if not isinstance(data, dict):
            raise Refusal("Expected a review desk request object")
        with statement.lock:
            return 200, statement.act(urlsplit(target).path.removeprefix("/api/"), data), JSON_TYPE
    except (ValueError, KeyError, TypeError, OSError) as error:
        return 400, {"error": str(error)[:2000]}, JSON_TYPE


def make_server(path, port):
    """The loopback server for one file: a thin shell that reads a request, asks answer() and writes the reply."""
    statement, token = OpenStatement(path), secrets.token_urlsafe(32)

    class Handler(BaseHTTPRequestHandler):
        timeout = 30

        def log_message(self, *args):
            pass

        def exchange(self):
            status, body, mime = answer(statement, token, self.server.server_port, self.command, self.path,
                                        self.headers, self.rfile.read)
            data = body.encode() if isinstance(body, str) else json.dumps(body, separators=(",", ":")).encode()
            self.send_response(status)
            self.send_header("Content-Type", mime + "; charset=utf-8")
            self.send_header("Content-Length", str(len(data)))
            for name, value in RESPONSE_HEADERS:
                self.send_header(name, value)
            self.end_headers()
            self.wfile.write(data)

        do_GET = do_POST = exchange

    return ThreadingHTTPServer(("127.0.0.1", port), Handler), token


def serve(path, port=8770):
    try:
        httpd, _ = make_server(path, port)
    except OSError as error:
        raise SystemExit(f"cannot listen on 127.0.0.1:{port} ({error.strerror}); pass --port") from None
    print(f"ZAL review desk: http://127.0.0.1:{httpd.server_port} ({path.name}; loopback only; records no agreement)", flush=True)
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        httpd.server_close()


def main(argv=None):
    parser = argparse.ArgumentParser(description="Read-only ZAL review desk for the finite profiles")
    parser.add_argument("command", choices=["serve", "snapshot", "data"])
    parser.add_argument("sources", type=Path, nargs="+")
    parser.add_argument("--port", type=int, default=8770)
    parser.add_argument("--out", type=Path, help="snapshot: the page to write")
    options = parser.parse_args(argv)
    if options.command == "snapshot":
        if options.out is None:
            raise SystemExit("snapshot needs --out PAGE.html")
        write_snapshot(options.sources, options.out)
        return 0
    if len(options.sources) != 1 or not options.sources[0].is_file():
        raise SystemExit(f"{options.command} takes one existing source file")
    if options.command == "serve":
        serve(options.sources[0].resolve(), options.port)
    else:
        try:
            bundle = statement_bundle(source_text(read_bounded(options.sources[0])), options.sources[0].name)[0]
        except ValueError as error:
            print(json.dumps({"status": "refused", "diagnostic": str(error), "authority": "none"}, indent=2), file=sys.stderr)
            return 2
        print(json.dumps(bundle, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
