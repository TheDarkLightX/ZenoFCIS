"""The exact pruning rules of `footprint.Pruner` as constraints over `realize`'s slot selectors.

`realize` asks the solver whether any program of a length fits the examples. The duplicate, order and dead rules
that the enumerator applies are predicates on the chosen forms alone, so they can be stated over the selectors.
The constraints are generated from the same `Pruner` tables the enumerator judges with (`dominated`, `order`,
the footprints' `reads`, `overwrites` and `writes`, and `observed`), not re-authored. The lost-input rule is not
encoded, so the constrained query admits a superset of the kept candidates.

Why the restricted query still decides the palette: every candidate a rule skips is either never equivalent, or
has a kept equivalent candidate of no greater length and cost (the dead rule deletes an instruction; the order and
duplicate rules keep the length). So when the restricted query is unsat at every length 0..k, no admitted program
of length 0..k exists in the palette. A single length's unsat says only that no *kept* program of that length
exists; `realize` asks the lengths from 0 upward and its record states the aggregate.
"""
from __future__ import annotations

from . import footprint
from .model import canonical, digest

_TABLES = {}


def tables(problem: dict):
    """(pruner, locations) for the problem, or None when some footprint cannot be derived (then nothing is pruned
    and the caller refuses the option rather than silently weakening it)."""
    key = digest(canonical(problem))
    if key not in _TABLES:
        from .session import _grammar
        pruner = footprint.Pruner(problem, _grammar(problem))
        if any(f is None for f in pruner.prints):
            _TABLES[key] = None
        else:
            locations = sorted(frozenset().union(*(f.reads | f.writes for f in pruner.prints)) | pruner.observed)
            _TABLES[key] = (pruner, locations)
        if len(_TABLES) > 64:
            _TABLES.pop(next(iter(_TABLES)))
    return _TABLES[key]


def constrain(z, solver, selectors, slots, pruner, locations) -> int:
    """Add the rules for `selectors`, slot j ranging over the form indices `slots[j]`; returns the constraint count."""
    context = solver.ctx
    k = len(selectors)
    count = 0
    true, false = z.BoolVal(True, ctx=context), z.BoolVal(False, ctx=context)
    # duplicate: a dominated form is never chosen (Pruner.judge: prefix[-1] in dominated)
    for selector, forms in zip(selectors, slots):
        for d in pruner.dominated:
            if d in forms:
                solver.add(selector != d)
                count += 1
    # order: a pair of independent neighbours is never proposed byte-descending (Pruner.judge: order[a][b])
    for i in range(k - 1):
        for a in slots[i]:
            for b in slots[i + 1]:
                if pruner.order[a][b]:
                    solver.add(z.Not(z.And(selectors[i] == a, selectors[i + 1] == b)))
                    count += 1

    def chosen(j, predicate):
        indices = [f for f in slots[j] if predicate(pruner.prints[f])]
        if not indices:
            return false
        if len(indices) == len(slots[j]):
            return true
        return z.Or([selectors[j] == f for f in indices])

    # dead (Pruner.dead with the complete program): a write is live when a later slot reads the location before
    # any later slot overwrites it, or when nothing later overwrites it and the location is observed at the end
    reads = {(j, L): chosen(j, lambda f, L=L: L in f.reads) for j in range(k) for L in locations}
    overwrites = {(j, L): chosen(j, lambda f, L=L: L in f.overwrites) for j in range(k) for L in locations}
    live = {}
    for i in range(k):
        for L in locations:
            terms, clear = [], []
            for j in range(i + 1, k):
                terms.append(z.And([reads[(j, L)]] + clear) if clear else reads[(j, L)])
                clear.append(z.Not(overwrites[(j, L)]))
            if L in pruner.observed:
                terms.append(z.And(clear) if clear else true)
            live[(i, L)] = z.Or(terms) if terms else false
    for i in range(k):
        for f in slots[i]:
            writes = sorted(pruner.prints[f].writes)
            alive = z.Or([live[(i, L)] for L in writes]) if writes else false
            solver.add(z.Implies(selectors[i] == f, alive))
            count += 1
    return count


def kept(pruner, choice) -> bool:
    """Whether the enumerator keeps the complete sequence `choice` (every prefix passes `judge`)."""
    return all(pruner.judge(list(choice[:n]), len(choice)) is None for n in range(1, len(choice) + 1))
