"""Deterministic why-not, what-if and counterexample narratives for zal/finite-fsm/2.

The profile 2 counterpart of explain.py: bounded integer variables and inputs,
range traps, transition requirements and temporal properties with fair lassos.
Pure functions over the admitted model; no eval, model calls, I/O or authority.
Every narrative sentence is a registered template filled only from witness
values, and the whole story is re-derived from the model before it is returned.
"""

from __future__ import annotations

from collections import deque
from functools import lru_cache

import behavior2 as B
from behavior import Refusal
from explain import MAX_WITNESSES, bounded, value_text


PROFILE = B.PROFILE
WHY_NOT = "zal/why-not/2"
WHAT_IF = "zal/what-if/2"
NARRATIVE = "zal/narrative/2"
TARGET_KEYS = ("class", "state", "variables", "reason", "rule", "variable")


def observations(context, symbolic=False):
    text = ", ".join(f"{name}={value_text(value, symbolic)}" for name, value in context.items())
    return text or ("none" if symbolic else "no context facts")


def configuration(state, values, symbolic=False):
    if not values:
        return state
    listed = ", ".join(f"{name}={value}" for name, value in values.items())
    return f"{state} {{{listed}}}" if symbolic else f"{state} with {listed}"


def outcome_text(outcome, symbolic=False, infinitive=False):
    if outcome["class"] == "Trap":
        low, high = outcome["range"]
        if symbolic:
            return f"trap {outcome['variable']}={outcome['value']} outside {low}..={high}"
        return (f"{'set' if infinitive else 'would set'} {outcome['variable']} to {outcome['value']}, outside {low}..={high}, "
                f"so the step {'is' if not infinitive else 'would be'} refused")
    if symbolic:
        return (f"accept {configuration(outcome['state'], outcome['variables'], True)}" if outcome["class"] == "Accept"
                else f"reject {outcome['reason']} unchanged")
    if outcome["class"] == "Accept":
        return f"{'accept and move' if infinitive else 'accepts and moves'} to {configuration(outcome['state'], outcome['variables'])}"
    return f"{'reject' if infinitive else 'rejects'} because {outcome['reason']} and {'keep' if infinitive else 'keeps'} the state and variables unchanged"


def decider_text(outcome):
    return "no rule is enabled, so the default" if outcome["rule"] == "default" else "rule " + outcome["rule"]


def outcome_of(model, state, values, event, context):
    """(outcome, overlapping rule IDs) for one valid input; a range trap is an outcome, not a refusal."""
    result = model.outcome(state, dict(values), event, dict(context))
    if result[0] == "overlap":
        return None, [rule.id for rule in result[1]]
    if result[0] == "trap":
        _, rule, name, value = result
        return {"class": "Trap", "rule": rule.id, "variable": name, "value": value, "range": list(model._ranges[name])}, None
    return B._outcome_data(result[1]), None


def statement(model, rule_id):
    prefixes = ("default reject ", "Otherwise reject ") if rule_id == "default" else (f"rule {rule_id}:", f"Rule {rule_id}:")
    return {"symbolic": next(line for line in model.render().splitlines() if line.startswith(prefixes[0])),
            "english": next(line for line in model.render(True).splitlines() if line.startswith(prefixes[1]))}


def admissible(model, state, values):
    probe = B._Step(state, values, None, {})
    return all(B.holds(formula, probe) for _, formula in model.invariants)


@lru_cache(maxsize=8)
def reachable_configurations(model):
    """Breadth-first configuration reachability over complete steps; overlaps and traps do not move."""
    start = (model.initial, tuple(sorted(model.initial_values().items())))
    seen, todo = {start}, deque([start])
    contexts = list(model.contexts())
    while todo:
        state, values = todo.popleft()
        for event in model.events:
            for context in contexts:
                outcome, overlap = outcome_of(model, state, dict(values), event, context)
                if overlap or outcome["class"] == "Trap":
                    continue
                key = (outcome["state"], tuple(sorted(outcome["variables"].items())))
                if key not in seen:
                    seen.add(key)
                    todo.append(key)
    return seen


def target_of(model, want):
    if not isinstance(want, dict) or not want or set(want) - set(TARGET_KEYS):
        raise Refusal("Name the wanted outcome with class, state, variables, reason, rule or variable")
    if "class" in want and want["class"] not in {"Accept", "Reject", "Trap"}:
        raise Refusal("Wanted class must be Accept, Reject or Trap")
    if "state" in want and want["state"] not in model.states:
        raise Refusal("Wanted state is not declared")
    if "rule" in want and want["rule"] not in {rule.id for rule in model.rules} | {"default"}:
        raise Refusal("Wanted rule is not declared")
    if "reason" in want and want["reason"] not in {rule.reason for rule in model.rules} | {model.default}:
        raise Refusal("Wanted reason is not declared by any rule or the default")
    if "variable" in want and want["variable"] not in model._ranges:
        raise Refusal("Wanted trap variable is not declared")
    if "variables" in want:
        values = want["variables"]
        if not isinstance(values, dict) or not values or any(
                name not in model._ranges or type(value) is not int or not model._ranges[name][0] <= value <= model._ranges[name][1]
                for name, value in values.items()):
            raise Refusal("Wanted variables must name declared variables with in-range integers")
    return {key: want[key] for key in TARGET_KEYS if key in want}


def matches(outcome, target):
    for key, value in target.items():
        if key == "variables":
            if any(outcome.get("variables", {}).get(name) != wanted for name, wanted in value.items()):
                return False
        elif outcome.get(key) != value:
            return False
    return True


def want_text(target):
    parts = [target["class"]] if "class" in target else []
    if "state" in target:
        parts.append(("to " if target.get("class") == "Accept" else "in ") + target["state"])
    if "variables" in target:
        parts.append("with " + ", ".join(f"{name}={value}" for name, value in target["variables"].items()))
    if "variable" in target:
        parts.append("on " + target["variable"])
    if "reason" in target:
        parts.append("because " + target["reason"])
    if "rule" in target:
        parts.append("by the default" if target["rule"] == "default" else "by rule " + target["rule"])
    return " ".join(parts)


def changes_between(model, before, after):
    """Changed inputs as typed IDs: state, variable:NAME, event, context:FACT and input:NAME."""
    changes = [{"input": "state", "from": before[0], "to": after[0]}] if before[0] != after[0] else []
    changes += [{"input": "variable:" + v.name, "from": before[1][v.name], "to": after[1][v.name]}
                for v in model.variables if before[1][v.name] != after[1][v.name]]
    if before[2] != after[2]:
        changes.append({"input": "event", "from": before[2], "to": after[2]})
    changes += [{"input": "context:" + name, "from": before[3][name], "to": after[3][name]}
                for name in model.facts if before[3][name] != after[3][name]]
    changes += [{"input": "input:" + item.name, "from": before[3][item.name], "to": after[3][item.name]}
                for item in model.inputs if before[3][item.name] != after[3][item.name]]
    return changes


def change_text(change):
    return f"{change['input'].partition(':')[2] or change['input']} from {value_text(change['from'])} to {value_text(change['to'])}"


def why_not(model, state, variables, event, context, want, scope="context", checker_identity="unbound"):
    """Search the stated scope for the fewest input changes that give the wanted outcome, traps included."""
    model._validate_input(state, variables, event, context)
    current, overlap = outcome_of(model, state, variables, event, context)
    if overlap:
        raise Refusal("Overlapping transitions; no precedence is implied")
    target = target_of(model, want)
    if scope not in {"context", "input"}:
        raise Refusal("Search scope is context (configuration and event fixed) or input (every declared input)")
    original = (state, dict(variables), event, dict(context))
    searched, overlaps, found, elsewhere = 0, 0, [], 0
    for candidate in model.inputs_domain():
        in_scope = scope == "input" or candidate[:3] == original[:3]
        outcome, overlap = outcome_of(model, *candidate)
        if overlap:
            overlaps += in_scope
            continue
        if not in_scope:
            elsewhere += matches(outcome, target)
            continue
        searched += 1
        if matches(outcome, target):
            found.append((len(changes_between(model, original, candidate)), candidate, outcome))
    distance = min((item[0] for item in found), default=None)
    minimal = [item for item in found if item[0] == distance]
    reachable = reachable_configurations(model)
    witnesses = [{"input": {"state": s, "variables": dict(v), "event": e, "context": dict(c)},
                  "changes": changes_between(model, original, (s, v, e, c)), "outcome": outcome,
                  "rule": outcome["rule"], "statement": statement(model, outcome["rule"]),
                  "source_reachable": (s, tuple(sorted(v.items()))) in reachable,
                  "admissible": admissible(model, s, v)} for _, (s, v, e, c), outcome in minimal[:MAX_WITNESSES]]
    probe = B._Step(state, dict(variables), event, dict(context))
    producing = []
    for rule in model.rules:
        result = {"class": "Accept" if rule.target else "Reject", "state": rule.target or rule.source,
                  "reason": rule.reason, "rule": rule.id}
        if matches(result, {k: v for k, v in target.items() if k not in {"variables", "variable"}}) and target.get("class") != "Trap":
            producing.append({"rule": rule.id, "source_matches": rule.source == state, "event_matches": rule.event == event,
                              "guard": rule.guard.render(True), "guard_value": B.holds(rule.guard, probe),
                              "statement": statement(model, rule.id)})
    phrase = (f"state {state}, variables and event {event} fixed, every valuation of "
              f"{', '.join([*model.facts, *(i.name for i in model.inputs)]) or 'no context'}"
              if scope == "context" else "every declared state, variable, event and context valuation")
    summary = [f"From {configuration(state, variables)}, event {event} with {observations(context)}: "
               f"{decider_text(current)} {outcome_text(current)}."]
    if distance == 0:
        status = "already"
        summary.append(f"The current input already gives {want_text(target)}.")
    elif found:
        status = "found"
        summary.append(f"The fewest input changes that give {want_text(target)}: {distance} change{'' if distance == 1 else 's'}; "
                       f"{len(minimal)} such input{'' if len(minimal) == 1 else 's'} in scope.")
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
    return bounded({"kind": "why-not", "schema": WHY_NOT, "profile": model.profile, "revision": model.revision,
                    "checker": checker_identity,
                    "input": {"state": state, "variables": dict(variables), "event": event, "context": dict(context)},
                    "outcome": current, "want": target, "status": status, "distance": distance,
                    "minimal_count": len(minimal), "minimal_changes": witnesses, "producing_rules": producing,
                    "scope": {"search": scope, "statement": phrase, "inputs_searched": searched,
                              "overlapping_inputs_skipped": overlaps,
                              "outside_scope_matches": elsewhere if scope == "context" else None,
                              "minimality": "fewest changed inputs (state, event, each variable, fact and integer input count one each)",
                              "reachability": "inputs are declared hypotheses; source_reachable reports configuration reachability "
                                              "from the initial configuration, admissible whether every invariant holds there"},
                    "summary": summary,
                    "method": "exhaustive enumeration of the stated finite scope; typed model decisions; no language model",
                    "authority": "none"})


def what_if(model, state, variables, event, context, change, checker_identity="unbound"):
    """Apply every named change, integers included, and compare both decisions rule by rule."""
    if not isinstance(change, dict) or not change or set(change) - {"state", "event", "variables", "context"}:
        raise Refusal("A what-if changes state, event, named variables or named context values")
    stored, observed = change.get("variables", {}), change.get("context", {})
    if not isinstance(stored, dict) or not isinstance(observed, dict) or \
            "variables" in change and not stored or "context" in change and not observed:
        raise Refusal("A variable or context change names at least one declared name")
    unknown = sorted(set(stored) - set(model._ranges)) + sorted(set(observed) - set(model.facts) - {i.name for i in model.inputs})
    if unknown:
        raise Refusal("What-if names an undeclared variable or context value: " + ", ".join(unknown))
    after_state, after_event = change.get("state", state), change.get("event", event)
    after_values = {name: stored.get(name, value) for name, value in variables.items()}
    after_context = {name: observed.get(name, value) for name, value in context.items()}
    before = B.explain(model, state, variables, event, context, checker_identity)
    after = B.explain(model, after_state, after_values, after_event, after_context, checker_identity)
    kinds = {**{name: "context:" for name in model.facts}, **{item.name: "input:" for item in model.inputs}}
    requested = ([("state", state, change["state"])] if "state" in change else []) + \
                [("variable:" + v.name, variables.get(v.name), stored[v.name]) for v in model.variables if v.name in stored] + \
                ([("event", event, change["event"])] if "event" in change else []) + \
                [(kinds[name] + name, context.get(name), observed[name]) for name in [*model.facts, *(i.name for i in model.inputs)]
                 if name in observed]
    seen = {"state": after["input"]["state"], "event": after["input"]["event"],
            **{"variable:" + name: value for name, value in after["input"]["variables"].items()},
            **{kinds[name] + name: value for name, value in after["input"]["context"].items()}}
    applied = []
    for name, old, new in requested:
        if seen[name] != new:
            raise Refusal(f"What-if did not apply the requested change to {name}")
        applied.append({"input": name, "from": old, "to": new, "effective": old != new})
    rules = []
    for old, new in zip(before["rules"], after["rules"]):
        view = lambda r: {"source_matches": r["state_matches"], "event_matches": r["event_matches"],
                          "guard_value": r["guard"]["value"], "enabled": r["enabled"],
                          "updates": [[u["variable"], u["after"], u["in_range"]] for u in r["updates"]]}
        old_view, new_view = view(old), view(new)
        rules.append({"id": old["id"], "guard": old["guard_english"], "before": old_view, "after": new_view,
                      "changed": [key for key in old_view if old_view[key] != new_view[key]]})
    outcome_changes = sorted(key for key in set(before["outcome"]) | set(after["outcome"])
                             if before["outcome"].get(key) != after["outcome"].get(key))
    requirements = [{"id": a["id"], "before": b["value"], "after": a["value"]}
                    for b, a in zip(before["requirements"], after["requirements"]) if b["value"] != a["value"]]
    summary = [f"Changes: {'; '.join(change_text(item) for item in applied)}."]
    summary.extend(f"{item['input'].partition(':')[2] or item['input']} is already {value_text(item['to'])}; that change has no effect."
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
    summary.extend(f"Requirement {item['id']} becomes {value_text(item['after'], True)} at this step." for item in requirements)
    if before["admissible"] != after["admissible"]:
        summary.append("The pre-configuration is " + ("now admissible." if after["admissible"] else "no longer admissible: an invariant fails."))
    return bounded({"kind": "what-if", "schema": WHAT_IF, "profile": model.profile, "revision": model.revision,
                    "checker": checker_identity, "before": {"input": before["input"], "outcome": before["outcome"],
                                                            "admissible": before["admissible"]},
                    "after": {"input": after["input"], "outcome": after["outcome"], "admissible": after["admissible"]},
                    "changes": applied, "outcome_changes": outcome_changes, "rules": rules, "requirements": requirements,
                    "summary": summary,
                    "scope": "Two exact hypothetical inputs; every named change is applied and reported. "
                             "Context values are assumptions, not authenticated external truth.",
                    "method": "typed AST evaluation of both inputs; no language model", "authority": "none"})


# Narrative sentences: template id -> (English, symbolic). Slots come only from
# sentence facts and the model objects those facts name.
TEMPLATES = {
    "start": ("The machine starts in {configuration}.", "initial {symbolic_configuration};"),
    "step": ("{label}: in {configuration}, event {event} arrives with {observations}; {decider} {outcome}.",
             "{symbolic_configuration} + {event} ({symbolic_observations}) -> {symbolic_outcome} [{rule}]"),
    "end": ("After {count} step{plural} the machine is in {configuration}.", "{symbolic_configuration}"),
    "input": ("In {configuration}, event {event} arrives with {observations}.",
              "{symbolic_configuration} + {event} ({symbolic_observations})"),
    "raw_input": ("Take {configuration}, an admissible configuration whether or not it is reachable, and event {event} "
                  "with {observations}.", "{symbolic_configuration} + {event} ({symbolic_observations})"),
    "enabled": ("Rule {rule} is enabled here (guard: {guard}), and it would {intent}.",
                "rule {rule}: [{symbolic_guard}] = true -> {symbolic_intent}"),
    "overlap": ("{count} rules are enabled together, so the next configuration is not determined; this profile gives no precedence.",
                "determinism: {rules} all enabled"),
    "trap": ("Rule {rule} would set {variable} to {value}, outside {low}..={high}, so the step is refused; a step never wraps around.",
             "trap {variable}={value} outside {low}..={high} [{rule}]"),
    "accepts": ("Rule {rule} accepts and moves to {configuration}.", "-> accept {symbolic_configuration} [{rule}]"),
    "violates": ("Invariant {invariant} requires {formula}, which is false in {configuration}.",
                 "invariant {invariant}: {symbolic_formula}; {symbolic_configuration} gives false"),
    "requirement": ("Requirement {requirement} requires {formula}, which is false at step {n}.",
                    "require {requirement}: {symbolic_formula} = false at step {n}"),
    "condition": ("Its condition ({premise}) holds at step {n}, and its conclusion ({conclusion}) does not.",
                  "premise = true; conclusion = false at step {n}"),
    "safety": ("Property {property} requires {formula}; at step {n}, {goal} is false.",
               "property {property}: {symbolic_goal} = false at step {n}"),
    "never_fails": ("Property {property} requires {formula}; at step {n}, {bad} holds.",
                    "property {property}: {symbolic_bad} = true at step {n}"),
    "trigger": ("Step {n} triggers property {property} ({formula}): {trigger} holds there.",
                "property {property}: trigger {symbolic_trigger} = true at step {n}"),
    "initial_trigger": ("Property {property} ({formula}) applies from the first step.",
                        "property {property}: applies at step 1"),
    "next_fails": ("The next step, step {n}, exists and {goal} is false there.", "next: {symbolic_goal} = false at step {n}"),
    "bounded_fails": ("{goal} is false at steps {first} through {last}: the triggering step and the {bound} events after it.",
                      "within {bound}: {symbolic_goal} = false at steps {first}..={last}"),
    "cycle": ("From here the next {count} step{plural} repeat{verb} forever.", "cycle of {count} step{plural}"),
    "fair": ("Fairness {fairness} ({formula}) holds at cycle step {position}, so this execution satisfies it.",
             "fairness {fairness}: {symbolic_formula} = true at cycle step {position}"),
    "never_happens": ("{goal} is false at every step from step {n} on, including every step of the repeating cycle, so it never happens.",
                      "{symbolic_goal} = false from step {n} and on the cycle"),
    "until_breaks": ("At step {n}, neither {goal} nor {hold} holds, so the until fails before its goal.",
                     "until: {symbolic_goal} = false and {symbolic_hold} = false at step {n}"),
}
ROW_KEYS = ("state", "variables", "event", "context", "outcome")


def lookup(root, path):
    """A backing is a key path into the witness, the length of the list at one, or one plus the integer at one."""
    if isinstance(path, dict):
        return len(lookup(root, path["length_of"])) if "length_of" in path else lookup(root, path["plus_one"]) + 1
    value = root
    for key in path:
        value = value[key]
    return value


def as_step(row):
    """Rebuild one evaluated step from a witness row, for independent formula judgments."""
    step = B._Step(row["state"], dict(row["variables"]), row["event"], dict(row["context"]))
    outcome = row["outcome"]
    step.accepted, step.post_state, step.post_values = outcome["class"] == "Accept", outcome["state"], dict(outcome["variables"])
    step.rule, step.reason = outcome["rule"], outcome["reason"]
    return step


def named(model, kind, name):
    return dict(getattr(model, kind))[name]


def shape(model, name):
    return B.property_shape(named(model, "properties", name))


def slots(model, template, facts):
    """Derive every displayed slot from the sentence's facts and the model objects they name."""
    values = dict(facts)
    if "state" in facts and "variables" in facts:
        values.update(configuration=configuration(facts["state"], facts["variables"]),
                      symbolic_configuration=configuration(facts["state"], facts["variables"], True))
    if "context" in facts:
        values.update(observations=observations(facts["context"]), symbolic_observations=observations(facts["context"], True))
    if template == "step":
        values.update(label=("Cycle step " if facts["phase"] == "cycle" else "Step ") + str(facts["n"]),
                      decider=decider_text(facts["outcome"]), outcome=outcome_text(facts["outcome"]),
                      symbolic_outcome=outcome_text(facts["outcome"], True), rule=facts["outcome"]["rule"])
    if template in {"end", "cycle"}:
        values.update(plural="" if facts["count"] == 1 else "s", verb="s" if facts["count"] == 1 else "")
    if template == "enabled":
        rule = next(r for r in model.rules if r.id == facts["rule"])
        intent = {"class": "Accept" if rule.target else "Reject", "state": rule.target or rule.source, "variables": {},
                  "reason": rule.reason, "rule": rule.id}
        updates = " and ".join(u.render(True) for u in rule.updates)
        values.update(guard=rule.guard.render(True), symbolic_guard=rule.guard.render(),
                      intent=outcome_text(intent, infinitive=True) + (f" with {updates}" if updates else ""),
                      symbolic_intent=", ".join([outcome_text(intent, True), *(u.render() for u in rule.updates)]))
    if template == "overlap":
        values.update(count=len(facts["rules"]), rules=", ".join(facts["rules"]))
    if template == "trap":
        values.update(low=facts["range"][0], high=facts["range"][1])
    if template == "violates":
        formula = named(model, "invariants", facts["invariant"])
        values.update(formula=formula.render(True), symbolic_formula=formula.render())
    if template in {"requirement", "condition"}:
        formula = named(model, "requirements", facts["requirement"])
        values.update(formula=formula.render(True), symbolic_formula=formula.render())
        if template == "condition":
            values.update(premise=B._bare(formula.args[0].render(True)), conclusion=B._bare(formula.args[1].render(True)))
    if "property" in facts:
        found = shape(model, facts["property"])
        for key in ("goal", "trigger", "hold"):
            if found.get(key) is not None:
                values.update({key: B._bare(found[key].render(True)), "symbolic_" + key: found[key].render()})
        formula = named(model, "properties", facts["property"])
        values.update(formula=formula.render(True), bound=found["bound"])
        if found["operator"] == "never":
            values.update(bad=B._bare(formula.args[0].render(True)), symbolic_bad=B._bare(formula.args[0].render()))
    if template == "fair":
        formula = named(model, "fairness", facts["fairness"])
        values.update(formula=B._bare(formula.render(True)), symbolic_formula=B._bare(formula.render()))
    return values


def sentence(model, template, facts, backing):
    values = slots(model, template, facts)
    english, symbolic = TEMPLATES[template]
    return {"template": template, "facts": facts, "backing": backing,
            "english": english.format(**values), "symbolic": symbolic.format(**values)}


# Step numbers are positions in the story, re-checked against the narrated steps; a
# fairness name is the key of its backed position.
STRUCTURAL = {"step": {"n", "phase"}, "trigger": {"n"}, "initial_trigger": {"n"}, "requirement": {"n"}, "condition": {"n"},
              "safety": {"n"}, "never_fails": {"n"}, "next_fails": {"n"}, "bounded_fails": {"first", "last"}, "never_happens": {"n"},
              "until_breaks": {"n"}, "fair": {"fairness"}}


def verify(model, root, sentences):
    """Independently re-derive every sentence and the lasso it tells; return the unbacked ones."""
    problems, rows, cycle, cycle_marker, started = [], {}, [], None, False
    current = (model.initial, model.initial_values())
    def fail(index, template, reason):
        problems.append(f"sentence {index + 1} ({template}): {reason}")
    for index, item in enumerate(sentences):
        facts, template = item["facts"], item["template"]
        report = lambda reason: fail(index, template, reason)
        if template not in TEMPLATES or set(item) != {"n", "template", "facts", "backing", "english", "symbolic"}:
            report("unregistered template or fields")
            continue
        try:
            if any(lookup(root, path) != facts[key] for key, path in item["backing"].items()):
                report("a fact differs from its witness path")
            unbacked = set(facts) - set(item["backing"]) - STRUCTURAL.get(template, set())
            if unbacked:
                report("facts without a witness path: " + ", ".join(sorted(unbacked)))
            if (item["english"], item["symbolic"]) != tuple(text.format(**slots(model, template, facts)) for text in TEMPLATES[template]):
                report("text is not its template filled from the facts")
            if template == "start":
                started = True
                if (facts["state"], facts["variables"]) != current:
                    report("the start is not the declared initial configuration")
            if template == "step":
                row = {key: facts[key] for key in ROW_KEYS}
                if (row["state"], row["variables"]) != current:
                    report("the step does not start where the previous step ended")
                if row["outcome"]["class"] == "Trap" or model.decide(row["state"], row["variables"], row["event"], row["context"]) != row["outcome"]:
                    report("the model does not decide this step this way")
                current = (row["outcome"]["state"], row["outcome"]["variables"])
                if facts["phase"] == "cycle":
                    if cycle_marker is None or facts["n"] != len(cycle) + 1:
                        report("a cycle step outside a numbered cycle")
                    cycle.append(row)
                    if cycle_marker is not None and len(cycle) == cycle_marker[1] and current != cycle_marker[0]:
                        report("the cycle does not return to where it started")
                else:
                    if cycle_marker is not None or facts["n"] != len(rows) + 1:
                        report("steps are not numbered consecutively before the cycle")
                    rows[facts["n"]] = row
            if template == "cycle":
                if facts["count"] < 1 or cycle_marker is not None:
                    report("a lasso needs exactly one nonempty cycle")
                cycle_marker = (current, facts["count"])
            if template == "end" and ((facts["state"], facts["variables"]) != current or facts["count"] != len(rows)):
                report("the final configuration or step count differs from the narrated steps")
            if template in {"input", "raw_input"}:
                model._validate_input(facts["state"], facts["variables"], facts["event"], facts["context"])
                if template == "input" and started and (facts["state"], facts["variables"]) != current:
                    report("the input does not start where the steps ended")
                if template == "raw_input":
                    if not admissible(model, facts["state"], facts["variables"]):
                        report("the narrated configuration is not admissible")
                    current = (facts["state"], facts["variables"])
            given = next((s["facts"] for s in sentences if s["template"] in {"input", "raw_input"}), None)
            if template in {"enabled", "overlap"}:
                enabled = model.outcome(given["state"], dict(given["variables"]), given["event"], dict(given["context"]))
                ids = [r.id for r in enabled[1]] if enabled[0] == "overlap" else []
                if template == "enabled" and facts["rule"] not in ids or template == "overlap" and (ids != list(facts["rules"]) or len(ids) < 2):
                    report("the enabled rules differ from the narrated overlap")
            if template == "trap":
                low, high = model._ranges[facts["variable"]]
                result = model.outcome(given["state"], dict(given["variables"]), given["event"], dict(given["context"]))
                if result[0] != "trap" or (result[1].id, result[2], result[3]) != (facts["rule"], facts["variable"], facts["value"]) \
                        or facts["range"] != [low, high] or low <= facts["value"] <= high:
                    report("the narrated input does not leave the range this way")
            if template == "accepts":
                decided = model.decide(given["state"], given["variables"], given["event"], given["context"])
                if (decided["class"], decided["state"], decided["variables"], decided["rule"]) != \
                        ("Accept", facts["state"], facts["variables"], facts["rule"]):
                    report("the narrated input does not accept this way")
            if template == "violates" and B.holds(named(model, "invariants", facts["invariant"]),
                                                   B._Step(facts["state"], facts["variables"], None, {})):
                report("the invariant holds in the narrated configuration")
            step_at = lambda n: as_step(rows[n])
            if template == "requirement" and B.holds(named(model, "requirements", facts["requirement"]), step_at(facts["n"])):
                report("the requirement holds at the narrated step")
            if template == "condition":
                formula = named(model, "requirements", facts["requirement"])
                if formula.op != "implies" or not B.holds(formula.args[0], step_at(facts["n"])) or B.holds(formula.args[1], step_at(facts["n"])):
                    report("the condition and conclusion are not as narrated")
            if "property" in facts:
                found = shape(model, facts["property"])
                goal = lambda step: B.holds(found["goal"], step)
                if template in {"safety", "never_fails"} and (goal(step_at(facts["n"])) or facts["n"] != len(rows)
                                                              or (template == "never_fails") != (found["operator"] == "never")):
                    report("the safety goal holds at the narrated step, or it is not the last")
                if template == "trigger" and (found["initial"] or not B.holds(found["trigger"], step_at(facts["n"]))):
                    report("the trigger does not hold at the narrated step")
                if template == "initial_trigger" and (not found["initial"] or facts["n"] != 1):
                    report("the property does not apply from the first step")
                trigger = next((s["facts"]["n"] for s in sentences if s["template"] in {"trigger", "initial_trigger"}), None)
                if template == "next_fails" and (trigger is None or facts["n"] != trigger + 1 or goal(step_at(facts["n"]))):
                    report("the next step is not the narrated failure")
                if template == "bounded_fails" and (trigger != facts["first"] or facts["last"] - facts["first"] != found["bound"]
                                                    or any(goal(step_at(n)) for n in range(facts["first"], facts["last"] + 1))):
                    report("the bounded window is not as narrated")
                if template == "never_happens" and (trigger is None or facts["n"] != trigger
                                                    or any(goal(step_at(n)) for n in rows if n >= facts["n"])):
                    report("the goal happens after the narrated step")
                if template == "until_breaks":
                    waiting = range(trigger, facts["n"]) if trigger is not None else range(0)
                    hold = lambda step: B.holds(found["hold"], step)
                    if trigger is None or goal(step_at(facts["n"])) or hold(step_at(facts["n"])) or \
                            any(goal(step_at(n)) or not hold(step_at(n)) for n in waiting):
                        report("the until does not break as narrated")
            if template == "fair" and (len(cycle) != (cycle_marker or (None, -1))[1] or
                                       not B.holds(named(model, "fairness", facts["fairness"]), as_step(cycle[facts["position"] - 1]))):
                report("the fairness formula does not hold at the narrated cycle step")
        except (KeyError, IndexError, TypeError, StopIteration, AttributeError, Refusal) as error:
            report("cannot be re-derived: " + type(error).__name__)
    if cycle_marker is not None:
        if len(cycle) != cycle_marker[1]:
            problems.append("the lasso's cycle has a different number of steps than narrated")
        never = next((s["facts"] for s in sentences if s["template"] == "never_happens"), None)
        if never is not None:
            found = shape(model, never["property"])
            if any(B.holds(found["goal"], as_step(row)) for row in cycle):
                problems.append("the goal happens on the repeating cycle")
        fair = {s["facts"]["fairness"] for s in sentences if s["template"] == "fair"}
        if fair != {name for name, _ in model.fairness}:
            problems.append("not every fairness declaration is shown to hold on the cycle")
    return problems


def finish(model, kind, root, parts, checker_identity, extra=None):
    sentences = [{"n": number, **part} for number, part in enumerate(parts, 1)]
    problems = verify(model, root, sentences)
    if problems:
        raise Refusal("Narrative sentence not backed by the witness: " + problems[0])
    return bounded({"kind": "narrative", "schema": NARRATIVE, "profile": model.profile, "revision": model.revision,
                    "checker": checker_identity, "source": kind, "status": "narrated", **(extra or {}),
                    "sentences": sentences, "english": [s["english"] for s in sentences],
                    "symbolic": [s["symbolic"] for s in sentences],
                    "method": "registered sentence templates filled from witness values, each re-derived from the model; no language model",
                    "authority": "none"})


def step_part(model, root, path, n, phase="prefix", pre=()):
    """One numbered step sentence for the row at path; pre names a nested pre-configuration."""
    where = {"state": [*path, *pre, "state"], "variables": [*path, *pre, "variables"],
             "event": [*path, "event"], "context": [*path, "context"], "outcome": [*path, "outcome"]}
    return sentence(model, "step", {"n": n, "phase": phase, **{key: lookup(root, value) for key, value in where.items()}}, where)


def step_parts(model, root, path, phase="prefix", pre=()):
    return [step_part(model, root, [*path, index], index + 1, phase, pre) for index in range(len(lookup(root, path)))]


def start_part(model, root, rows_path, fallback, pre=()):
    path = [*rows_path, 0, *pre] if lookup(root, rows_path) else fallback
    return sentence(model, "start", {"state": lookup(root, [*path, "state"]), "variables": lookup(root, [*path, "variables"])},
                    {"state": [*path, "state"], "variables": [*path, "variables"]})


def given_part(model, root, template, path):
    keys = ("state", "variables", "event", "context")
    return sentence(model, template, {key: lookup(root, [*path, key]) for key in keys}, {key: [*path, key] for key in keys})


def narrate_check(model, report, checker_identity="unbound"):
    """Tell one profile 2 checker counterexample as a short numbered story."""
    status = report.get("status")
    if status == "unknown":
        if report.get("revision") != model.revision:
            raise Refusal("Check result belongs to a different revision")
        return bounded({"kind": "narrative", "schema": NARRATIVE, "profile": model.profile, "revision": model.revision,
                        "checker": checker_identity, "source": "check", "status": "unknown",
                        "check_status": "unknown", "diagnostic": report.get("diagnostic", "unknown check result"),
                        "sentences": [], "english": [], "symbolic": [],
                        "method": "the check result is unknown; no game result is narrated",
                        "authority": "none"})
    if status != "counterexample":
        return bounded({"kind": "narrative", "schema": NARRATIVE, "profile": model.profile, "revision": model.revision,
                        "checker": checker_identity, "source": "check", "status": "no-counterexample",
                        "check_status": report.get("status"), "sentences": [], "english": [], "symbolic": [],
                        "method": "nothing to narrate: the check reported no counterexample in its stated scope",
                        "authority": "none"})
    if report.get("revision") != model.revision:
        raise Refusal("Counterexample belongs to a different revision")
    if report.get("obligation") == "game":
        return bounded({"kind": "narrative", "schema": NARRATIVE, "profile": model.profile, "revision": model.revision,
                        "checker": checker_identity, "source": "check", "status": "counterexample",
                        "check_status": "counterexample", "obligation": "game", "property": report["property"],
                        "player": report["player"], "objective": report["objective"],
                        "initial": report["initial"], "sentences": [], "english": [], "symbolic": [],
                        "method": "the game property loses from the initial configuration; no finite trace or strategy is claimed",
                        "authority": "none"})
    obligation, witness = report.get("obligation"), report["witness"]
    W = ["witness"]
    if obligation == "determinism":
        parts = [given_part(model, report, "input", W)]
        parts += [sentence(model, "enabled", {"rule": rule}, {"rule": ["rules", i]}) for i, rule in enumerate(report["rules"])]
        parts.append(sentence(model, "overlap", {"rules": report["rules"]}, {"rules": ["rules"]}))
    elif obligation == "range":
        trap = sentence(model, "trap", {key: witness[key] for key in ("rule", "variable", "value", "range")},
                        {key: [*W, key] for key in ("rule", "variable", "value", "range")})
        if witness["reachable"]:
            parts = [start_part(model, report, [*W, "trace"], W), *step_parts(model, report, [*W, "trace"]),
                     given_part(model, report, "input", W), trap]
        else:
            parts = [given_part(model, report, "raw_input", W), trap]
    elif obligation == "invariant":
        parts = [start_part(model, report, [*W, "trace"], W), *step_parts(model, report, [*W, "trace"]),
                 sentence(model, "end", {"count": len(witness["trace"]), "state": witness["state"], "variables": witness["variables"]},
                          {"count": {"length_of": [*W, "trace"]}, "state": [*W, "state"], "variables": [*W, "variables"]}),
                 sentence(model, "violates", {"invariant": report["invariant"], "state": witness["state"], "variables": witness["variables"]},
                          {"invariant": ["invariant"], "state": [*W, "state"], "variables": [*W, "variables"]})]
    elif obligation == "invariant-preservation":
        post = [*W, "outcome"]
        parts = [given_part(model, report, "raw_input", W),
                 sentence(model, "accepts", {"rule": witness["outcome"]["rule"], "state": witness["outcome"]["state"],
                                             "variables": witness["outcome"]["variables"]},
                          {"rule": [*post, "rule"], "state": [*post, "state"], "variables": [*post, "variables"]}),
                 sentence(model, "violates", {"invariant": report["invariant"], "state": witness["outcome"]["state"],
                                              "variables": witness["outcome"]["variables"]},
                          {"invariant": ["invariant"], "state": [*post, "state"], "variables": [*post, "variables"]})]
    elif obligation == "requirement":
        n = len(witness["trace"] or []) + 1
        if witness["reachable"]:
            parts = [start_part(model, report, [*W, "trace"], [*W, "step"]), *step_parts(model, report, [*W, "trace"])]
        else:
            parts = [given_part(model, report, "raw_input", [*W, "step"])]
        parts.append(step_part(model, report, [*W, "step"], n))
        parts.append(sentence(model, "requirement", {"requirement": report["requirement"], "n": n}, {"requirement": ["requirement"]}))
        if named(model, "requirements", report["requirement"]).op == "implies":
            parts.append(sentence(model, "condition", {"requirement": report["requirement"], "n": n}, {"requirement": ["requirement"]}))
    elif obligation == "property":
        parts = property_parts(model, report)
    else:
        raise Refusal(f"No narrative template for obligation {obligation}; the structured witness remains authoritative")
    return finish(model, "check", report, parts, checker_identity, {"obligation": obligation})


def property_parts(model, report):
    witness, W = report["witness"], ["witness"]
    name, kind = report["property"], witness["kind"]
    found = shape(model, name)
    about, backing = {"property": name}, {"property": ["property"]}
    key = "trace" if "trace" in witness else "prefix"
    rows = witness[key]
    parts = [start_part(model, report, [*W, key], W), *step_parts(model, report, [*W, key])]
    def triggered(n):
        return (sentence(model, "initial_trigger", {**about, "n": 1}, backing) if found["initial"] else
                sentence(model, "trigger", {**about, "n": n}, backing))
    if kind == "safety":
        return [*parts, sentence(model, "never_fails" if found["operator"] == "never" else "safety", {**about, "n": len(rows)}, backing)]
    if kind == "next":
        return [*parts, triggered(len(rows) - 1), sentence(model, "next_fails", {**about, "n": len(rows)}, backing)]
    if kind == "bounded":
        first = len(rows) - found["bound"]
        return [*parts, triggered(first), sentence(model, "bounded_fails", {**about, "first": first, "last": len(rows)}, backing)]
    # A fair lasso: the prefix, then a cycle that repeats forever.
    parts.append(sentence(model, "cycle", {"count": len(witness["cycle"])}, {"count": {"length_of": [*W, "cycle"]}}))
    parts += step_parts(model, report, [*W, "cycle"], "cycle")
    parts += [sentence(model, "fair", {"fairness": fairness, "position": position + 1},
                       {"position": {"plus_one": [*W, "fairness_positions", fairness]}})
              for fairness, position in witness["fairness_positions"].items()]
    trigger = lasso_trigger(model, found, rows, kind, witness["violation_length"])
    parts.append(triggered(trigger))
    if kind == "liveness":
        parts.append(sentence(model, "never_happens", {**about, "n": trigger}, backing))
    else:
        parts.append(sentence(model, "until_breaks", {**about, "n": witness["violation_length"]}, backing))
    return parts


def lasso_trigger(model, found, rows, kind, violation_length):
    """The triggering step of a lasso counterexample, numbered from 1; the verifier re-checks it."""
    steps = [as_step(row) for row in rows]
    goal = lambda step: B.holds(found["goal"], step)
    hold = lambda step: found["hold"] is None or B.holds(found["hold"], step)
    for n in [1] if found["initial"] else [n for n, step in enumerate(steps, 1) if B.holds(found["trigger"], step)]:
        if kind == "liveness" and not any(goal(step) for step in steps[n - 1:]):
            return n
        if kind == "liveness-finite" and n <= violation_length and \
                all(hold(step) and not goal(step) for step in steps[n - 1:violation_length - 1]) and \
                not goal(steps[violation_length - 1]) and not hold(steps[violation_length - 1]):
            return n
    raise Refusal("The lasso has no triggering step that the narrated violation follows")


def narrate_trace(model, inputs, checker_identity="unbound"):
    """Replay assumed inputs from the initial configuration and tell them as a story."""
    rows = B.replay(model, inputs)
    root = {"initial": {"state": model.initial, "variables": model.initial_values()}, "trace": rows}
    final = ["trace", len(rows) - 1, "outcome"] if rows else ["initial"]
    parts = [sentence(model, "start", dict(root["initial"]), {"state": ["initial", "state"], "variables": ["initial", "variables"]})]
    parts += step_parts(model, root, ["trace"], pre=("pre",))
    parts.append(sentence(model, "end", {"count": len(rows), "state": lookup(root, [*final, "state"]),
                                         "variables": lookup(root, [*final, "variables"])},
                          {"count": {"length_of": ["trace"]}, "state": [*final, "state"], "variables": [*final, "variables"]}))
    return finish(model, "trace", root, parts, checker_identity,
                  {"scope": "Assumed inputs replayed from the declared initial configuration; context values are assumptions."})
