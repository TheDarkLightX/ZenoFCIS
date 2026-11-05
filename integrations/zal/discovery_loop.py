"""Bounded research prototype: discover, check, and reuse finite Boolean laws.

No ZAL runtime, publication, proof-system, or deployment authority is conferred.
The proposer enumerates 16 binary truth functions; the independent checker
uses the semantics of the quantifier, not the proposer's candidate ranking.
The four-pair check is sufficient for pointwise Boolean cofactor projection.
"""
from __future__ import annotations

from dataclasses import dataclass
from hashlib import sha256
from itertools import permutations
import json

MAX_ARITY = 6
MODES = ("exists", "forall")


@dataclass(frozen=True)
class Table:
    arity: int
    bits: int

    def __post_init__(self):
        if (type(self.arity) is not int or not 0 <= self.arity <= MAX_ARITY
                or type(self.bits) is not int or not 0 <= self.bits < 1 << (1 << self.arity)):
            raise ValueError("table domain or capacity")

    def at(self, row: int) -> bool:
        if type(row) is not int or not 0 <= row < (1 << self.arity):
            raise ValueError("row outside table")
        return bool((self.bits >> row) & 1)

    @classmethod
    def from_predicate(cls, arity: int, predicate):
        if type(arity) is not int or not 0 <= arity <= MAX_ARITY:
            raise ValueError("arity outside prototype")
        return cls(arity, sum(int(bool(predicate(row))) << row for row in range(1 << arity)))

    def digest(self) -> str:
        wire = json.dumps([self.arity, self.bits], separators=(",", ":"))
        return sha256(wire.encode("ascii")).hexdigest()


def _binary(mask: int, a: bool, b: bool) -> bool:
    return bool((mask >> (int(a) + 2 * int(b))) & 1)


def _quantifier(mode: str, a: bool, b: bool) -> bool:
    if mode == "exists":
        return a or b
    if mode == "forall":
        return a and b
    raise ValueError("unknown quantifier")


@dataclass(frozen=True)
class Attempt:
    mask: int
    counterexample: tuple[bool, bool] | None


@dataclass(frozen=True)
class Procedure:
    mode: str
    mask: int
    attempts: tuple[Attempt, ...]


def check_procedure(procedure: Procedure) -> bool:
    """Independent exhaustive 4-pair checker for a *pointwise* projection law."""
    if (type(procedure) is not Procedure or procedure.mode not in MODES
            or type(procedure.mask) is not int or not 0 <= procedure.mask < 16):
        return False
    return all(_binary(procedure.mask, a, b) == _quantifier(procedure.mode, a, b)
               for a in (False, True) for b in (False, True))


def discover(mode: str) -> Procedure:
    """CEGIS: enumerate binary operators; retain each actual counterexample."""
    if mode not in MODES:
        raise ValueError("unknown quantifier")
    observations = [(False, True)]  # deliberately underdetermined seed
    history = []
    for _ in range(16):
        compatible = (mask for mask in range(16)
                      if all(_binary(mask, a, b) == _quantifier(mode, a, b)
                             for a, b in observations))
        candidate = next(compatible, None)
        if candidate is None:
            raise RuntimeError("bounded grammar has no candidate")
        witness = next(((a, b) for a in (False, True) for b in (False, True)
                        if _binary(candidate, a, b) != _quantifier(mode, a, b)), None)
        history.append(Attempt(candidate, witness))
        if witness is None:
            result = Procedure(mode, candidate, tuple(history))
            if not check_procedure(result):
                raise RuntimeError("checker refused candidate")
            return result
        observations.append(witness)
    raise RuntimeError("discovery budget exhausted")


def _with_bit(row: int, variable: int, value: int) -> int:
    lower = row & ((1 << variable) - 1)
    upper = row >> variable
    return lower | (value << variable) | (upper << (variable + 1))


@dataclass(frozen=True)
class ProjectionReceipt:
    input_hash: str
    mode: str
    variable: int
    mask: int
    output: Table


def project(table: Table, variable: int, procedure: Procedure) -> ProjectionReceipt:
    if (type(table) is not Table or not 1 <= table.arity <= MAX_ARITY
            or type(variable) is not int or not 0 <= variable < table.arity
            or not check_procedure(procedure)):
        raise ValueError("projection refuses unverified procedure or invalid domain")
    bits = 0
    for row in range(1 << (table.arity - 1)):
        a = table.at(_with_bit(row, variable, 0))
        b = table.at(_with_bit(row, variable, 1))
        bits |= int(_binary(procedure.mask, a, b)) << row
    return ProjectionReceipt(table.digest(), procedure.mode, variable,
                             procedure.mask, Table(table.arity - 1, bits))


def replay_projection(table: Table, receipt: ProjectionReceipt) -> bool:
    """Recompute the quantified relation directly; never trust saved output."""
    if (type(table) is not Table or type(receipt) is not ProjectionReceipt
            or table.digest() != receipt.input_hash
            or receipt.mode not in MODES
            or type(receipt.variable) is not int
            or not 0 <= receipt.variable < table.arity
            or type(receipt.output) is not Table
            or receipt.output.arity != table.arity - 1
            or not check_procedure(Procedure(receipt.mode, receipt.mask, ()))):
        return False
    for row in range(1 << receipt.output.arity):
        a = table.at(_with_bit(row, receipt.variable, 0))
        b = table.at(_with_bit(row, receipt.variable, 1))
        if receipt.output.at(row) != _quantifier(receipt.mode, a, b):
            return False
    return True


@dataclass(frozen=True)
class BDD:
    arity: int
    order: tuple[int, ...]
    nodes: tuple[tuple[int, int, int], ...]
    root: int

    def at(self, row: int) -> bool:
        if type(row) is not int or not 0 <= row < (1 << self.arity):
            raise ValueError("row outside BDD")
        node = self.root
        while node > 1:
            variable, low, high = self.nodes[node - 2]
            node = high if (row >> variable) & 1 else low
        return bool(node)


def make_bdd(table: Table, order: tuple[int, ...]) -> BDD:
    if (type(table) is not Table or type(order) is not tuple
            or len(order) != table.arity or any(type(v) is not int for v in order)
            or sorted(order) != list(range(table.arity))):
        raise ValueError("invalid BDD variable order")
    nodes = []
    intern = {}

    def build(rows: tuple[int, ...], depth: int) -> int:
        value = table.at(rows[0])
        if all(table.at(r) == value for r in rows):
            return int(value)
        variable = order[depth]
        low = build(tuple(r for r in rows if not (r >> variable) & 1), depth + 1)
        high = build(tuple(r for r in rows if (r >> variable) & 1), depth + 1)
        if low == high:
            return low
        key = variable, low, high
        if key not in intern:
            intern[key] = len(nodes) + 2
            nodes.append(key)
        return intern[key]

    root = build(tuple(range(1 << table.arity)), 0)
    return BDD(table.arity, order, tuple(nodes), root)


def check_bdd(table: Table, bdd: BDD) -> bool:
    if (type(table) is not Table or type(bdd) is not BDD or table.arity != bdd.arity
            or type(bdd.order) is not tuple or len(bdd.order) != table.arity
            or any(type(v) is not int for v in bdd.order)
            or sorted(bdd.order) != list(range(table.arity))
            or type(bdd.nodes) is not tuple or type(bdd.root) is not int
            or not 0 <= bdd.root < len(bdd.nodes) + 2):
        return False
    # Each edge must point to an earlier node or a terminal; this forbids cycles.
    for index, node in enumerate(bdd.nodes):
        if (type(node) is not tuple or len(node) != 3
                or any(type(v) is not int for v in node)
                or node[0] not in bdd.order
                or not 0 <= node[1] < index + 2
                or not 0 <= node[2] < index + 2):
            return False
    return all(bdd.at(row) == table.at(row) for row in range(1 << table.arity))


def search_representation(table: Table) -> tuple[BDD, int]:
    """Enumerate all <=6! variable orders, minimizing reachable ROBDD nodes."""
    if type(table) is not Table:
        raise ValueError("table required")
    best = None
    attempts = 0
    for order in permutations(range(table.arity)):
        candidate = make_bdd(table, order)
        attempts += 1
        if best is None or (len(candidate.nodes), candidate.order) < (len(best.nodes), best.order):
            best = candidate
    if best is None or not check_bdd(table, best):
        raise RuntimeError("representation failed exact replay")
    return best, attempts


def demo() -> dict:
    # Four data bits (x0...x3) selected by two high bits (x4,x5).
    mux = Table.from_predicate(6, lambda row: (row >> ((row >> 4) & 3)) & 1)
    default = make_bdd(mux, tuple(range(mux.arity)))
    best, order_attempts = search_representation(mux)
    procedures = {mode: discover(mode) for mode in MODES}
    reused = 0
    for table in (mux, Table.from_predicate(4, lambda x: x.bit_count() & 1),
                  Table.from_predicate(3, lambda x: (not (x & 1)) or bool(x & 4))):
        for mode in MODES:
            for variable in range(table.arity):
                receipt = project(table, variable, procedures[mode])
                if not replay_projection(table, receipt):
                    raise RuntimeError("projection replay failure")
                reused += 1
    return {
        "status": "bounded-prototype-replayed",
        "authority": "none",
        "procedure_masks": {mode: p.mask for mode, p in procedures.items()},
        "counterexamples": {mode: sum(a.counterexample is not None for a in p.attempts)
                            for mode, p in procedures.items()},
        "representation": {"baseline_order": list(default.order), "baseline_nodes": len(default.nodes),
                           "chosen_order": list(best.order), "chosen_nodes": len(best.nodes),
                           "searched_orders": order_attempts, "checked_rows": 1 << mux.arity},
        "projected_cases_replayed": reused,
    }


if __name__ == "__main__":
    print(json.dumps(demo(), sort_keys=True, indent=2))
