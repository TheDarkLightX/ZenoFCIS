"""Deterministic why-not, what-if and counterexample narratives for zal/finite-fsm/1.

Pure functions over the admitted typed model: no eval, model calls, I/O or
authority. Searches enumerate an exact, stated finite scope. Every narrative
sentence is a registered template filled only from witness values, and is
re-derived from the model before it is returned.
"""

from __future__ import annotations

from behavior import PROFILE, Refusal, canonical, explain as decision, replay


WHY_NOT = "zal/why-not/1"
WHAT_IF = "zal/what-if/1"
NARRATIVE = "zal/narrative/1"
MAX_WITNESSES = 32
TARGET_KEYS = ("class", "state", "reason", "rule")


def value_text(value, symbolic=False):
    if type(value) is bool:
        return ("true" if value else "false") if symbolic else ("yes" if value else "no")
    return str(value)


def observations(context, symbolic=False):
    text = ", ".join(f"{name}={value_text(value, symbolic)}" for name, value in context.items())
    return text or ("none" if symbolic else "no context facts")


def outcome_text(outcome, symbolic=False, infinitive=False):
    if symbolic:
        return f"accept {outcome['state']}" if outcome["class"] == "Accept" else f"reject {outcome['reason']} unchanged"
    if outcome["class"] == "Accept":
        return f"{'accept and move' if infinitive else 'accepts and moves'} to {outcome['state']}"
    return f"{'reject' if infinitive else 'rejects'} because {outcome['reason']} and {'keep' if infinitive else 'keeps'} the state unchanged"


def decider_text(outcome):
    return "no rule is enabled, so the default" if outcome["rule"] == "default" else "rule " + outcome["rule"]


def statement(model, rule_id):
    """The rule's (or default's) own canonical lines from the model renderer."""
    prefixes = ("default reject ", "Otherwise reject ") if rule_id == "default" else (f"rule {rule_id}:", f"Rule {rule_id}:")
    symbolic = next(line for line in model.render().splitlines() if line.startswith(prefixes[0]))
    english = next(line for line in model.render(True).splitlines() if line.startswith(prefixes[1]))
    return {"symbolic": symbolic, "english": english}


def reachable_states(model):
    """Breadth-first control reachability; inputs with overlapping rules are skipped."""
    seen, todo = {model.initial}, [model.initial]
    while todo:
        state = todo.pop(0)
        for source, event, context in model.inputs():
            if source == state and len(model.enabled(source, event, context)) == 1:
                target = model.decide(source, event, context)["state"]
                if target not in seen:
                    seen.add(target)
                    todo.append(target)
    return seen


def target_of(model, want):
    if not isinstance(want, dict) or not want or set(want) - set(TARGET_KEYS):
        raise Refusal("Name the wanted outcome with class, state, reason or rule")
    if "class" in want and want["class"] not in {"Accept", "Reject"}:
        raise Refusal("Wanted class must be Accept or Reject")
    if "state" in want and want["state"] not in model.states:
        raise Refusal("Wanted state is not declared")
    if "rule" in want and want["rule"] not in {rule.id for rule in model.rules} | {"default"}:
        raise Refusal("Wanted rule is not declared")
    if "reason" in want and want["reason"] not in {rule.reason for rule in model.rules} | {model.default}:
        raise Refusal("Wanted reason is not declared by any rule or the default")
    return {key: want[key] for key in TARGET_KEYS if key in want}


def want_text(target):
    parts = []
    if "class" in target:
        parts.append(target["class"])
    if "state" in target:
        parts.append(("to " if target.get("class") == "Accept" else "in ") + target["state"])
    if "reason" in target:
        parts.append("because " + target["reason"])
    if "rule" in target:
        parts.append("by the default" if target["rule"] == "default" else "by rule " + target["rule"])
    return " ".join(parts)


def changes_between(model, before, after):
    """Changed inputs as typed IDs, in declaration order."""
    changes = [{"input": "state", "from": before[0], "to": after[0]}] if before[0] != after[0] else []
    if before[1] != after[1]:
        changes.append({"input": "event", "from": before[1], "to": after[1]})
    changes.extend({"input": "context:" + name, "from": before[2][name], "to": after[2][name]}
                   for name in model.facts if before[2][name] != after[2][name])
    return changes


def change_text(change):
    return f"{change['input'].removeprefix('context:')} from {value_text(change['from'])} to {value_text(change['to'])}"


def why_not(model, state, event, context, want, scope="context", checker_identity="unbound"):
    """Search the stated scope for the fewest input changes that give the wanted outcome."""
    current = model.decide(state, event, context)  # Validates the complete input first.
    target = target_of(model, want)
    if scope not in {"context", "input"}:
        raise Refusal("Search scope is context (state and event fixed) or input (every declared input)")
    def matches(outcome):
        return all(outcome[key] == value for key, value in target.items())
    original = (state, event, dict(context))
    searched, overlaps, found, elsewhere = 0, 0, [], 0
    for candidate in model.inputs():
        in_scope = scope == "input" or candidate[:2] == (state, event)
        if len(model.enabled(*candidate)) > 1:
            overlaps += in_scope
            continue
        outcome = model.decide(*candidate)
        if not in_scope:
            elsewhere += matches(outcome)
            continue
        searched += 1
        if matches(outcome):
            found.append((len(changes_between(model, original, candidate)), candidate, outcome))
    distance = min((item[0] for item in found), default=None)
    minimal = [item for item in found if item[0] == distance]
    reachable = reachable_states(model)
    witnesses = [{"input": {"state": s, "event": e, "context": dict(c)},
                  "changes": changes_between(model, original, (s, e, c)), "outcome": outcome,
                  "rule": outcome["rule"], "statement": statement(model, outcome["rule"]),
                  "source_reachable": s in reachable} for _, (s, e, c), outcome in minimal[:MAX_WITNESSES]]
    producing = []
    for rule in model.rules:
        result = {"class": "Accept" if rule.target else "Reject", "state": rule.target or rule.source,
                  "reason": rule.reason, "rule": rule.id}
        if matches(result):
            producing.append({"rule": rule.id, "source_matches": rule.source == state, "event_matches": rule.event == event,
                              "guard": rule.guard.render(True), "guard_value": rule.guard.value(state, context),
                              "statement": statement(model, rule.id)})
    phrase = (f"state {state} and event {event} fixed, every valuation of {', '.join(model.facts) or 'no facts'}"
              if scope == "context" else "every declared state, event and context valuation")
    summary = [f"From {state}, event {event} with {observations(context)}: {decider_text(current)} {outcome_text(current)}."]
    if distance == 0:
        status = "already"
        summary.append(f"The current input already gives {want_text(target)}.")
    elif found:
        status = "found"
        noun = "change" if distance == 1 else "changes"
        summary.append(f"The fewest input changes that give {want_text(target)}: {distance} {noun}; {len(minimal)} such input"
                       f"{'' if len(minimal) == 1 else 's'} in scope.")
        summary.extend(f"Changing {' and '.join(change_text(c) for c in item['changes'])}: "
                       f"{decider_text(item['outcome'])} {outcome_text(item['outcome'])}." for item in witnesses)
    else:
        status = "none-in-scope"
        summary.append(f"No input in the searched scope ({phrase}) gives {want_text(target)}.")
        if scope == "context":
            summary.append(f"{elsewhere} other declared input{'' if elsewhere == 1 else 's'} outside this scope give"
                           f"{'s' if elsewhere == 1 else ''} it; search scope input to list them." if elsewhere else
                           "No input in the whole declared domain gives it either.")
    for rule in producing:
        if status != "already" and rule["source_matches"] and rule["event_matches"] and not rule["guard_value"]:
            summary.append(f"Rule {rule['rule']} would give it here, but its guard ({rule['guard']}) is false for this input.")
    result = {"kind": "why-not", "schema": WHY_NOT, "profile": PROFILE, "revision": model.revision,
              "checker": checker_identity, "input": {"state": state, "event": event, "context": dict(context)},
              "outcome": current, "want": target, "status": status, "distance": distance,
              "minimal_count": len(minimal), "minimal_changes": witnesses, "producing_rules": producing,
              "scope": {"search": scope, "statement": phrase, "inputs_searched": searched,
                        "overlapping_inputs_skipped": overlaps,
                        "outside_scope_matches": elsewhere if scope == "context" else None,
                        "minimality": "fewest changed inputs (state, event and each context fact count one each)",
                        "reachability": "inputs are declared hypotheses; source_reachable reports control reachability from the initial state"},
              "summary": summary,
              "method": "exhaustive enumeration of the stated finite scope; typed model decisions; no language model",
              "authority": "none"}
    return bounded(result)


def guard_facts(node):
    if node["operator"] == "fact":
        return {node["name"]}
    return set().union(*(guard_facts(child) for child in node.get("operands", [])))


def what_if(model, state, event, context, change, checker_identity="unbound"):
    """Apply every named input change and compare both decisions rule by rule."""
    if not isinstance(change, dict) or not change or set(change) - {"state", "event", "context"}:
        raise Refusal("A what-if changes state, event or named context facts")
    facts = change.get("context", {})
    if not isinstance(facts, dict) or "context" in change and not facts:
        raise Refusal("A context change names at least one declared fact")
    unknown = sorted(set(facts) - set(model.facts))
    if unknown:
        raise Refusal("What-if names an undeclared context fact: " + ", ".join(unknown))
    after_state, after_event = change.get("state", state), change.get("event", event)
    after_context = {name: facts.get(name, value) for name, value in context.items()}
    before = decision(model, state, event, context, checker_identity)
    after = decision(model, after_state, after_event, after_context, checker_identity)
    requested = ([("state", state, change["state"])] if "state" in change else []) + \
                ([("event", event, change["event"])] if "event" in change else []) + \
                [("context:" + name, context.get(name), facts[name]) for name in model.facts if name in facts]
    observed = {"state": after["input"]["state"], "event": after["input"]["event"],
                **{"context:" + name: value for name, value in after["input"]["context"].items()}}
    applied = []
    for name, old, new in requested:
        if observed[name] != new:
            raise Refusal(f"What-if did not apply the requested change to {name}")
        applied.append({"input": name, "from": old, "to": new, "effective": old != new})
    changed_facts = {item["input"].removeprefix("context:") for item in applied if item["effective"]}
    rules = []
    for old, new in zip(before["rules"], after["rules"]):
        fields = {"source_matches": "state_matches", "event_matches": "event_matches", "enabled": "enabled"}
        view = lambda r: {**{key: r[field] for key, field in fields.items()}, "guard_value": r["guard"]["value"]}
        old_view, new_view = view(old), view(new)
        rules.append({"id": old["id"], "guard": old["guard_english"], "before": old_view, "after": new_view,
                      "changed": [key for key in old_view if old_view[key] != new_view[key]],
                      "changed_facts_read": sorted(guard_facts(old["guard"]) & changed_facts)})
    outcome_changes = [key for key in TARGET_KEYS if before["outcome"][key] != after["outcome"][key]]
    summary = [f"Changes: {'; '.join(change_text(item) for item in applied)}."]
    summary.extend(f"{item['input'].removeprefix('context:')} is already {value_text(item['to'])}; that change has no effect."
                   for item in applied if not item["effective"])
    if outcome_changes:
        summary.append(f"Before: {decider_text(before['outcome'])} {outcome_text(before['outcome'])}. "
                       f"After: {decider_text(after['outcome'])} {outcome_text(after['outcome'])}.")
    else:
        summary.append(f"The outcome stays the same: {decider_text(after['outcome'])} {outcome_text(after['outcome'])}.")
    for rule in rules:
        if "enabled" in rule["changed"]:
            summary.append(f"Rule {rule['id']} becomes {'enabled' if rule['after']['enabled'] else 'disabled'}.")
        elif "guard_value" in rule["changed"]:
            summary.append(f"Rule {rule['id']}: its guard becomes {value_text(rule['after']['guard_value'], True)}, "
                           f"but the rule stays {'enabled' if rule['after']['enabled'] else 'disabled'}.")
    result = {"kind": "what-if", "schema": WHAT_IF, "profile": PROFILE, "revision": model.revision,
              "checker": checker_identity, "before": {"input": before["input"], "outcome": before["outcome"]},
              "after": {"input": after["input"], "outcome": after["outcome"]}, "changes": applied,
              "outcome_changes": outcome_changes, "rules": rules, "summary": summary,
              "scope": "Two exact hypothetical inputs; every named change is applied and reported. "
                       "Context facts are assumptions, not authenticated external truth.",
              "method": "typed AST evaluation of both inputs; no language model", "authority": "none"}
    return bounded(result)


# Narrative sentences: template id -> (English, symbolic). Slots come only from
# sentence facts; facts come only from backed witness paths or the rule they name.
TEMPLATES = {
    "start": ("The machine starts in {state}.", "initial {state};"),
    "step": ("Step {n}: in {pre}, event {event} arrives with {observations}; {decider} {outcome}.",
             "{pre} + {event} ({symbolic_observations}) -> {symbolic_outcome} [{rule}]"),
    "end": ("After {count} step{plural} the state is {state}.", "state == {state}"),
    "input": ("In {state}, event {event} arrives with {observations}.", "{state} + {event} ({symbolic_observations})"),
    "raw_input": ("Take state {state}, whether or not it is reachable, and event {event} with {observations}.",
                  "{state} + {event} ({symbolic_observations})"),
    "enabled": ("Rule {rule} is enabled here (guard: {guard}), and it would {intent}.",
                "rule {rule}: [{symbolic_guard}] = true -> {symbolic_intent}"),
    "overlap": ("{count} rules are enabled together, so the next state is not determined; this profile gives no precedence.",
                "determinism: {rules} all enabled"),
    "accepts": ("Rule {rule} accepts and moves to {state}.", "-> accept {state} [{rule}]"),
    "violates": ("Invariant {invariant} requires {formula}, which is false when the state is {state}.",
                 "invariant {invariant}: {symbolic_formula}; state == {state} gives false"),
}


def lookup(root, path):
    """A backing is a key path into the witness, or the length of the list at one."""
    if isinstance(path, dict):
        return len(lookup(root, path["length_of"]))
    value = root
    for key in path:
        value = value[key]
    return value


def in_domain(model, state, event, context):
    return state in model.states and event in model.events and isinstance(context, dict) and set(context) == set(model.facts)


def rule_by_id(model, rule_id):
    return next(rule for rule in model.rules if rule.id == rule_id)


def invariant_by_name(model, name):
    return next(formula for key, formula in model.invariants if key == name)


def slots(model, template, facts):
    """Derive every displayed slot from the sentence's facts and the named model objects."""
    values = dict(facts)
    if "context" in facts:
        values.update(observations=observations(facts["context"]), symbolic_observations=observations(facts["context"], True))
    if template == "step":
        values.update(decider=decider_text(facts["outcome"]), outcome=outcome_text(facts["outcome"]),
                      symbolic_outcome=outcome_text(facts["outcome"], True), rule=facts["outcome"]["rule"])
    if template == "end":
        values["plural"] = "" if facts["count"] == 1 else "s"
    if template == "enabled":
        rule = rule_by_id(model, facts["rule"])
        intent = {"class": "Accept" if rule.target else "Reject", "state": rule.target or rule.source,
                  "reason": rule.reason, "rule": rule.id}
        values.update(guard=rule.guard.render(True), symbolic_guard=rule.guard.render(),
                      intent=outcome_text(intent, infinitive=True), symbolic_intent=outcome_text(intent, True))
    if template == "overlap":
        values.update(count=len(facts["rules"]), rules=", ".join(facts["rules"]))
    if template == "violates":
        formula = invariant_by_name(model, facts["invariant"])
        values.update(formula=formula.render(True), symbolic_formula=formula.render())
    return values


def sentence(model, template, facts, backing):
    values = slots(model, template, facts)
    english, symbolic = TEMPLATES[template]
    return {"template": template, "facts": facts, "backing": backing,
            "english": english.format(**values), "symbolic": symbolic.format(**values)}


def verify(model, root, sentences):
    """Independently re-derive every sentence; return the unbacked ones."""
    problems = []
    previous, current = None, model.initial
    for index, item in enumerate(sentences):
        facts, template = item["facts"], item["template"]
        def fail(reason):
            problems.append(f"sentence {index + 1} ({template}): {reason}")
        if template not in TEMPLATES or set(item) != {"n", "template", "facts", "backing", "english", "symbolic"}:
            fail("unregistered template or fields")
            continue
        try:
            if any(lookup(root, path) != facts[key] for key, path in item["backing"].items()):
                fail("a fact differs from its witness path")
            unbacked = set(facts) - set(item["backing"]) - ({"n"} if template == "step" else set())
            if unbacked:
                fail("facts without a witness path: " + ", ".join(sorted(unbacked)))
            values = slots(model, template, facts)
            if (item["english"], item["symbolic"]) != tuple(text.format(**values) for text in TEMPLATES[template]):
                fail("text is not its template filled from the facts")
            if template == "start" and facts["state"] != model.initial:
                fail("the start is not the declared initial state")
            if template == "step":
                if facts["n"] != (previous or 0) + 1:
                    fail("steps are not numbered consecutively")
                previous = facts["n"]
                if facts["pre"] != current:
                    fail("the step does not start where the previous step ended")
                if model.decide(facts["pre"], facts["event"], facts["context"]) != facts["outcome"]:
                    fail("the model does not decide this step this way")
                current = facts["outcome"]["state"]
            if template in {"input", "raw_input"} and not in_domain(model, facts["state"], facts["event"], facts["context"]):
                fail("the narrated input is outside the declared domain")
            if template == "enabled":
                given = sentences[0]["facts"]
                if rule_by_id(model, facts["rule"]) not in model.enabled(given["state"], given["event"], given["context"]):
                    fail("the rule is not enabled at the narrated input")
            if template == "overlap":
                given = sentences[0]["facts"]
                if [rule.id for rule in model.enabled(given["state"], given["event"], given["context"])] != list(facts["rules"]) \
                        or len(facts["rules"]) < 2:
                    fail("the enabled rules differ from the narrated overlap")
            if template == "accepts":
                given = sentences[0]["facts"]
                if model.decide(given["state"], given["event"], given["context"]) != \
                        {"class": "Accept", "state": facts["state"], "reason": None, "rule": facts["rule"]}:
                    fail("the narrated input does not accept this way")
            if template == "violates" and invariant_by_name(model, facts["invariant"]).value(facts["state"], {}):
                fail("the invariant holds in the narrated state")
            if template == "end":
                steps = [s["facts"] for s in sentences if s["template"] == "step"]
                if facts["count"] != len(steps) or facts["state"] != (steps[-1]["outcome"]["state"] if steps else model.initial):
                    fail("the final state or step count differs from the narrated steps")
        except (KeyError, IndexError, TypeError, StopIteration, Refusal) as error:
            fail("cannot be re-derived: " + type(error).__name__)
    return problems


def finish(model, kind, root, parts, checker_identity, extra=None):
    sentences = [{"n": number, **part} for number, part in enumerate(parts, 1)]
    problems = verify(model, root, sentences)
    if problems:
        raise Refusal("Narrative sentence not backed by the witness: " + problems[0])
    return bounded({"kind": "narrative", "schema": NARRATIVE, "profile": PROFILE, "revision": model.revision,
                    "checker": checker_identity, "source": kind, "status": "narrated", **(extra or {}),
                    "sentences": sentences, "english": [s["english"] for s in sentences],
                    "symbolic": [s["symbolic"] for s in sentences],
                    "method": "registered sentence templates filled from witness values, each re-derived from the model; no language model",
                    "authority": "none"})


def step_parts(model, rows, prefix, keys):
    """Numbered step sentences; keys name the pre-state field of this row shape."""
    parts = []
    for index, row in enumerate(rows):
        path = [*prefix, index]
        parts.append(sentence(model, "step", {"n": index + 1, "pre": row[keys], "event": row["event"],
                                              "context": row["context"], "outcome": row["outcome"]},
                              {"pre": [*path, keys], "event": [*path, "event"], "context": [*path, "context"],
                               "outcome": [*path, "outcome"]}))
    return parts


def narrate_check(model, report, checker_identity="unbound"):
    """Tell one checker counterexample as a short numbered story."""
    if getattr(model, "data", None) and model.data()["profile"] != PROFILE:
        import explain2
        return explain2.narrate_check(model, report, checker_identity)
    if report.get("status") != "counterexample":
        return bounded({"kind": "narrative", "schema": NARRATIVE, "profile": PROFILE, "revision": model.revision,
                        "checker": checker_identity, "source": "check", "status": "no-counterexample",
                        "check_status": report.get("status"), "sentences": [], "english": [], "symbolic": [],
                        "method": "nothing to narrate: the check reported no counterexample in its stated scope",
                        "authority": "none"})
    if report.get("revision") != model.revision:
        raise Refusal("Counterexample belongs to a different revision")
    obligation = report.get("obligation")
    witness = report["witness"]
    given = {"state": ["witness", "state"], "event": ["witness", "event"], "context": ["witness", "context"]}
    if obligation == "determinism":
        parts = [sentence(model, "input", {key: witness[key] for key in given}, given)]
        parts.extend(sentence(model, "enabled", {"rule": rule}, {"rule": ["rules", i]}) for i, rule in enumerate(report["rules"]))
        parts.append(sentence(model, "overlap", {"rules": report["rules"]}, {"rules": ["rules"]}))
    elif obligation == "invariant":
        trace = witness["trace"]
        start = ["witness", "trace", 0, "state"] if trace else ["witness", "state"]
        parts = [sentence(model, "start", {"state": lookup(report, start)}, {"state": start})]
        parts.extend(step_parts(model, trace, ["witness", "trace"], "state"))
        parts.append(sentence(model, "end", {"count": len(trace), "state": witness["state"]},
                              {"count": {"length_of": ["witness", "trace"]}, "state": ["witness", "state"]}))
        parts.append(sentence(model, "violates", {"invariant": report["invariant"], "state": witness["state"]},
                              {"invariant": ["invariant"], "state": ["witness", "state"]}))
    elif obligation == "invariant-preservation":
        parts = [sentence(model, "raw_input", {key: witness[key] for key in given}, given),
                 sentence(model, "accepts", {"rule": witness["outcome"]["rule"], "state": witness["outcome"]["state"]},
                          {"rule": ["witness", "outcome", "rule"], "state": ["witness", "outcome", "state"]}),
                 sentence(model, "violates", {"invariant": report["invariant"], "state": witness["outcome"]["state"]},
                          {"invariant": ["invariant"], "state": ["witness", "outcome", "state"]})]
    else:
        raise Refusal(f"No narrative template for obligation {obligation}; the structured witness remains authoritative")
    return finish(model, "check", report, parts, checker_identity, {"obligation": obligation})


def narrate_trace(model, inputs, checker_identity="unbound"):
    """Replay assumed inputs from the initial state and tell them as a story."""
    if model.data()["profile"] != PROFILE:
        import explain2
        return explain2.narrate_trace(model, inputs, checker_identity)
    rows = replay(model, inputs)
    root = {"initial": model.initial, "trace": rows}
    final = ["trace", len(rows) - 1, "outcome", "state"] if rows else ["initial"]
    parts = [sentence(model, "start", {"state": model.initial}, {"state": ["initial"]})]
    parts.extend(step_parts(model, rows, ["trace"], "pre"))
    parts.append(sentence(model, "end", {"count": len(rows), "state": lookup(root, final)},
                          {"count": {"length_of": ["trace"]}, "state": final}))
    return finish(model, "trace", root, parts, checker_identity,
                  {"scope": "Assumed inputs replayed from the declared initial state; context facts are assumptions."})


def bounded(result):
    if len(canonical(result).encode()) > 512 * 1024:
        raise Refusal("Explanation exceeds the 512 KiB output bound")
    return result


def human_text(result):
    """Readable terminal view of why-not, what-if and narrative results."""
    kind = result["kind"]
    if kind == "narrative":
        title = result["source"] + (", " + result["obligation"] if result.get("obligation") else "")
        lines = [f"Story ({title}) at {result['revision'][:12]}; each sentence is generated from the witness (no LLM)"]
        if result["status"] == "no-counterexample":
            lines.append(f"The check reported no counterexample (status {result['check_status']}); there is nothing to narrate.")
        for item in result["sentences"]:
            lines.extend([f"{item['n']}. {item['english']}", "   Symbolic: " + item["symbolic"]])
        return "\n".join(lines)
    lines = [f"{'Why not' if kind == 'why-not' else 'What if'} at {result['revision'][:12]} (typed logic; no LLM)"]
    if kind == "why-not":
        scope = result["scope"]
        lines.append(f"Wanted: {want_text(result['want'])}. Search scope: {scope['statement']}; "
                     f"{scope['inputs_searched']} inputs searched, {scope['overlapping_inputs_skipped']} with overlapping rules skipped.")
        lines.extend(result["summary"])
        for item in result["minimal_changes"]:
            lines.append("  " + item["statement"]["english"] + ("" if item["source_reachable"] else " (source state unreachable)"))
        if result["minimal_count"] > len(result["minimal_changes"]):
            lines.append(f"Showing {len(result['minimal_changes'])} of {result['minimal_count']} minimal inputs.")
    else:
        lines.extend(result["summary"])
        for rule in result["rules"]:
            if rule["changed"]:
                lines.append(f"  Rule {rule['id']} ({rule['guard']}): " + ", ".join(
                    f"{key} {value_text(rule['before'][key], True)} -> {value_text(rule['after'][key], True)}" for key in rule["changed"]))
        lines.append(result["scope"])
    lines.append("Authenticated facts: none supplied by this profile.")
    return "\n".join(lines)
