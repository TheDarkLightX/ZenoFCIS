"""Source-pinned finite transport research packet, not a release or proof gate.

From integrations/zal, run:
    python3 -B -m unittest test_semantic_transport_registry -q
    python3 -B run_semantic_transport_experiment.py > transport-results.json
The packet makes no claims about wall-clock crossovers or native ZAL behavior.
"""
from __future__ import annotations

from hashlib import sha256
from pathlib import Path
import json

import semantic_transport_registry as s
from test_representation_cosearch import make_task
import representation_transport as previous

HERE = Path(__file__).resolve().parent
MANIFEST = HERE / "test-data" / "representation_cosearch_cases.json"
SOURCES = (
    "semantic_transport_registry.py", "test_semantic_transport_registry.py",
    "run_semantic_transport_experiment.py", "representation_cosearch.py",
    "test-data/representation_cosearch_cases.json",
)


def as_goal(spec):
    task = make_task(spec)
    return s.Goal(s.Domain(task.rows), task.labels)


def measure():
    tasks = json.loads(MANIFEST.read_text(encoding="utf-8"))
    if tasks.get("schema") != "zeno-fcis/representation-cosearch-cases/1":
        raise RuntimeError("research manifest mismatch")
    record = {
        "schema": "zeno-fcis/finite-semantic-transport-experiment/1",
        "authority": "none",
        "source_sha256": {name: sha256((HERE/name).read_bytes()).hexdigest() for name in SOURCES},
        "grammar": {"operations": ["append-add", "append-mul", "project"],
                    "max_depth": 2, "max_weight_magnitude": s.MAX_WEIGHT,
                    "max_finite_rows": s.MAX_ROWS, "max_coordinates": s.MAX_ARITY},
        "development": [], "transfer": [], "negative": [],
        "limitations": {
            "finite": "Exact only on the declared rows and admitted integer arithmetic",
            "coverage": "First breadth-first witness, not a globally cheapest representation",
            "heldout": "Fixed different problems; refitting allowed; no statistical generalization claim",
            "native_zal": "not-integrated", "wall_clock_crossover": "unknown",
            "formal_theorem_prover": "not-run", "deployment_authority": "none"},
    }
    discovered = {}
    for spec in tasks["development"]:
        goal = as_goal(spec)
        result = s.discover(goal, max_steps=2)
        if result.status != "finite-replayed" or not s.check_certificate(goal, result.path, result.certificate):
            raise RuntimeError("replay refused development case: " + spec["id"])
        registry = s.Registry(result.path)
        evidence = s.register_certificate(registry, result.certificate, goal)
        if not s.replay_registered(goal, registry, evidence):
            raise RuntimeError("registered evidence refused")
        discovered[spec["id"]] = result
        record["development"].append({
            "id": spec["id"], "goal_sha256": goal.identity(), "registry_sha256": registry.identity(),
            "row_count": len(goal.source.rows), "initial_dimension": goal.source.arity,
            "steps": [{"op": edge.rule.op, "arguments": edge.rule.args,
                       "dimension": edge.target.arity, "relation": s.edge_kind(edge),
                       "reachable_values": len(set(edge.mapping)), "target_values": len(edge.target.rows)}
                      for edge in result.path],
            "coefficients": result.certificate.separator.weights,
            "bias": result.certificate.separator.bias,
            "checked_rows": result.certificate.checked_rows,
            "explored_states": result.visited, "proposed_edges_checked": result.edges_checked,
            "information_loss_refutations": len(result.rejected_loss),
            "replayed": True, "search_complete": result.complete,
        })
    for spec in tasks["evaluation"]:
        task = as_goal(spec)
        previous_result = discovered[spec["source"]]
        new, conflict = s.reuse_rules(previous_result, task)
        if new is None or conflict is not None:
            raise RuntimeError("transfer refuted: " + spec["id"])
        path, certificate = new
        registry = s.Registry(path)
        evidence = s.register_certificate(registry, certificate, task)
        if not s.replay_registered(task, registry, evidence):
            raise RuntimeError("transfer replay refused")
        if tuple(edge.rule for edge in path) != tuple(edge.rule for edge in previous_result.path):
            raise RuntimeError("rule grammar changed during transfer")
        record["transfer"].append({
            "id": spec["id"], "source": spec["source"], "rows_checked": certificate.checked_rows,
            "same_rules_refitted_coefficients": True, "replayed": True,
            "goal_sha256": certificate.goal_hash, "registry_sha256": registry.identity(),
            "source_rules": [edge.rule.serialized() for edge in previous_result.path],
        })
    for spec in tasks["negative"]:
        task = as_goal(spec)
        result = s.discover(task, max_steps=2)
        record["negative"].append({
            "id": spec["id"], "status": result.status,
            "search_complete": result.complete, "visited": result.visited,
            "checked_edges": result.edges_checked,
        })
    # A finite quotient is exact for some properties but NOT equality over all integers.
    source = s.Domain(((0,), (97,)))
    quotient = s.propose_edge("mod97", source, s.Rule("mod-reduce", (97,)))
    old_goal = s.Goal(source, (True, False))
    mapped, loss = s.transport_labels(old_goal, (quotient,))
    if mapped is not None or loss is None or loss.source_rows != (0, 1):
        raise RuntimeError("modular counterexample missing")
    record["negative"].append({"id": "integer-vs-residue-equality", "status": loss.reason,
                                "witness": [source.rows[i][0] for i in loss.source_rows],
                                "relation": s.edge_kind(quotient)})
    # A change in axioms is NOT silently registered as exact translation.
    axiom_pair = previous.changing_axioms_without_interpretation(
        "Euclidean-geometry", "hyperbolic-geometry")
    if axiom_pair.status != "unproved-no-transport-map":
        raise RuntimeError("axiom-change gate changed")
    record["negative"].append({"id": "geometry-axioms-change", "status": axiom_pair.status})
    return record


if __name__ == "__main__":
    print(json.dumps(measure(), sort_keys=True, indent=2))
