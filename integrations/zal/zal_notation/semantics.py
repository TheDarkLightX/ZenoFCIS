"""Small, explicit semantic adapters for exact built-in grammar profiles.

These operations interpret supplied models in memory.  They do not execute
effects, authenticate imported evidence, establish user intent, or grant
approval.  Arbitrary grammar/notation packs never acquire these semantics by
reusing a profile's identifier: the complete grammar must match.
"""
from __future__ import annotations

import copy
from collections import defaultdict

from .builtin import fsm_grammar, lambda_grammar, petri_grammar
from .core import MAX_DEPTH, MAX_NODES, NotationError, canonical, digest, validate_model


class SemanticError(NotationError):
    """Unsupported semantic profile or a refused model operation."""


class DisabledTransition(SemanticError):
    """A well-formed Petri transition lacks the required input tokens."""


def _admit(grammar: dict, model: dict, expected: dict) -> None:
    validate_model(grammar, model)
    if grammar != expected:
        raise SemanticError("semantic adapter requires the exact supported built-in grammar")


def _report(grammar: dict, model: dict, adapter: str, scope: str) -> dict:
    return {
        "schema": "zal.semantic-report/1",
        "adapter": adapter,
        "grammar_digest": digest(grammar),
        "source_model_digest": digest(model),
        "evidence": "checked-in-this-run",
        "scope": scope,
        "authority": "none",
    }


def _to_db(node: dict, scope: tuple[str, ...] = ()) -> tuple:
    tag = node["tag"]
    if tag == "Var":
        name = node["fields"]["name"]
        try:
            return ("v", scope.index(name))
        except ValueError as exc:
            raise SemanticError(f"closed lambda terms required; free variable {name!r}") from exc
    if tag == "Lam":
        return ("l", _to_db(node["children"][0], (node["fields"]["name"],) + scope))
    return ("a", _to_db(node["children"][0], scope), _to_db(node["children"][1], scope))


def _from_db(node: tuple, scope: tuple[str, ...] = ()) -> dict:
    tag = node[0]
    if tag == "v":
        if not 0 <= node[1] < len(scope):
            raise SemanticError("internal lambda binding invariant failed")
        return {"tag": "Var", "fields": {"name": scope[node[1]]}, "children": []}
    if tag == "l":
        name = f"x{len(scope)}"
        return {"tag": "Lam", "fields": {"name": name},
                "children": [_from_db(node[1], (name,) + scope)]}
    return {"tag": "App", "fields": {},
            "children": [_from_db(node[1], scope), _from_db(node[2], scope)]}


def _shift(node: tuple, amount: int, cutoff: int = 0) -> tuple:
    if amount == 0:
        return node
    if node[0] == "v":
        return ("v", node[1] + amount if node[1] >= cutoff else node[1])
    if node[0] == "l":
        return ("l", _shift(node[1], amount, cutoff + 1))
    return ("a", _shift(node[1], amount, cutoff), _shift(node[2], amount, cutoff))


def _beta(body: tuple, argument: tuple) -> tuple:
    # Remove the redex's binder.  Under d inner binders, its occurrences have
    # index d; replacement lifts the argument by d to avoid capture.  Indices
    # outside the removed binder decrease by one.  Reuse immutable argument
    # tuples; count the expanded tree before producing a named model.
    lifted: dict[int, tuple] = {0: argument}

    def substitute(node: tuple, depth: int) -> tuple:
        if node[0] == "v":
            index = node[1]
            if index == depth:
                if depth not in lifted:
                    lifted[depth] = _shift(argument, depth)
                return lifted[depth]
            return ("v", index - 1 if index > depth else index)
        if node[0] == "l":
            return ("l", substitute(node[1], depth + 1))
        return ("a", substitute(node[1], depth), substitute(node[2], depth))

    return substitute(body, 0)


def _has_redex(node: tuple) -> bool:
    stack = [node]
    while stack:
        current = stack.pop()
        if current[0] == "a":
            if current[1][0] == "l":
                return True
            stack.extend((current[2], current[1]))
        elif current[0] == "l":
            stack.append(current[1])
    return False


def _normal_step(node: tuple) -> tuple[tuple, bool]:
    if node[0] == "v":
        return node, False
    if node[0] == "l":
        body, changed = _normal_step(node[1])
        return (("l", body), True) if changed else (node, False)
    if node[1][0] == "l":
        return _beta(node[1][1], node[2]), True
    left, changed = _normal_step(node[1])
    if changed:
        return ("a", left, node[2]), True
    right, changed = _normal_step(node[2])
    return (("a", node[1], right), True) if changed else (node, False)


def _fits(node: tuple) -> bool:
    stack = [(node, 0)]
    count = 0
    while stack:
        current, depth = stack.pop()
        count += 1
        if count > MAX_NODES or depth > MAX_DEPTH:
            return False
        if current[0] == "l":
            stack.append((current[1], depth + 1))
        elif current[0] == "a":
            stack.extend(((current[1], depth + 1), (current[2], depth + 1)))
    return True


def lambda_trace(grammar: dict, model: dict, *, max_steps: int = 16) -> dict:
    """Full normal-order beta reduction of a closed named lambda term.

    Returned ``models`` contains the initial model and each successful bounded
    reduction.  Normal-form is reported only after checking absence of beta
    redexes, including under lambdas.  A stopped trace retains its last model;
    neither budget exhaustion nor repeated terms proves nontermination.
    """
    _admit(grammar, model, lambda_grammar())
    if type(max_steps) is not int or not 0 <= max_steps <= 128:
        raise SemanticError("max_steps must be an integer in 0..128")
    current = _to_db(model["body"])
    models = [copy.deepcopy(model)]
    status = "normal-form"
    while _has_redex(current):
        if len(models) - 1 >= max_steps:
            status = "step-limit"
            break
        proposed, changed = _normal_step(current)
        if not changed:
            raise SemanticError("internal lambda redex invariant failed")
        if not _fits(proposed):
            status = "size-limit"
            break
        next_model = {"schema": model["schema"], "grammar": copy.deepcopy(model["grammar"]),
                      "kind": "term", "body": _from_db(proposed)}
        validate_model(grammar, next_model)
        models.append(next_model)
        current = proposed
    report = _report(grammar, model, "zal/lambda-beta/1", "closed lambda model; beta reduction only")
    report.update({"status": status, "models": models, "steps": len(models) - 1,
                   "strategy": "leftmost-outermost", "max_steps": max_steps,
                   "limits": {"nodes_per_model": MAX_NODES, "depth_per_model": MAX_DEPTH}})
    return report


def _fsm_data(model: dict):
    nodes = {node["id"]: node for node in model["body"]["nodes"]}
    initial = sorted(node["id"] for node in nodes.values() if node["tag"] == "start")
    transitions: dict[tuple[str, str], list[dict]] = defaultdict(list)
    for edge in model["body"]["edges"]:
        transitions[(edge["source"][0], edge["fields"]["event"])].append(edge)
    alphabet = sorted({event for _, event in transitions})
    return nodes, initial, transitions, alphabet


def fsm_check(grammar: dict, model: dict) -> dict:
    """Check an explicit finite event-labelled transition table.

    The alphabet is exactly the union of event labels present on edges.  Labels
    are uninterpreted strings, including any text resembling a guard.  Coverage
    includes every declared state, not only reachable states.  A graph with no
    edges has an empty alphabet and vacuous event coverage, which is reported.
    """
    _admit(grammar, model, fsm_grammar())
    nodes, initial, transitions, alphabet = _fsm_data(model)
    nondeterministic = [
        {"state": state, "event": event, "edges": sorted(edge["id"] for edge in edges)}
        for (state, event), edges in sorted(transitions.items()) if len(edges) > 1
    ]
    missing = []
    missing_count = 0
    for state in sorted(nodes):
        for event in alphabet:
            if (state, event) not in transitions:
                missing_count += 1
                if len(missing) < 256:
                    missing.append({"state": state, "event": event})
    initial_valid = len(initial) == 1
    deterministic = not nondeterministic
    complete = missing_count == 0
    report = _report(grammar, model, "zal/fsm-table/1",
                     "all declared states crossed with the union of declared edge event labels")
    report.update({
        "status": "deterministic-complete" if initial_valid and deterministic and complete else "violations",
        "structural_valid": True, "initial_valid": initial_valid, "initial_states": initial,
        "deterministic": deterministic, "complete": complete,
        "alphabet": alphabet, "alphabet_origin": "union-of-edge-event-labels",
        "state_count": len(nodes), "transition_count": len(model["body"]["edges"]),
        "nondeterministic": nondeterministic, "missing": missing,
        "missing_count": missing_count, "missing_omitted": missing_count - len(missing),
    })
    return report


def fsm_trace(grammar: dict, model: dict, events: list[str]) -> dict:
    """Replay exact event labels; refuse a missing or nonunique transition."""
    _admit(grammar, model, fsm_grammar())
    if (type(events) is not list or len(events) > 2048
            or any(type(event) is not str or len(event) > 1024 for event in events)):
        raise SemanticError("events must be an array of at most 2048 strings of at most 1024 characters")
    canonical(events)
    _nodes, initial, transitions, alphabet = _fsm_data(model)
    report = _report(grammar, model, "zal/fsm-trace/1", "supplied event sequence; exact edge labels")
    report.update({"status": "completed", "trace": [], "steps": 0,
                   "events": list(events), "final_state": initial[0] if len(initial) == 1 else None})
    if len(initial) != 1:
        report.update({"status": "refused", "reason": "initial-state-count", "initial_states": initial})
        return report
    state = initial[0]
    for index, event in enumerate(events):
        matches = transitions.get((state, event), ())
        if len(matches) != 1:
            reason = ("nondeterministic-transition" if matches else
                      "unknown-event" if event not in alphabet else "missing-transition")
            report.update({"status": "refused", "reason": reason,
                           "refusal": {"index": index, "state": state, "event": event,
                                       "candidate_edges": [edge["id"] for edge in matches]}})
            break
        edge = matches[0]
        target = edge["target"][0]
        report["trace"].append({"state": state, "event": event, "next": target,
                                "outcome": edge["fields"]["outcome"]})
        state = target
        report["steps"] += 1
        report["final_state"] = state
    return report


def petri_fire(grammar: dict, model: dict, transition_id: str) -> dict:
    """Fire one enabled ordinary weighted Petri-net transition.

    Tokens are nonnegative mathematical integers within the core's storage
    bounds.  Arc weights are positive.  Parallel input and output arcs are
    summed separately; output tokens cannot fund an otherwise disabled input.
    The source model is untouched and the returned model is fully revalidated.
    """
    _admit(grammar, model, petri_grammar())
    nodes = {node["id"]: node for node in model["body"]["nodes"]}
    if type(transition_id) is not str or transition_id not in nodes:
        raise SemanticError("unknown Petri transition id")
    if nodes[transition_id]["tag"] != "transition":
        raise SemanticError("requested Petri node is not a transition")
    for node in nodes.values():
        if node["tag"] == "place" and node["fields"]["tokens"] < 0:
            raise SemanticError("ordinary Petri places require nonnegative tokens")
    consumed: dict[str, int] = defaultdict(int)
    produced: dict[str, int] = defaultdict(int)
    for edge in model["body"]["edges"]:
        source, target = edge["source"][0], edge["target"][0]
        weight = edge["fields"]["weight"]
        if weight <= 0:
            raise SemanticError("ordinary Petri arcs require positive integer weights")
        if nodes[source]["tag"] == nodes[target]["tag"]:
            raise SemanticError("Petri arcs must connect a place and a transition")
        if target == transition_id:
            consumed[source] += weight
        if source == transition_id:
            produced[target] += weight
    for place, required in consumed.items():
        available = nodes[place]["fields"]["tokens"]
        if available < required:
            raise DisabledTransition(
                f"transition {transition_id!r} disabled at {place!r}: needs {required}, has {available}")
    result = copy.deepcopy(model)
    for node in result["body"]["nodes"]:
        if node["tag"] == "place":
            place = node["id"]
            node["fields"]["tokens"] += produced[place] - consumed[place]
    validate_model(grammar, result)
    return result
