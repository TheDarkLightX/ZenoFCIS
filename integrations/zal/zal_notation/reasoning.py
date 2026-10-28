"""Replayable semantic steps and bounded finite observational reasoning.

Folding is presentation.  Semantic steps use explicit built-in interpreters.
Finite relations concern supplied one-step tables and their declared observation
maps; they are not claims about arbitrary programs, trace equivalence, liveness,
human agreement, or an external adapter's correctness.  All data are inert.
"""
from __future__ import annotations

import copy
from itertools import product

from .builtin import fsm_grammar, lambda_grammar, petri_grammar
from .core import canonical, digest, keys, model_digest, name, need, validate_model
from .semantics import DisabledTransition, SemanticError, lambda_trace, petri_fire

MAX_CASES = 4096
MAX_OUTCOMES = 4096
MAX_RELATION_ENTRIES = 16384


def _finish(report: dict) -> dict:
    # A returned receipt must itself fit the ordinary exchange/data budget.
    canonical(report)
    return report


def reasoning_step(grammar: dict, model: dict, operation: dict) -> dict:
    """Replay one supported semantic step against exact source bytes.

    Requests are ``lambda-beta``, ``fsm-transition`` (event and optional state),
    or ``petri-fire`` (transition).  An FSM moves an execution configuration:
    its machine model, start tag, and accepting tags remain unchanged.  Returned
    statuses are checked, refuted (rule precondition fails), or unsupported.
    A caller must replay an imported receipt instead of trusting its status.
    """
    validate_model(grammar, model)
    canonical(operation)
    keys(operation, {"kind"}, {"event", "state", "transition"}, "reasoning operation")
    need(type(operation["kind"]) is str, "reasoning operation kind must be a string")
    source = model_digest(grammar, model)
    report = {
        "schema": "zal.reasoning-step/1", "checker": "zal.reasoning/1",
        "step_class": "semantic", "source_model_digest": source,
        "grammar_digest": digest(grammar), "replay_request": copy.deepcopy(operation),
        "status": "unsupported", "evidence": "checked-in-this-run", "authority": "none",
    }
    kind = operation["kind"]
    if kind in {"fold", "unfold", "view-fold", "view-unfold"}:
        report.update(reason="folding-is-a-view-operation",
                      scope="Retain the same model and change render view metadata; no semantic rewrite.")
        return _finish(report)
    target = None
    try:
        if kind == "lambda-beta":
            keys(operation, {"kind"}, where="lambda beta request")
            if grammar != lambda_grammar():
                report["reason"] = "unsupported-lambda-grammar"
            else:
                trace = lambda_trace(grammar, model, max_steps=1)
                report.update(rule="beta", strategy=trace["strategy"],
                              continuation_status=trace["status"],
                              scope="One leftmost-outermost beta reduction of a closed lambda term.")
                if trace["steps"] == 1:
                    target = trace["models"][1]
                elif trace["status"] == "normal-form":
                    report.update(status="refuted", reason="no-beta-redex",
                                  counterexample={"rule_precondition": "source contains a beta redex",
                                                  "observed": "beta-normal-form"})
                else:
                    report.update(reason="next-model-exceeds-size-bound", limits=trace["limits"])
        elif kind == "fsm-transition":
            keys(operation, {"kind", "event"}, {"state"}, "FSM transition request")
            need(type(operation["event"]) is str, "FSM event must be a string")
            need("state" not in operation or type(operation["state"]) is str, "FSM state must be a string when supplied")
            if grammar != fsm_grammar():
                report["reason"] = "unsupported-fsm-grammar"
            else:
                nodes = {n["id"]: n for n in model["body"]["nodes"]}
                initial = [n["id"] for n in nodes.values() if n["tag"] == "start"]
                state = operation.get("state")
                if state is None and len(initial) == 1:
                    state = initial[0]
                report["scope"] = "One event-labelled transition from a declared FSM state; the machine definition is unchanged and reachability is not asserted."
                if type(state) is not str or state not in nodes:
                    report.update(status="refuted", reason="no-valid-source-configuration",
                                  counterexample={"requested_state": state, "initial_states": initial})
                else:
                    event = operation["event"]
                    candidates = [e for e in model["body"]["edges"]
                                  if e["source"][0] == state and e["fields"]["event"] == event]
                    before = {"schema": "zal.fsm-configuration/1", "model_digest": source, "state": state}
                    report.update(source_configuration=before, source_configuration_digest=digest(before))
                    if len(candidates) != 1:
                        report.update(status="refuted", reason="transition-is-not-unique" if candidates else "transition-is-missing",
                                      counterexample={"state": state, "event": event,
                                                      "candidate_edges": [e["id"] for e in candidates]})
                    else:
                        edge = candidates[0]
                        after = dict(before, state=edge["target"][0])
                        report.update(rule="event-transition", target_configuration=after,
                                      target_configuration_digest=digest(after),
                                      transition={"edge": edge["id"], "state": state, "event": event,
                                                  "next": edge["target"][0], "outcome": edge["fields"]["outcome"]})
                        target = copy.deepcopy(model)
        elif kind == "petri-fire":
            keys(operation, {"kind", "transition"}, where="Petri firing request")
            need(type(operation["transition"]) is str, "Petri transition must be a string")
            if grammar != petri_grammar():
                report["reason"] = "unsupported-petri-grammar"
            else:
                report.update(rule="ordinary-weighted-firing",
                              scope="One enabled ordinary Petri-net firing, with parallel input/output arcs aggregated.")
                try:
                    target = petri_fire(grammar, model, operation["transition"])
                except DisabledTransition as exc:
                    report.update(status="refuted", reason="transition-disabled",
                                  counterexample={"transition": operation["transition"], "detail": str(exc)})
        else:
            report["reason"] = "unsupported-operation"
    except SemanticError as exc:
        report.update(status="unsupported", reason="unsupported-semantic-input", detail=str(exc))
    if target is not None:
        validate_model(grammar, target)
        report.update(status="checked", target_model=target,
                      target_model_digest=model_digest(grammar, target))
    return _finish(report)


def validate_table(table: dict) -> None:
    """Validate a total, nonempty, finite input-to-output-set relation.

    Inputs and outputs are unique canonical JSON values.  ``rows[i]`` is a
    nonempty list of distinct indices into outputs for input ``inputs[i]``.
    This profile has no executable expressions and no implicit error behavior.
    """
    canonical(table)
    keys(table, {"schema", "id", "version", "inputs", "outputs", "rows"},
         {"description", "origin"}, "finite table")
    need(table["schema"] == "zal.finite-table/1", "unsupported finite-table schema")
    name(table["id"], "finite table id")
    need(type(table["version"]) is int and 1 <= table["version"] <= 1_000_000, "invalid finite-table version")
    need(type(table["inputs"]) is list and 0 < len(table["inputs"]) <= MAX_CASES, "finite input domain must contain 1..4096 values")
    need(type(table["outputs"]) is list and 0 < len(table["outputs"]) <= MAX_OUTCOMES, "finite output domain must contain 1..4096 values")
    for field in ("inputs", "outputs"):
        encoded = [canonical(v) for v in table[field]]
        need(len(encoded) == len(set(encoded)), f"finite {field} must contain distinct canonical JSON values")
    rows = table["rows"]
    need(type(rows) is list and len(rows) == len(table["inputs"]), "finite table requires exactly one row per input")
    entries = 0
    for row in rows:
        need(type(row) is list and 0 < len(row) <= 64, "finite rows must contain 1..64 outcomes")
        need(all(type(index) is int and 0 <= index < len(table["outputs"]) for index in row), "invalid finite output index")
        need(len(row) == len(set(row)), "duplicate output index in finite row")
        entries += len(row)
    need(entries <= MAX_RELATION_ENTRIES, "finite relation entry budget exceeded")


def check_relation(
    left: dict, right: dict, *, relation: str = "equivalence", input_map: list[int] | None = None,
    left_observation: list | None = None, right_observation: list | None = None,
    exact: bool = False,
) -> dict:
    """Check a finite one-step relation under explicit observation mappings.

    For every left input i, project each allowed output into a common canonical
    JSON observation domain and compare with the right row at input_map[i].
    Equivalence uses equality; refinement and sound abstraction use LEFT subset
    RIGHT.  ``exact=True`` selects equality for abstraction.  Equivalence and
    refinement require a bijective input map, preserving the admitted domain.
    Abstraction permits many-to-one maps but requires every right input covered.

    Malformed tables/maps raise NotationError.  Unsupported alignment or relation
    requests return unsupported, never checked.  A refutation contains a concrete
    input and unmatched observation.  No conclusion about arbitrary trace or
    liveness properties, or external adapter conformance, is implied.
    """
    validate_table(left)
    validate_table(right)
    need(type(relation) is str and type(exact) is bool, "invalid relation request")
    raw_request = {"relation": relation, "input_map": input_map,
                   "left_observation": left_observation, "right_observation": right_observation,
                   "exact": exact}
    canonical(raw_request)
    report = {
        "schema": "zal.finite-relation/1", "checker": "zal.finite-enumeration/1",
        "status": "unsupported", "relation": relation,
        "endpoints": {side: {"id": t["id"], "version": t["version"], "table_digest": digest(t)}
                      for side, t in (("left", left), ("right", right))},
        "scope": "Supplied finite one-step extensional tables and explicit observations; no trace, liveness, intent, or external-adapter theorem.",
        "evidence": "checked-in-this-run", "authority": "none", "checked_cases": 0,
        "domain": {"left_inputs": len(left["inputs"]), "right_inputs": len(right["inputs"])},
        "replay_request": copy.deepcopy(raw_request),
    }
    if relation not in {"equivalence", "refinement", "abstraction"} or (exact and relation == "refinement"):
        report["reason"] = "unsupported-relation-mode"
        return _finish(report)
    if input_map is None:
        lookup = {canonical(value): index for index, value in enumerate(right["inputs"])}
        try:
            aligned = [lookup[canonical(value)] for value in left["inputs"]]
        except KeyError:
            report["reason"] = "input-domains-require-an-explicit-map"
            return _finish(report)
    else:
        canonical(input_map)
        need(type(input_map) is list and len(input_map) == len(left["inputs"]), "input map must cover every left input")
        need(all(type(i) is int and 0 <= i < len(right["inputs"]) for i in input_map), "invalid input map index")
        aligned = list(input_map)
    if set(aligned) != set(range(len(right["inputs"]))):
        report["reason"] = "input-map-does-not-cover-the-right-domain"
        return _finish(report)
    if relation != "abstraction" and len(set(aligned)) != len(aligned):
        report["reason"] = "equivalence-and-refinement-require-bijective-input-alignment"
        return _finish(report)
    observations = []
    for side, table, supplied in (("left", left, left_observation), ("right", right, right_observation)):
        values = table["outputs"] if supplied is None else supplied
        canonical(values)
        need(type(values) is list and len(values) == len(table["outputs"]), f"{side} observation must cover every output")
        observations.append(copy.deepcopy(values))
    lo, ro = observations
    encoded_left = [canonical(v) for v in lo]
    encoded_right = [canonical(v) for v in ro]
    decoded = {canonical(value): value for value in lo + ro}
    equality = relation == "equivalence" or (relation == "abstraction" and exact)
    request = {"relation": relation, "exact": exact, "input_map": aligned,
               "left_observation": lo, "right_observation": ro}
    canonical(request)
    report.update(replay_request=request, observation_digest=digest(request),
                  comparison="equality" if equality else "left-subset-right",
                  quantifier="for every declared left input and every allowed observed left output",
                  direction="left-to-right", method="exhaustive-enumeration")
    if equality:
        report["quantifier"] = "for every declared left input, observed left and aligned right output sets are equal"
    for index, right_index in enumerate(aligned):
        left_set = {encoded_left[o] for o in left["rows"][index]}
        right_set = {encoded_right[o] for o in right["rows"][right_index]}
        report["checked_cases"] += 1
        passes = left_set == right_set if equality else left_set <= right_set
        if not passes:
            report.update(status="refuted", counterexample={
                "left_input_index": index, "left_input": copy.deepcopy(left["inputs"][index]),
                "right_input_index": right_index, "right_input": copy.deepcopy(right["inputs"][right_index]),
                "left_outputs": [copy.deepcopy(left["outputs"][o]) for o in left["rows"][index]],
                "right_outputs": [copy.deepcopy(right["outputs"][o]) for o in right["rows"][right_index]],
                "left_observed": [copy.deepcopy(decoded[v]) for v in sorted(left_set)],
                "right_observed": [copy.deepcopy(decoded[v]) for v in sorted(right_set)],
                "unmatched_left": [copy.deepcopy(decoded[v]) for v in sorted(left_set - right_set)],
                "unmatched_right": [copy.deepcopy(decoded[v]) for v in sorted(right_set - left_set)],
            })
            return _finish(report)
    report["status"] = "checked"
    return _finish(report)


def _maximum(maximum: int) -> None:
    need(type(maximum) is int and 0 <= maximum <= 15, "transfer maximum must be an integer in 0..15")


def transfer_definition(*, maximum: int = 15) -> dict:
    """Return the stable descriptive profile implemented by transfer_snapshot."""
    _maximum(maximum)
    return {
        "schema": "zal.transfer-profile/1", "id": "zal/guarded-transfer", "version": 1,
        "maximum": maximum, "input_order": ["a", "r", "q"],
        "input_domain": "each input is an integer in 0..maximum",
        "output_order": ["a_next", "r_next", "outcome"],
        "arithmetic": "ordinary mathematical integer arithmetic; no wrapping",
        "guard": "1 <= q <= a and r <= maximum - q",
        "policies": {
            "guarded": {"when_guard_true": ["a - q", "r + q", "Accept"],
                        "when_guard_false": ["a", "r", "Reject"]},
            "reject-all": {"always": ["a", "r", "Reject"]},
        },
        "total_observation": "alpha(a_next, r_next, outcome) = a_next + r_next; outcome and allocation omitted",
        "interpretation": "Fixed built-in profile implemented by transfer_snapshot; descriptive strings are not evaluated.",
    }


def transfer_snapshot(a: int, r: int, q: int, *, maximum: int = 15, policy: str = "guarded") -> dict:
    """Exact shared semantics for transfer scenes and finite-table generation."""
    _maximum(maximum)
    need(all(type(v) is int and 0 <= v <= maximum for v in (a, r, q)), "transfer inputs must be integers in 0..maximum")
    need(type(policy) is str and policy in {"guarded", "reject-all"}, "unknown transfer policy")
    guard = 1 <= q <= a and r <= maximum - q
    accepted = policy == "guarded" and guard
    next_a, next_r = (a - q, r + q) if accepted else (a, r)
    return {"available": a, "reserved": r, "request": q, "maximum": maximum, "policy": policy,
            "guard": guard, "accepted": accepted, "next_available": next_a, "next_reserved": next_r,
            "outcome": "Accept" if accepted else "Reject", "total_before": a + r, "total_after": next_a + next_r}


def transfer_table(policy: str = "guarded", *, maximum: int = 15) -> dict:
    """Build the complete 0..M transfer table; M=15 has 4096 input triples."""
    definition = transfer_definition(maximum=maximum)
    need(type(policy) is str and policy in {"guarded", "reject-all"}, "unknown transfer policy")
    inputs, outputs, rows, output_ids = [], [], [], {}
    for a, r, q in product(range(maximum + 1), repeat=3):
        snapshot = transfer_snapshot(a, r, q, maximum=maximum, policy=policy)
        outcome = [snapshot["next_available"], snapshot["next_reserved"], snapshot["outcome"]]
        identity = canonical(outcome)
        if identity not in output_ids:
            output_ids[identity] = len(outputs)
            outputs.append(outcome)
        inputs.append([a, r, q])
        rows.append([output_ids[identity]])
    table = {"schema": "zal.finite-table/1", "id": f"zal/transfer/{policy}/M{maximum}", "version": 1,
             "inputs": inputs, "outputs": outputs, "rows": rows,
             "origin": {"adapter": "zal.transfer/1", "definition_digest": digest(definition)}}
    validate_table(table)
    return table


def transfer_total_table(*, maximum: int = 15) -> dict:
    """Abstract identity on total quantity, with the request retained as input."""
    _maximum(maximum)
    inputs = [[total, q] for total in range(2 * maximum + 1) for q in range(maximum + 1)]
    table = {"schema": "zal.finite-table/1", "id": f"zal/transfer/total/M{maximum}", "version": 1,
             "inputs": inputs, "outputs": list(range(2 * maximum + 1)), "rows": [[value[0]] for value in inputs],
             "description": "Identity on ordinary integer total; allocation and outcome omitted."}
    validate_table(table)
    return table


def transfer_demo(*, maximum: int = 15) -> dict:
    """Compact replayable reports; tables are individually available via helpers.

    Returning only table identities here avoids combining three full endpoints
    into a single over-budget exchange document.  The report requests retain the
    explicit alignment/observation arrays needed to replay each finite check.
    """
    guarded = transfer_table("guarded", maximum=maximum)
    rejected = transfer_table("reject-all", maximum=maximum)
    total = transfer_total_table(maximum=maximum)
    observe_guarded = [a + r for a, r, _ in guarded["outputs"]]
    observe_rejected = [a + r for a, r, _ in rejected["outputs"]]
    input_map = [(a + r) * (maximum + 1) + q for a, r, q in guarded["inputs"]]
    checks = {
        "full_equivalence": check_relation(guarded, rejected),
        "total_equivalence": check_relation(guarded, rejected,
                                             left_observation=observe_guarded, right_observation=observe_rejected),
        "guarded_total_abstraction": check_relation(guarded, total, relation="abstraction", exact=True,
                                                     input_map=input_map, left_observation=observe_guarded),
        "reject_all_total_abstraction": check_relation(rejected, total, relation="abstraction", exact=True,
                                                        input_map=input_map, left_observation=observe_rejected),
    }
    tables = {label: {"id": t["id"], "version": t["version"], "table_digest": digest(t),
                      "input_count": len(t["inputs"]), "output_count": len(t["outputs"])}
              for label, t in (("guarded", guarded), ("reject-all", rejected), ("total", total))}
    result = {"schema": "zal.transfer-demo/1", "definition": transfer_definition(maximum=maximum),
              "tables": tables, "checks": checks, "authority": "none",
              "scope": "Exact enumeration of the declared bounded model. Total-only equality deliberately omits allocation and acceptance."}
    return _finish(result)


def check_transfer_claim(claim: str) -> dict:
    """Compact fixed-M15 transfer checks for replayable shared discussions.

    The exact tables and observation arrays are derived by the named built-in
    profile on every call.  The receipt retains their identities and the finite
    request digest, while its small replay request names the fixed claim.  A
    negative result may stop at its first witness: admitted_cases remains 4096,
    whereas checked_cases reports the number actually inspected.
    """
    need(type(claim) is str and claim in {
        "total-preserved", "same-total-as-reject-all", "full-equivalence-reject-all",
    }, "unsupported transfer claim")
    guarded = transfer_table()
    totals = [a + r for a, r, _ in guarded["outputs"]]
    if claim == "total-preserved":
        abstract = transfer_total_table()
        alignment = [(a + r) * 16 + q for a, r, q in guarded["inputs"]]
        result = check_relation(guarded, abstract, relation="abstraction", exact=True,
                                input_map=alignment, left_observation=totals)
    else:
        rejected = transfer_table("reject-all")
        options = ({} if claim == "full-equivalence-reject-all" else {
            "left_observation": totals,
            "right_observation": [a + r for a, r, _ in rejected["outputs"]],
        })
        result = check_relation(guarded, rejected, **options)
    result["schema"] = "zal.transfer-check/1"
    result["claim"] = claim
    result["profile_digest"] = digest(transfer_definition())
    result["admitted_cases"] = 4096
    result["replay_request"] = {"kind": "transfer-check", "claim": claim}
    result["scope"] = "Fixed M=15 transfer profile; all input triples in 0..15. Observation maps are fixed by the named claim; no external-system or human-agreement claim."
    return _finish(result)
