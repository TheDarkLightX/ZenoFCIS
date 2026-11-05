"""Finite, research-only representation/procedure co-search for ZAL.

Compositional arithmetic feature grammar: x_i, plus and multiply.
A bounded integer linear-threshold procedure is synthesized using CEGIS.
A second representation is an ordered BDD for complete Boolean inputs.
Quantifier projection can be applied *before* both representation searches.

The only correctness claims are finite, exactly enumerated dataset semantics and
pointwise Boolean projection. The tool grants no proof/deployment authority.
No dependencies beyond the Python standard library.
"""
from __future__ import annotations

from dataclasses import dataclass
from hashlib import sha256
from itertools import product
import json

import discovery_loop as boole

MAX_VARIABLES = 3
MAX_ROWS = 64
MAX_TREE_NODES = 7
MAX_CANDIDATES = 512
WEIGHT_LIMIT = 3


def digest(value) -> str:
    return sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode("ascii")).hexdigest()


@dataclass(frozen=True)
class Task:
    name: str
    rows: tuple[tuple[int, ...], ...]
    labels: tuple[bool, ...]

    def __post_init__(self):
        if (type(self.name) is not str or not self.name or len(self.name) > 128 or not self.name.isascii()
                or type(self.rows) is not tuple or not 2 <= len(self.rows) <= MAX_ROWS
                or type(self.labels) is not tuple or len(self.labels) != len(self.rows)
                or any(type(y) is not bool for y in self.labels)
                or set(self.labels) != {True, False}):
            raise ValueError("task shape or labels")
        arity = len(self.rows[0])
        if not 1 <= arity <= MAX_VARIABLES:
            raise ValueError("task arity")
        if any(type(row) is not tuple or len(row) != arity or any(type(x) is not int or not -4 <= x <= 4 for x in row)
               for row in self.rows) or len(set(self.rows)) != len(self.rows):
            raise ValueError("task finite inputs")

    @property
    def arity(self):
        return len(self.rows[0])

    def identity(self):
        return digest({"name": self.name, "rows": self.rows, "labels": self.labels})

    @classmethod
    def from_boolean_table(cls, name: str, table: boole.Table):
        if table.arity < 1 or table.arity > MAX_VARIABLES:
            raise ValueError("unsupported Boolean table arity")
        rows = tuple(tuple((i >> j) & 1 for j in range(table.arity)) for i in range(1 << table.arity))
        return cls(name, rows, tuple(table.at(i) for i in range(1 << table.arity)))

    def boolean_table(self):
        if len(self.rows) != (1 << self.arity):
            return None
        d = {row: label for row, label in zip(self.rows, self.labels)}
        if set(d) != set(product((0, 1), repeat=self.arity)):
            return None
        return boole.Table.from_predicate(self.arity, lambda r: d[tuple((r >> j) & 1 for j in range(self.arity))])


@dataclass(frozen=True)
class Expr:
    op: str
    args: tuple[Expr, ...] = ()
    index: int = -1

    @property
    def size(self):
        return 1 + sum(arg.size for arg in self.args)

    def key(self):
        return (self.op, self.index, tuple(a.key() for a in self.args))

    def evaluate(self, row):
        if self.op == "var" and not self.args:
            if type(self.index) is not int or not 0 <= self.index < len(row):
                raise ValueError("invalid variable")
            return row[self.index]
        if self.op not in ("+", "*") or self.index != -1 or len(self.args) != 2:
            raise ValueError("invalid expression")
        a, b = (arg.evaluate(row) for arg in self.args)
        return a + b if self.op == "+" else a * b

    def pretty(self):
        if self.op == "var":
            return f"x{self.index}"
        return f"({self.args[0].pretty()}{self.op}{self.args[1].pretty()})"


def valid_expr(root: Expr, arity: int) -> bool:
    """Preflight AST shape and complexity without recursive traversal."""
    if type(arity) is not int or not 1 <= arity <= MAX_VARIABLES:
        return False
    todo = [root]
    seen_nodes = 0
    while todo:
        node = todo.pop()
        seen_nodes += 1
        if (seen_nodes > MAX_TREE_NODES or type(node) is not Expr
                or type(node.op) is not str or type(node.index) is not int
                or type(node.args) is not tuple):
            return False
        if node.op == "var":
            if node.args or not 0 <= node.index < arity:
                return False
        elif node.op in ("+", "*"):
            if node.index != -1 or len(node.args) != 2:
                return False
            todo.extend(node.args)
        else:
            return False
    return True


def _make(op, a, b):
    if a.key() > b.key():
        a, b = b, a
    return Expr(op, (a, b))


def generate_features(task: Task, max_nodes: int = MAX_TREE_NODES, max_candidates: int = MAX_CANDIDATES):
    """Compositional typed grammar with a finite-task semantic quotient.

    Retains the minimum-node syntax for each observed output vector; returns
    completeness=false when the candidate-budget truncates the grammar.
    """
    if type(task) is not Task or type(max_nodes) is not int or not 1 <= max_nodes <= MAX_TREE_NODES:
        raise ValueError("invalid grammar")
    if type(max_candidates) is not int or not 1 <= max_candidates <= MAX_CANDIDATES:
        raise ValueError("invalid candidate budget")
    by_size = {size: [] for size in range(1, max_nodes + 1, 2)}
    observed = {}
    for i in range(task.arity):
        term = Expr("var", (), i)
        vector = tuple(term.evaluate(r) for r in task.rows)
        if vector not in observed:
            observed[vector] = term
            by_size[1].append(term)
    base_vectors = set(observed)
    generated = []
    completed = True
    for size in range(3, max_nodes + 1, 2):
        terms = {}
        for left_size in range(1, size - 1, 2):
            right_size = size - 1 - left_size
            for left in by_size[left_size]:
                for right in by_size[right_size]:
                    for op in ("+", "*"):
                        candidate = _make(op, left, right)
                        terms[candidate.key()] = candidate
        for candidate in sorted(terms.values(), key=lambda t: t.key()):
            vector = tuple(candidate.evaluate(row) for row in task.rows)
            if any(abs(v) > 256 for v in vector) or vector in observed:
                continue
            # A duplicate semantic behavior is kept only in its first/cheapest
            # representation. This is evaluation-domain, NOT global equality.
            if len(generated) >= max_candidates:
                completed = False
                return tuple(generated), completed
            observed[vector] = candidate
            generated.append(candidate)
            by_size[size].append(candidate)
    return tuple(generated), completed


@dataclass(frozen=True)
class Separator:
    weights: tuple[int, ...]
    bias: int


def separator_from_rows(vectors, labels, indices, weight_limit: int):
    """Bounded integer feasibility, choosing the cheapest feasible separator."""
    count = len(vectors[0])
    if not indices or set(labels[i] for i in indices) != {False, True}:
        raise ValueError("require one observed positive and negative")
    search = sorted(product(range(-weight_limit, weight_limit + 1), repeat=count),
                    key=lambda w: (sum(abs(v) for v in w), w))
    positive, negative = ([i for i in indices if labels[i] == wanted] for wanted in (True, False))
    for weights in search:
        if not any(weights):
            continue
        min_pos = min(sum(w * x for w, x in zip(weights, vectors[i])) for i in positive)
        max_neg = max(sum(w * x for w, x in zip(weights, vectors[i])) for i in negative)
        if min_pos > max_neg:
            return Separator(weights, -max_neg)
    return None


@dataclass(frozen=True)
class Trace:
    proposal_count: int
    counterexample_indices: tuple[int, ...]
    separator: Separator | None
    status: str


def cegis(task: Task, features: tuple[Expr, ...], weight_limit: int = WEIGHT_LIMIT):
    """Propose on observed rows, refute on full declared finite task."""
    if (type(task) is not Task or type(features) is not tuple or not features
            or len(features) > 4 or not all(valid_expr(f, task.arity) for f in features)
            or type(weight_limit) is not int or not 1 <= weight_limit <= WEIGHT_LIMIT):
        raise ValueError("CEGIS domain")
    values = tuple(tuple(f.evaluate(row) for f in features) for row in task.rows)
    first_pos = task.labels.index(True)
    first_neg = task.labels.index(False)
    known = [first_pos, first_neg]
    witnesses = []
    for proposal_count in range(1, len(task.rows) + 1):
        candidate = separator_from_rows(values, task.labels, known, weight_limit)
        if candidate is None:
            return Trace(proposal_count - 1, tuple(witnesses), None, "infeasible-under-bounded-weights")
        bad = next((i for i, (row, expected) in enumerate(zip(values, task.labels))
                    if (sum(w * v for w, v in zip(candidate.weights, row)) + candidate.bias > 0) != expected), None)
        if bad is None:
            return Trace(proposal_count, tuple(witnesses), candidate, "finite-equivalent")
        if bad in known:
            raise RuntimeError("CEGIS inconsistent candidate")
        known.append(bad)
        witnesses.append(bad)
    return Trace(len(task.rows), tuple(witnesses), None, "budget-exhausted")


# The checker uses a different representation: multivariate integer polynomial.
# It does not call the AST interpreter or the CEGIS fitting path.
def polynomial(expr: Expr, arity: int):
    if expr.op == "var" and not expr.args and type(expr.index) is int and 0 <= expr.index < arity:
        powers = tuple(int(i == expr.index) for i in range(arity))
        return {powers: 1}
    if expr.op not in ("+", "*") or expr.index != -1 or len(expr.args) != 2:
        raise ValueError("invalid polynomial expression")
    left, right = (polynomial(child, arity) for child in expr.args)
    result = {}
    if expr.op == "+":
        for poly in (left, right):
            for key, value in poly.items():
                result[key] = result.get(key, 0) + value
    else:
        for powers_a, coeff_a in left.items():
            for powers_b, coeff_b in right.items():
                powers = tuple(a + b for a, b in zip(powers_a, powers_b))
                result[powers] = result.get(powers, 0) + coeff_a * coeff_b
    return {powers: v for powers, v in result.items() if v}


def polynomial_at(poly, row):
    return sum(coeff * product_of_powers(row, powers) for powers, coeff in poly.items())


def product_of_powers(row, powers):
    value = 1
    for x, degree in zip(row, powers):
        value *= x ** degree
    return value


@dataclass(frozen=True)
class Receipt:
    task_digest: str
    features: tuple[Expr, ...]
    separator: Separator


def replay(task: Task, receipt: Receipt):
    """Independently re-evaluate each finite input using polynomial expansion."""
    if (type(task) is not Task or type(receipt) is not Receipt
            or receipt.task_digest != task.identity()
            or type(receipt.separator) is not Separator
            or type(receipt.features) is not tuple
            or len(receipt.features) != len(receipt.separator.weights)
            or not 1 <= len(receipt.features) <= 4
            or not all(valid_expr(f, task.arity) for f in receipt.features)
            or type(receipt.separator.bias) is not int
            or any(type(w) is not int or abs(w) > WEIGHT_LIMIT for w in receipt.separator.weights)):
        return False
    try:
        polys = [polynomial(feature, task.arity) for feature in receipt.features]
        return all((receipt.separator.bias + sum(w * polynomial_at(poly, row)
                                                    for w, poly in zip(receipt.separator.weights, polys)) > 0) == label
                   for row, label in zip(task.rows, task.labels))
    except (TypeError, ValueError, OverflowError, RecursionError):
        return False


@dataclass(frozen=True)
class SearchReport:
    task_identity: str
    baseline: Trace
    candidate: Receipt | None
    candidate_trace: Trace | None
    generated: int
    enumerated: int
    complete: bool
    bdd_nodes: int | None
    bdd_orders: int


def search(task: Task, max_nodes: int = MAX_TREE_NODES, max_candidates: int = MAX_CANDIDATES,
           max_evaluations: int = 512):
    """Search compositional feature maps + linear procedures and BDD representation.

    The BDD and linear-feature results are a portfolio; incomparable resource
    units are deliberately NOT combined into one score.
    """
    if type(max_evaluations) is not int or not 0 <= max_evaluations <= 512:
        raise ValueError("search evaluation budget")
    bases = tuple(Expr("var", (), i) for i in range(task.arity))
    baseline = cegis(task, bases)
    generated, generation_complete = generate_features(task, max_nodes, max_candidates)
    best = None
    chosen_trace = None
    enumerated = 0
    if baseline.separator is None:
        for feature in generated:
            if enumerated >= max_evaluations:
                break
            trace = cegis(task, bases + (feature,))
            enumerated += 1
            if trace.separator is None:
                continue
            receipt = Receipt(task.identity(), bases + (feature,), trace.separator)
            if not replay(task, receipt):
                raise RuntimeError("independent polynomial checker refused the result")
            score = (feature.size, sum(abs(w) for w in trace.separator.weights), feature.pretty())
            if best is None or score < best[0]:
                best = score, receipt
                chosen_trace = trace
        # As soon as a complete search of a smaller tree size is finished,
        # further sizes cannot improve primary feature-node count.
    table = task.boolean_table()
    bdd_nodes, bdd_orders = None, 0
    if table is not None:
        bdd, bdd_orders = boole.search_representation(table)
        if not boole.check_bdd(table, bdd):
            raise RuntimeError("independent BDD replay refused the result")
        bdd_nodes = len(bdd.nodes)
    return SearchReport(task.identity(), baseline, best[1] if best is not None else None,
                        chosen_trace, len(generated), enumerated,
                        generation_complete and enumerated == len(generated), bdd_nodes, bdd_orders)


def run_demo():
    grid = tuple(product(range(-2, 3), repeat=2))
    tasks = [
        Task("xor", tuple(product((0, 1), repeat=2)),
             tuple(bool(a ^ b) for a, b in product((0, 1), repeat=2))),
        Task("radial", grid, tuple(x*x + y*y >= 4 for x, y in grid)),
        Task("oblique", grid, tuple((x+y)*(x+y) >= 4 for x, y in grid)),
    ]
    results = {}
    for task in tasks:
        report = search(task)
        results[task.name] = {
            "baseline": report.baseline.status,
            "lifted": report.candidate is not None,
            "feature": report.candidate.features[-1].pretty() if report.candidate else None,
            "counterexamples": len(report.candidate_trace.counterexample_indices) if report.candidate_trace else None,
            "semantic_candidates": report.generated,
            "attempted_candidates": report.enumerated,
            "complete": report.complete,
            "bdd_nodes": report.bdd_nodes,
            "bdd_orders": report.bdd_orders,
        }
    return {"schema": "zeno-fcis/representation-cosearch/1", "authority": "none", "results": results}


if __name__ == "__main__":
    print(json.dumps(run_demo(), indent=2, sort_keys=True))


def reuse_feature(source: SearchReport, target: Task) -> tuple[Trace, Receipt | None]:
    """Freeze a discovered feature; refit only decision weights on a new task.

    This is transfer of a representation, NOT zero-shot predictive transfer.
    Source training data are not accessed, and target feature search is disabled.
    """
    if type(source) is not SearchReport or source.candidate is None or type(target) is not Task:
        raise ValueError("no reusable feature or malformed task")
    if len(source.candidate.features) != target.arity + 1:
        raise ValueError("incompatible target arity")
    grammar_feature = source.candidate.features[-1]
    vars_ = tuple(Expr("var", (), i) for i in range(target.arity))
    feature_set = vars_ + (grammar_feature,)
    trace = cegis(target, feature_set)
    if trace.separator is None:
        return trace, None
    receipt = Receipt(target.identity(), feature_set, trace.separator)
    if not replay(target, receipt):
        raise RuntimeError("independent transfer replay failed")
    return trace, receipt
