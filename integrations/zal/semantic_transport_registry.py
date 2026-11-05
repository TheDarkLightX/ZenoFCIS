"""Research-only finite semantic transport and elementary-rule co-search.

A representation change is a checked map between *finite* coordinate domains.
Rule proposals do not read labels. A property is transportable through a
many-to-one map only when its labels are constant on every reachable fiber.
Searching composes primitive addition, multiplication and coordinate projection
rules; a bounded linear-separator procedure is fitted on each representation.

No native ZAL hook, unbounded theorem, solver certificate, deployment or
publication authority is granted. A complete finite replay is not an
unrestricted semantic proof or a guarantee of algorithmic improvement.
"""
from __future__ import annotations

from dataclasses import dataclass
from hashlib import sha256
from itertools import product
import json

MAX_ROWS = 64
MAX_ARITY = 4
MAX_STEPS = 3
MAX_VISITS = 4096
MAX_ABS = 256
MAX_WEIGHT = 2


def _hash(obj) -> str:
    return sha256(json.dumps(obj, sort_keys=True, separators=(",", ":"),
                             ensure_ascii=True).encode("ascii")).hexdigest()


def _id(text: str) -> bool:
    return type(text) is str and 1 <= len(text) <= 60 and text.isascii() and all(
        c.isalnum() or c in "-_." for c in text)


@dataclass(frozen=True)
class Domain:
    """Closed finite coordinate domain. Order is part of its identity."""

    rows: tuple[tuple[int, ...], ...]

    def __post_init__(self):
        if type(self.rows) is not tuple or not 1 <= len(self.rows) <= MAX_ROWS:
            raise ValueError("domain-size")
        if type(self.rows[0]) is not tuple:
            raise ValueError("domain-row")
        arity = len(self.rows[0])
        if not 1 <= arity <= MAX_ARITY:
            raise ValueError("domain-arity")
        for row in self.rows:
            if (type(row) is not tuple or len(row) != arity or
                    any(type(x) is not int or abs(x) > MAX_ABS for x in row)):
                raise ValueError("domain-integer-canonicality")
        if len(set(self.rows)) != len(self.rows):
            raise ValueError("domain-duplicate")

    @property
    def arity(self) -> int:
        return len(self.rows[0])

    def identity(self) -> str:
        return _hash({"schema": "finite-coordinate-domain/1", "rows": self.rows})


@dataclass(frozen=True)
class Rule:
    """A small, total, typed representation rule; no executable callbacks."""

    op: str
    args: tuple[int, ...]

    def validate(self, arity: int) -> None:
        if (type(self.op) is not str or type(self.args) is not tuple or
                any(type(a) is not int for a in self.args)):
            raise ValueError("rule-type")
        if self.op in ("append-add", "append-mul"):
            if (len(self.args) != 2 or arity >= MAX_ARITY or
                    any(not 0 <= k < arity for k in self.args) or
                    self.args != tuple(sorted(self.args))):
                raise ValueError("rule-append-shape")
        elif self.op == "project":
            if len(self.args) != 1 or not 0 <= self.args[0] < arity:
                raise ValueError("rule-project-shape")
        elif self.op == "mod-reduce":
            if arity != 1 or len(self.args) != 1 or not 2 <= self.args[0] <= 97:
                raise ValueError("rule-mod-shape")
        else:
            raise ValueError("rule-unknown")

    def serialized(self):
        return [self.op, *self.args]

    def cost(self) -> int:
        return 2 if self.op == "append-mul" else 1


def _propose_row(rule: Rule, row: tuple[int, ...]) -> tuple[int, ...]:
    rule.validate(len(row))
    if rule.op == "append-add":
        return row + (row[rule.args[0]] + row[rule.args[1]],)
    if rule.op == "append-mul":
        return row + (row[rule.args[0]] * row[rule.args[1]],)
    if rule.op == "project":
        return (row[rule.args[0]],)
    return (row[0] % rule.args[0],)


def _check_row(rule: Rule, row: tuple[int, ...]) -> tuple[int, ...]:
    """Independent interpreter for checking a proposed finite row mapping."""
    rule.validate(len(row))
    if rule.op == "project":
        return tuple(value for position, value in enumerate(row) if position == rule.args[0])
    if rule.op == "mod-reduce":
        modulus = rule.args[0]
        return (row[0] - (row[0] // modulus) * modulus,)
    lhs = row[rule.args[0]]
    rhs = row[rule.args[1]]
    if rule.op == "append-add":
        return (*row, lhs + rhs)
    return (*row, lhs * rhs)


@dataclass(frozen=True)
class Edge:
    label: str
    source: Domain
    target: Domain
    rule: Rule
    mapping: tuple[int, ...]

    def identity(self) -> str:
        return _hash({"schema": "finite-transport-edge/1", "label": self.label,
                      "source": self.source.identity(), "target": self.target.identity(),
                      "rule": self.rule.serialized(), "mapping": self.mapping})


def propose_edge(label: str, source: Domain, rule: Rule,
                 extra_target_rows: tuple[tuple[int, ...], ...] = ()) -> Edge:
    if not _id(label) or type(source) is not Domain or type(rule) is not Rule:
        raise ValueError("edge-subject")
    rule.validate(source.arity)
    if type(extra_target_rows) is not tuple:
        raise ValueError("extra-target-type")
    images = tuple(_propose_row(rule, row) for row in source.rows)
    # Canonical image ordering cannot depend on hash randomization.
    target = Domain(tuple(sorted(set(images).union(extra_target_rows))))
    lookup = {row: index for index, row in enumerate(target.rows)}
    edge = Edge(label, source, target, rule, tuple(lookup[row] for row in images))
    if not check_edge(edge):
        raise RuntimeError("proposed edge fails independent check")
    return edge


def check_edge(edge: Edge) -> bool:
    """Checks a closed edge's exact semantics; does not trust edge.mapping."""
    if (type(edge) is not Edge or not _id(edge.label) or
            type(edge.source) is not Domain or type(edge.target) is not Domain or
            type(edge.rule) is not Rule or type(edge.mapping) is not tuple or
            len(edge.mapping) != len(edge.source.rows)):
        return False
    try:
        edge.rule.validate(edge.source.arity)
        if any(type(i) is not int or not 0 <= i < len(edge.target.rows) for i in edge.mapping):
            return False
        return all(_check_row(edge.rule, row) == edge.target.rows[index]
                   for row, index in zip(edge.source.rows, edge.mapping))
    except (ValueError, TypeError, OverflowError):
        return False


def edge_kind(edge: Edge) -> str:
    if not check_edge(edge):
        raise ValueError("unverified-edge")
    image = set(edge.mapping)
    injective = len(image) == len(edge.mapping)
    surjective = len(image) == len(edge.target.rows)
    if injective and surjective:
        return "finite-bijection"
    if injective:
        return "finite-embedding"
    if surjective:
        return "finite-quotient"
    return "finite-partial-quotient"


@dataclass(frozen=True)
class Goal:
    source: Domain
    labels: tuple[bool, ...]

    def __post_init__(self):
        if (type(self.source) is not Domain or type(self.labels) is not tuple or
                len(self.labels) != len(self.source.rows) or
                any(type(label) is not bool for label in self.labels) or
                set(self.labels) != {True, False}):
            raise ValueError("goal-closed-bool-labels")

    def identity(self) -> str:
        return _hash({"schema": "finite-observation-goal/1",
                      "source": self.source.identity(), "labels": self.labels})


@dataclass(frozen=True)
class Loss:
    edge_index: int
    source_rows: tuple[int, int]
    reason: str


def transport_labels(goal: Goal, edges: tuple[Edge, ...]):
    """Carry labels along a path; return actual first conflicting source rows."""
    if type(goal) is not Goal or type(edges) is not tuple:
        raise ValueError("path-structure")
    source = goal.source
    mapping = tuple(range(len(goal.source.rows)))
    for depth, edge in enumerate(edges):
        if not check_edge(edge) or edge.source.identity() != source.identity():
            raise ValueError("path-mismatch-or-forgery")
        mapping = tuple(edge.mapping[i] for i in mapping)
        groups: dict[int, int] = {}
        for original, current in enumerate(mapping):
            other = groups.get(current)
            if other is not None and goal.labels[other] != goal.labels[original]:
                return None, Loss(depth, (other, original), "property-lost-under-quotient")
            groups[current] = original
        source = edge.target
    # Return only reachable destination rows; no claim about unreachable points.
    target_labels = {destination: goal.labels[origin] for origin, destination in enumerate(mapping)}
    return (source, mapping, target_labels), None


@dataclass(frozen=True)
class Separator:
    weights: tuple[int, ...]
    bias: int


def fit_separator(domain: Domain, label_by_index: dict[int, bool],
                  bound: int = MAX_WEIGHT):
    """Finite linear threshold search; no general-real-domain guarantees."""
    if (type(domain) is not Domain or type(label_by_index) is not dict or
            type(bound) is not int or not 1 <= bound <= MAX_WEIGHT or
            set(label_by_index.values()) != {True, False} or
            any(type(k) is not int or not 0 <= k < len(domain.rows) or
                type(v) is not bool for k, v in label_by_index.items())):
        raise ValueError("classifier-domain")
    yes = [domain.rows[index] for index, label in label_by_index.items() if label]
    no = [domain.rows[index] for index, label in label_by_index.items() if not label]
    candidates = sorted(product(range(-bound, bound + 1), repeat=domain.arity),
                        key=lambda weights: (sum(abs(w) for w in weights), weights))
    for weights in candidates:
        if not any(weights):
            continue
        # Compute min positive and max negative without ever relying on a
        # temporary training subset. All states in the finite goal are used.
        minimum_positive = min(sum(a*b for a, b in zip(weights, row)) for row in yes)
        maximum_negative = max(sum(a*b for a, b in zip(weights, row)) for row in no)
        if minimum_positive > maximum_negative:
            return Separator(tuple(weights), -maximum_negative)
    return None


@dataclass(frozen=True)
class Certificate:
    schema: str
    goal_hash: str
    edge_hashes: tuple[str, ...]
    final_domain_hash: str
    separator: Separator
    checked_rows: int
    authority: str = "none"


def check_certificate(goal: Goal, path: tuple[Edge, ...], cert: Certificate) -> bool:
    """Full independent finite replay, including source-state and edge bindings."""
    try:
        if (type(goal) is not Goal or type(path) is not tuple or
                any(type(edge) is not Edge for edge in path) or
                type(cert) is not Certificate or
                cert.schema != "zeno-fcis/semantic-transport-certificate/1" or
                cert.authority != "none" or cert.goal_hash != goal.identity() or
                type(cert.edge_hashes) is not tuple or
                cert.edge_hashes != tuple(edge.identity() for edge in path) or
                type(cert.checked_rows) is not int or cert.checked_rows != len(goal.source.rows) or
                type(cert.separator) is not Separator or
                type(cert.separator.weights) is not tuple or
                any(type(w) is not int or abs(w) > MAX_WEIGHT for w in cert.separator.weights) or
                type(cert.separator.bias) is not int or abs(cert.separator.bias) > 4096):
            return False
        transported, conflict = transport_labels(goal, path)
        if conflict is not None:
            return False
        domain, _, _ = transported
        if (cert.final_domain_hash != domain.identity() or
                len(cert.separator.weights) != domain.arity):
            return False
        # Deliberately do not use the stored index mappings for evaluation:
        # recompute original coordinates through the rules.
        for row, expected in zip(goal.source.rows, goal.labels):
            current = row
            for edge in path:
                current = _check_row(edge.rule, current)
                if current not in edge.target.rows:
                    return False
            score = sum(w*x for w, x in zip(cert.separator.weights, current))
            if (score + cert.separator.bias > 0) != expected:
                return False
        return True
    except (TypeError, ValueError, OverflowError, AttributeError, RecursionError):
        return False


def certify(goal: Goal, path: tuple[Edge, ...], separator: Separator) -> Certificate:
    if type(path) is not tuple or type(separator) is not Separator:
        raise ValueError("certificate-input")
    transported, conflict = transport_labels(goal, path)
    if conflict is not None:
        raise ValueError("property-lost: " + str(conflict.source_rows))
    final, _, _ = transported
    cert = Certificate("zeno-fcis/semantic-transport-certificate/1", goal.identity(),
                       tuple(edge.identity() for edge in path), final.identity(),
                       separator, len(goal.source.rows))
    if not check_certificate(goal, path, cert):
        raise ValueError("finite-certificate-refused")
    return cert


@dataclass(frozen=True)
class Registry:
    """Immutable, bounded collection of source-pinned, independently checked edges."""

    edges: tuple[Edge, ...] = ()

    def __post_init__(self):
        if (type(self.edges) is not tuple or len(self.edges) > 128 or
                any(not check_edge(edge) for edge in self.edges)):
            raise ValueError("registry-unverified-content")
        identities = [edge.identity() for edge in self.edges]
        labels = [edge.label for edge in self.edges]
        if len(set(identities)) != len(identities) or len(set(labels)) != len(labels):
            raise ValueError("registry-duplicate-edge")

    def identity(self) -> str:
        return _hash({"schema": "zeno-fcis/finite-transport-registry/1",
                      "edges": tuple(edge.identity() for edge in self.edges)})

    def with_edge(self, edge: Edge) -> Registry:
        return Registry(self.edges + (edge,))

    def resolve(self, identifiers: tuple[str, ...]) -> tuple[Edge, ...]:
        if (type(identifiers) is not tuple or any(type(x) is not str for x in identifiers)):
            raise ValueError("invalid-edge-identifiers")
        indexed = {edge.identity(): edge for edge in self.edges}
        try:
            path = tuple(indexed[identity] for identity in identifiers)
        except KeyError as error:
            raise ValueError("edge-not-in-registry") from error
        if len(path) != len(set(identifiers)):
            raise ValueError("repeated-edge")
        for previous, following in zip(path, path[1:]):
            if previous.target.identity() != following.source.identity():
                raise ValueError("disconnected-path")
        return path


@dataclass(frozen=True)
class RegisteredEvidence:
    registry_hash: str
    certificate: Certificate


def register_certificate(registry: Registry, certificate: Certificate,
                         goal: Goal) -> RegisteredEvidence:
    if type(registry) is not Registry or type(certificate) is not Certificate:
        raise ValueError("registry-evidence")
    path = registry.resolve(certificate.edge_hashes)
    if not check_certificate(goal, path, certificate):
        raise ValueError("registry-certificate-refused")
    return RegisteredEvidence(registry.identity(), certificate)


def replay_registered(goal: Goal, registry: Registry, evidence: RegisteredEvidence) -> bool:
    if (type(goal) is not Goal or type(registry) is not Registry or
            type(evidence) is not RegisteredEvidence or
            evidence.registry_hash != registry.identity()):
        return False
    try:
        path = registry.resolve(evidence.certificate.edge_hashes)
        return check_certificate(goal, path, evidence.certificate)
    except (ValueError, TypeError, AttributeError):
        return False


@dataclass(frozen=True)
class SearchResult:
    status: str
    path: tuple[Edge, ...]
    certificate: Certificate | None
    visited: int
    edges_checked: int
    rejected_loss: tuple[Loss, ...]
    complete: bool
    grammar: str = "append-add/append-mul/project, depth<=3, integers[-2,2]"


def next_rules(domain: Domain) -> tuple[Rule, ...]:
    """Generate elementary transformations, independent of goal labels."""
    out = [Rule("project", (i,)) for i in range(domain.arity)] if domain.arity > 1 else []
    if domain.arity < MAX_ARITY:
        for i in range(domain.arity):
            for j in range(i, domain.arity):
                out.extend((Rule("append-add", (i, j)), Rule("append-mul", (i, j))))
    return tuple(out)


def discover(goal: Goal, max_steps: int = 2, max_visits: int = MAX_VISITS):
    """Breadth-first co-search over finite representations and linear procedures.

    First solution has minimum edge depth in this *bounded* grammar; no claim
    of minimum runtime, total model size, feature complexity, or global cost.
    """
    if (type(goal) is not Goal or type(max_steps) is not int or
            not 0 <= max_steps <= MAX_STEPS or type(max_visits) is not int or
            not 1 <= max_visits <= MAX_VISITS):
        raise ValueError("search-budget-contract")
    queue = [(goal.source, (), tuple(range(len(goal.source.rows))))]
    head = 0
    seen = {(goal.source.identity(), tuple(range(len(goal.source.rows))))}
    edges_checked = 0
    losses = []
    while head < len(queue):
        if head >= max_visits:
            return SearchResult("inconclusive-budget", (), None, head, edges_checked,
                                tuple(losses), False)
        domain, path, mapping = queue[head]
        head += 1
        labels = {mapping[i]: goal.labels[i] for i in range(len(mapping))}
        procedure = fit_separator(domain, labels)
        if procedure is not None:
            cert = certify(goal, path, procedure)
            return SearchResult("finite-replayed", path, cert, head, edges_checked,
                                tuple(losses), False)
        if len(path) >= max_steps:
            continue
        for ordinal, rule in enumerate(next_rules(domain)):
            try:
                edge = propose_edge(f"step-{len(path)}-{ordinal}", domain, rule)
            except (ValueError, RuntimeError, OverflowError):
                continue
            edges_checked += 1
            if not check_edge(edge):
                raise RuntimeError("proposed rule cannot be replayed")
            new_mapping = tuple(edge.mapping[k] for k in mapping)
            grouped: dict[int, int] = {}
            counterexample = None
            for i, destination in enumerate(new_mapping):
                previous = grouped.get(destination)
                if previous is not None and goal.labels[previous] != goal.labels[i]:
                    counterexample = Loss(len(path), (previous, i), "property-lost-under-quotient")
                    break
                grouped[destination] = i
            if counterexample is not None:
                losses.append(counterexample)
                continue
            # The same target value set can encode distinct source->target
            # mappings. A quotient solely by target domain would be unsound:
            # later feature rules operate on where each *source* row landed.
            state_key = (edge.target.identity(), new_mapping)
            if state_key not in seen:
                seen.add(state_key)
                queue.append((edge.target, path + (edge,), new_mapping))
    return SearchResult("no-solution-within-grammar", (), None, head, edges_checked,
                        tuple(losses), True)


def reuse_rules(source: SearchResult, target: Goal):
    """Reuse only syntactic representation rules; refit the linear procedure."""
    if (type(source) is not SearchResult or source.status != "finite-replayed" or
            source.certificate is None or type(target) is not Goal):
        raise ValueError("unqualified-transfer-source")
    domain = target.source
    path = []
    for depth, prior in enumerate(source.path):
        edge = propose_edge(f"replay-{depth}", domain, prior.rule)
        path.append(edge)
        domain = edge.target
    transported, conflict = transport_labels(target, tuple(path))
    if conflict is not None:
        return None, conflict
    last, _, labels = transported
    procedure = fit_separator(last, labels)
    if procedure is None:
        return None, None
    cert = certify(target, tuple(path), procedure)
    return (tuple(path), cert), None
