"""Typed search for composed procedures spanning time and spectral domains.

The compiler searches an operation grammar, not a preset convolution pipeline:
  inputs : Time
  ntt : Time -> Spectrum
  intt : Spectrum -> Time
  pointwise : Spectrum x Spectrum -> Spectrum
  direct_convolve : Time x Time -> Time

Soundness is checked on finite input fixtures by an independent direct oracle.
Only a finite, bounded candidate grammar and test set are searched.
"""
from __future__ import annotations

from dataclasses import dataclass
from itertools import product
import random

import representation_transport as trans


@dataclass(frozen=True)
class Plan:
    op: str
    args: tuple["Plan", ...] = ()

    @property
    def size(self):
        return 1 + sum(x.size for x in self.args)

    def key(self):
        return (self.op, tuple(x.key() for x in self.args))

    def describe(self):
        return self.op if not self.args else self.op + "(" + ",".join(x.describe() for x in self.args) + ")"

    @property
    def domain(self):
        if self.op in ("left", "right") and len(self.args) == 0:
            return "time"
        if self.op == "ntt" and len(self.args) == 1 and self.args[0].domain == "time":
            return "frequency"
        if self.op == "intt" and len(self.args) == 1 and self.args[0].domain == "frequency":
            return "time"
        if (self.op == "pointwise" and len(self.args) == 2
                and self.args[0].domain == self.args[1].domain == "frequency"):
            return "frequency"
        if (self.op == "direct_convolve" and len(self.args) == 2
                and self.args[0].domain == self.args[1].domain == "time"):
            return "time"
        raise ValueError("ill-typed procedure")


def cost(plan: Plan, length: int):
    if length not in trans.LENGTHS:
        raise ValueError("unsupported length")
    _ = plan.domain
    if plan.op in ("left", "right"):
        return 0
    base = sum(cost(x, length) for x in plan.args)
    ntt_data = (length // 2) * (length.bit_length() - 1)
    return base + {"ntt": ntt_data, "intt": ntt_data + length,
                   "pointwise": length, "direct_convolve": length * length}[plan.op]


def enumerate_plans(max_size: int = 6):
    if type(max_size) is not int or not 3 <= max_size <= 6:
        raise ValueError("plan size bound")
    by_size = {size: {"time": {}, "frequency": {}} for size in range(1, max_size + 1)}
    for op in ("left", "right"):
        leaf = Plan(op)
        by_size[1]["time"][leaf.key()] = leaf
    for size in range(2, max_size + 1):
        for source_domain, target_domain, operation in (("time", "frequency", "ntt"),
                                                        ("frequency", "time", "intt")):
            for inner in by_size[size - 1][source_domain].values():
                term = Plan(operation, (inner,))
                by_size[size][target_domain][term.key()] = term
        for domain, target, operation in (("time", "time", "direct_convolve"),
                                           ("frequency", "frequency", "pointwise")):
            for size_a in range(1, size - 1):
                size_b = size - 1 - size_a
                for left in by_size[size_a][domain].values():
                    for right in by_size[size_b][domain].values():
                        a, b = (right, left) if right.key() < left.key() else (left, right)
                        term = Plan(operation, (a, b))
                        by_size[size][target][term.key()] = term
    choices = [plan for d in by_size.values() for plan in d["time"].values()
               if plan.op not in ("left", "right")]
    return tuple(sorted(choices, key=lambda p: p.key()))


def evaluate(plan: Plan, left: tuple[int, ...], right: tuple[int, ...]) -> tuple[int, ...]:
    length = trans._validate_vector(left)
    if trans._validate_vector(right) != length or plan.size > 6 or plan.domain != "time":
        raise ValueError("invalid candidate or input")

    def eval_node(term):
        if term.op == "left":
            return left
        if term.op == "right":
            return right
        if term.op == "ntt":
            return trans.fast_transform(eval_node(term.args[0]))[0]
        if term.op == "intt":
            return trans.fast_transform(eval_node(term.args[0]), inverse=True)[0]
        if term.op == "pointwise":
            a, b = (eval_node(child) for child in term.args)
            return tuple(x*y % trans.PRIME for x, y in zip(a, b))
        if term.op == "direct_convolve":
            a, b = (eval_node(child) for child in term.args)
            return trans.direct_cyclic_convolution(a, b)
        raise ValueError("unknown node")
    return eval_node(plan)


@dataclass(frozen=True)
class SynthesisResult:
    status: str
    length: int
    plan: Plan | None
    cost_data_multiplies: int | None
    checked_cases: int
    candidates_enumerated: int
    attempted_candidates: int
    witnesses: tuple[tuple[int, int], ...]
    evaluation_status: str


def fixtures(length: int, seed: int) -> tuple[tuple[tuple[int, ...], tuple[int, ...]], ...]:
    if length not in trans.LENGTHS:
        raise ValueError("unsupported length")
    rng = random.Random(seed)
    zero = (0,) * length
    equal = tuple((i + 1) % trans.PRIME for i in range(length))
    vector = [(zero, zero), (equal, equal)]
    vector.extend((tuple(rng.randrange(trans.PRIME) for _ in range(length)),
                   tuple(rng.randrange(trans.PRIME) for _ in range(length))) for _ in range(18))
    return tuple(vector)


def synthesize(length: int, *, bound: int = 6, budget: int = 4096,
               development_seed: int = 1105, evaluation_seed: int = 26011) -> SynthesisResult:
    if (length not in trans.LENGTHS or type(budget) is not int or budget < 1
            or type(development_seed) is not int or type(evaluation_seed) is not int
            or development_seed == evaluation_seed):
        raise ValueError("invalid synthesis contract")
    dev = fixtures(length, development_seed)
    heldout = fixtures(length, evaluation_seed)
    candidates = sorted(enumerate_plans(bound), key=lambda p: (cost(p, length), p.size, p.key()))
    known = [0]
    witnesses = []
    attempted = 0
    for candidate in candidates:
        if attempted == budget:
            break
        attempted += 1
        # The reference is a separate direct implementation, not the
        # transformed algorithm being evaluated.
        if any(evaluate(candidate, *dev[i]) != trans.direct_cyclic_convolution(*dev[i]) for i in known):
            continue
        mismatch = next((i for i, row in enumerate(dev)
                         if evaluate(candidate, *row) != trans.direct_cyclic_convolution(*row)), None)
        if mismatch is not None:
            known.append(mismatch)
            witnesses.append((attempted, mismatch))
            continue
        if any(evaluate(candidate, *row) != trans.direct_cyclic_convolution(*row) for row in heldout):
            return SynthesisResult("evaluation-refuted", length, None, None, len(dev), len(candidates),
                                   attempted, tuple(witnesses), "failed-hidden-evaluation")
        return SynthesisResult("finite-replayed", length, candidate, cost(candidate, length),
                               len(dev) + len(heldout), len(candidates), attempted,
                               tuple(witnesses), "passed-fixed-evaluation")
    return SynthesisResult("budget-inconclusive", length, None, None, len(known),
                           len(candidates), attempted, tuple(witnesses), "not-evaluated")


def demo():
    return {str(n): {
        "status": (report := synthesize(n)).status,
        "plan": report.plan.describe() if report.plan is not None else None,
        "cost": report.cost_data_multiplies,
        "direct_cost": n*n,
        "evaluated_cases": report.checked_cases,
        "counterexamples": len(report.witnesses),
        "attempted": report.attempted_candidates,
        "candidate_count": report.candidates_enumerated,
    } for n in trans.LENGTHS}


if __name__ == "__main__":
    import json
    print(json.dumps(demo(), sort_keys=True, indent=2))
