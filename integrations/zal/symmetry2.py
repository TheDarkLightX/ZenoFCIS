"""Exhaustive, observer-preserving symmetry checks for finite ZAL profile 2.

Policy consists only of owned tuples. All maps, tables and search queues below
belong to one check invocation; none are retained on the decision machine.
"""

from collections import deque
import re

import behavior2 as b


def refuse(name, obligation, detail):
    raise b.Refusal(f"Symmetry {name}: {obligation}: {detail}")


def parse_declarations(items, english=False):
    declarations = []
    for name, body in items:
        pattern = rf"({b.NAME}) with ({b.NAME})" if english else rf"\(({b.NAME}) ({b.NAME})\)"
        parts = body.split(" and ") if english else re.findall(r"\([^()]*\)", body)
        if not parts or (not english and " ".join(parts) != body):
            refuse(name, "declaration", "expected disjoint swap pairs")
        pairs = []
        for part in parts:
            match = re.fullmatch(pattern, part)
            if not match:
                refuse(name, "declaration", "expected disjoint swap pairs")
            pairs.append(tuple(sorted(match.groups())))
        declarations.append((name, tuple(sorted(pairs))))
    return tuple(sorted(declarations))


def validate_declarations(model):
    items = model.symmetries
    if not isinstance(items, tuple):
        refuse("declarations", "declaration", "symmetries must be owned tuples")
    seen = set()
    variables = {v.name: (v.low, v.high, v.initial) for v in model.variables}
    kinds = [set(variables), set(model.states), set(model.events)]
    object_ids = {r.id for r in model.rules}
    object_ids.update(n for collection in (model.invariants, model.requirements, model.fairness, model.properties)
                      for n, _ in collection)
    object_ids.update(n for n, *_ in model.games)
    object_ids.update(n for n, _ in model.players)
    for item in items:
        if not isinstance(item, tuple) or len(item) != 2 or not isinstance(item[0], str):
            refuse("declarations", "declaration", "expected (ID, pairs) tuples")
        name, pairs = item
        if (not re.fullmatch(b.NAME, name) or name in b.RESERVED or name in seen or name in object_ids
                or not isinstance(pairs, tuple) or not pairs):
            refuse(name, "declaration", "distinct statement ID and nonempty owned pairs required")
        seen.add(name)
        if len(seen) > 4:
            refuse(name, "finiteness", "at most four generators")
        used = set()
        for pair in pairs:
            if (not isinstance(pair, tuple) or len(pair) != 2
                    or any(not isinstance(x, str) for x in pair)):
                refuse(name, "declaration", "expected owned name pairs")
            a, c = pair
            if a == c or used & set(pair) or not any(set(pair) <= kind for kind in kinds):
                refuse(name, "declaration", "pairs must be disjoint declared names of the same kind")
            if sum(set(pair) <= kind for kind in kinds) != 1:
                refuse(name, "declaration", "swapped names have ambiguous declaration kinds")
            if a in variables and variables[a] != variables[c]:
                refuse(name, "declaration", "swapped variables need the same range and initial value")
            if tuple(sorted(pair)) != pair:
                refuse(name, "declaration", "pairs must be canonically sorted")
            used.update(pair)
        if tuple(sorted(pairs)) != pairs:
            refuse(name, "declaration", "pairs must be canonically sorted")
    if tuple(sorted(items)) != items:
        refuse("declarations", "declaration", "statements must be canonically sorted")


def rename(pairs):
    return {a: c for pair in pairs for a, c in (pair, pair[::-1])}


def configuration(model, permutation, state, values):
    return (permutation.get(state, state),
            {permutation.get(n, n): value for n, value in values.items()})


def transformed_step(model, step, permutation):
    state, values = configuration(model, permutation, step.state, step.values)
    result = model.outcome(state, values, permutation.get(step.event, step.event), dict(step.context))
    return result


def _effect(model, rule, state, values, event, context, permutation):
    """Complete enabled-rule outcome, including the first arithmetic refusal."""
    if rule.target is None:
        return ["Reject", rule.reason, permutation.get(state, state),
                sorted((permutation.get(n, n), v) for n, v in values.items())]
    probe = b._Step(state, values, event, context)
    post = dict(values)
    for update in rule.updates:
        operand = b.value_of(update.term, probe)
        before = values[update.variable]
        after = operand if update.op == "=" else before + operand if update.op == "+=" else before - operand
        low, high = model._ranges[update.variable]
        if not low <= after <= high:
            return ["trap", permutation.get(update.variable, update.variable), after]
        post[update.variable] = after
    return ["Accept", None, permutation.get(rule.target, rule.target),
            sorted((permutation.get(n, n), v) for n, v in post.items())]


def _observe(model, result, state, values, event, context, permutation):
    if result[0] == "overlap":
        rows = [(_effect(model, rule, state, values, event, context, permutation), rule.id)
                for rule in result[1]]
        return "overlap", sorted(rows, key=lambda row: b.canonical(row))
    if result[0] == "trap":
        _, rule, variable, value = result
        return "trap", [(["trap", permutation.get(variable, variable), value], rule.id)]
    step = result[1]
    return "step", [(["Accept" if step.accepted else "Reject", step.reason,
                       permutation.get(step.post_state, step.post_state),
                       sorted((permutation.get(n, n), v) for n, v in step.post_values.items())], step.rule)]


def _claims(model):
    for collection in (model.invariants, model.requirements, model.fairness, model.properties):
        yield from collection
    yield from ((name, formula) for name, _, _, formula in model.games)


def _step_predicates(formula):
    # Equality of the complete non-temporal labels at every position preserves
    # all the supported temporal constructors, including bounded next/until.
    if not b._temporal(formula):
        yield formula
    else:
        for arg in formula.args:
            if isinstance(arg, b.Expr):
                yield from _step_predicates(arg)


def check_automorphism(model, name, permutation):
    initial = model.initial, model.initial_values()
    if configuration(model, permutation, *initial) != initial:
        refuse(name, "automorphism", "initial configuration is moved")
    mapping = {"default": "default"}
    for state, values, event, context in model.inputs_domain():
        other_state, other_values = configuration(model, permutation, state, values)
        other_event = permutation.get(event, event)
        left = _observe(model, model.outcome(state, values, event, context),
                        state, values, event, context, permutation)
        right = _observe(model, model.outcome(other_state, other_values, other_event, context),
                         other_state, other_values, other_event, context, {})
        if left[0] != right[0] or [row[0] for row in left[1]] != [row[0] for row in right[1]]:
            refuse(name, "automorphism", f"outcome changes at {state}, {values}, {event}, {context}")
        for (_, source), (_, target) in zip(left[1], right[1]):
            if source in mapping and mapping[source] != target:
                refuse(name, "automorphism", f"rule outcome {source} has no consistent renaming")
            mapping[source] = target
    if any(mapping.get(target) != source for source, target in mapping.items()):
        refuse(name, "automorphism", "rule outcomes do not form an involution")


def check_claims(model, name, permutation):
    for claim, formula in _claims(model):
        for predicate in _step_predicates(formula):
            for state, values, event, context in model.inputs_domain():
                result = model.outcome(state, values, event, context)
                if result[0] != "step":
                    continue
                other = transformed_step(model, result[1], permutation)
                if other[0] != "step" or b.holds(predicate, result[1]) != b.holds(predicate, other[1]):
                    refuse(name, "claim-invariance", f"{claim}: the property observes a swapped name")
        # State claims apply even at configurations where every step traps.
        if any(claim == n for n, _ in model.invariants) or any(claim == n for n, *_ in model.games):
            for state, values in model.configurations():
                other_state, other_values = configuration(model, permutation, state, values)
                if b.holds(formula, b._Step(state, values, None, {})) != b.holds(
                        formula, b._Step(other_state, other_values, None, {})):
                    refuse(name, "claim-invariance", f"{claim}: the property observes a swapped name")


def check_players(model, name, permutation):
    for player, states in model.players:
        if {permutation.get(state, state) for state in states} != set(states):
            refuse(name, "players", f"the states of player {player} are moved to another player")
    # Only states, events and variables can move. Context names/values and the
    # rule assigning the opponent every context choice are therefore unchanged.


def check_obligations(model):
    for name, pairs in model.symmetries:
        permutation = rename(pairs)
        check_automorphism(model, name, permutation)
        check_claims(model, name, permutation)
        check_players(model, name, permutation)


def check(model, checker_identity, no_symmetry=False):
    check_obligations(model)
    # The graph adapter is supplied in the next implementation stage.
    result = b._check(model, checker_identity)
    return result
