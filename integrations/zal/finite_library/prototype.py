"""ZAL Semantic Library: finite, non-authoritative algebra experiment.

Original research code; no Tau code, executable, SDK, or runtime dependency.
This is NOT the ZAL interpreter, a proof kernel, or an adoption/publication API.
Run `python prototype.py demo` or `python -m unittest -v` in this directory.
"""
from __future__ import annotations

import argparse
import json
import re
from collections import deque
from dataclasses import dataclass
from hashlib import sha256
from itertools import product
from typing import Any

PROFILE = "zal/semantic-library-finite-enum/1"
NAME = re.compile(r"[a-z][a-z0-9_]{0,47}\Z")
MAX_ROWS, MAX_CHOICES, MAX_BYTES = 4096, 65536, 1 << 20
ScalarTuple = tuple[str, ...]
Row = tuple[ScalarTuple, tuple[ScalarTuple, ...]]


class Refused(ValueError):
    """Malformed or unsupported input, distinct from semantic refutation."""


class Incomplete(RuntimeError):
    """Insufficient work budget; never a semantic verdict."""


def need(ok: bool, message: str) -> None:
    if not ok:
        raise Refused(message)


def name(value: Any) -> None:
    need(type(value) is str and NAME.fullmatch(value) is not None, "invalid identifier")


def canonical(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()


def digest(value: Any) -> str:
    return sha256(canonical(value)).hexdigest()


@dataclass
class Budget:
    limit: int = 100000
    used: int = 0

    def __post_init__(self) -> None:
        need(type(self.limit) is int and self.limit >= 0 and self.used == 0, "invalid budget")

    def spend(self, amount: int = 1) -> None:
        need(type(amount) is int and amount >= 0, "invalid work amount")
        if self.used + amount > self.limit:
            raise Incomplete("logical work limit reached")
        self.used += amount


@dataclass(frozen=True)
class Port:
    name: str
    role: str
    unit: str
    values: tuple[str, ...]

    def __post_init__(self) -> None:
        for value in (self.name, self.role, self.unit):
            name(value)
        need(type(self.values) is tuple and 1 <= len(self.values) <= 16, "invalid enum size")
        for value in self.values:
            name(value)
        need(self.values == tuple(sorted(set(self.values))), "enum must be canonical and distinct")

    def data(self) -> dict:
        return {"name": self.name, "role": self.role, "unit": self.unit, "values": list(self.values)}


def domain(ports: tuple[Port, ...], cap: int = MAX_ROWS) -> tuple[ScalarTuple, ...]:
    size = 1
    for port in ports:
        size *= len(port.values)
        need(size <= cap, "Cartesian domain exceeds declared profile")
    return tuple(product(*(port.values for port in ports)))


@dataclass(frozen=True)
class Contract:
    inputs: tuple[Port, ...]
    outputs: tuple[Port, ...]
    # An absent input row is an environmental restriction, NOT default rejection.
    # An admitted row with no allowed output is a contradiction at that input.
    rows: tuple[Row, ...]
    # (input-state-port, output-next-state-port, initial-value)
    state: tuple[tuple[str, str, str], ...] = ()
    owns: tuple[str, ...] = ()

    def __post_init__(self) -> None:
        for ports in (self.inputs, self.outputs):
            need(type(ports) is tuple and 1 <= len(ports) <= 8, "invalid port list")
            need(all(type(p) is Port for p in ports), "ports must be immutable Port values")
            need(len({p.name for p in ports}) == len(ports), "duplicate port")
        allowed_in, allowed_out = set(domain(self.inputs)), set(domain(self.outputs))
        need(type(self.rows) is tuple and len(self.rows) <= MAX_ROWS, "invalid table")
        seen, choices = set(), 0
        for row in self.rows:
            need(type(row) is tuple and len(row) == 2, "invalid row")
            x, ys = row
            need(type(x) is tuple and all(type(v) is str for v in x) and x in allowed_in, "input outside domain")
            need(x not in seen, "duplicate input row")
            seen.add(x)
            need(type(ys) is tuple, "mutable output choices")
            for y in ys:
                need(type(y) is tuple and all(type(v) is str for v in y) and y in allowed_out, "output outside domain")
            need(ys == tuple(sorted(set(ys))), "output choices must be canonical and distinct")
            choices += len(ys)
            need(choices <= MAX_CHOICES, "too many output choices")
        need(self.rows == tuple(sorted(self.rows)), "rows must be canonical")
        need(type(self.owns) is tuple and len(self.owns) <= 16 and self.owns == tuple(sorted(set(self.owns))), "invalid ownership declarations")
        for resource in self.owns:
            name(resource)
        need(type(self.state) is tuple and len(self.state) <= 8, "invalid state links")
        ins, outs, state_in, state_out = {p.name: p for p in self.inputs}, {p.name: p for p in self.outputs}, set(), set()
        for link in self.state:
            need(type(link) is tuple and len(link) == 3, "invalid state link")
            a, b, initial = link
            need(a in ins and b in outs and a not in state_in and b not in state_out, "invalid state mapping")
            need((ins[a].role, ins[a].unit, ins[a].values) == (outs[b].role, outs[b].unit, outs[b].values), "state type mismatch")
            need(initial in ins[a].values, "invalid initial state")
            state_in.add(a)
            state_out.add(b)
        need(self.state == tuple(sorted(self.state)), "state links must be canonical")

    def data(self) -> dict:
        return {"profile": PROFILE, "inputs": [p.data() for p in self.inputs],
                "outputs": [p.data() for p in self.outputs], "state": [list(s) for s in self.state],
                "owns": list(self.owns), "rows": [{"input": list(x), "outputs": [list(y) for y in ys]} for x, ys in self.rows]}

    @property
    def revision(self) -> str:
        return digest(self.data())

    @property
    def signature(self) -> tuple:
        return self.inputs, self.outputs, self.state, self.owns

    def table(self) -> dict[ScalarTuple, tuple[ScalarTuple, ...]]:
        return dict(self.rows)


def no_duplicates(pairs: list) -> dict:
    result = {}
    for key, value in pairs:
        need(key not in result, "duplicate JSON key")
        result[key] = value
    return result


def from_data(data: dict) -> Contract:
    need(type(data) is dict and set(data) == {"profile", "inputs", "outputs", "state", "owns", "rows"}, "unknown or missing contract fields")
    need(data["profile"] == PROFILE, "unsupported profile")
    def ports(values: Any) -> tuple[Port, ...]:
        need(type(values) is list and 1 <= len(values) <= 8, "invalid port list")
        result = []
        for p in values:
            need(type(p) is dict and set(p) == {"name", "role", "unit", "values"}, "invalid port fields")
            need(type(p["values"]) is list, "invalid port values")
            result.append(Port(p["name"], p["role"], p["unit"], tuple(p["values"])))
        return tuple(result)
    ins, outs = ports(data["inputs"]), ports(data["outputs"])
    need(type(data["rows"]) is list and len(data["rows"]) <= MAX_ROWS, "invalid rows")
    rows = []
    total = 0
    for row in data["rows"]:
        need(type(row) is dict and set(row) == {"input", "outputs"}, "invalid row fields")
        need(type(row["input"]) is list and type(row["outputs"]) is list, "invalid row arrays")
        total += len(row["outputs"])
        need(total <= MAX_CHOICES, "too many choices")
        need(all(type(y) is list for y in row["outputs"]), "invalid output tuple")
        rows.append((tuple(row["input"]), tuple(tuple(y) for y in row["outputs"])))
    need(type(data["state"]) is list and len(data["state"]) <= 8 and all(type(s) is list for s in data["state"]), "invalid state array")
    need(type(data["owns"]) is list and len(data["owns"]) <= 16, "invalid ownership array")
    return Contract(ins, outs, tuple(rows), tuple(tuple(s) for s in data["state"]), tuple(data["owns"]))


def loads(source: str) -> Contract:
    need(type(source) is str and source.isascii() and len(source) <= MAX_BYTES, "source exceeds profile")
    try:
        return from_data(json.loads(source, object_pairs_hook=no_duplicates))
    except (json.JSONDecodeError, TypeError, KeyError, RecursionError) as exc:
        raise Refused("malformed contract JSON") from exc


def result(status: str, **fields: Any) -> dict:
    return {"status": status, "authority": "none", "human_agreement": "pending",
            "native_fcis_qualification": "not-performed", **fields}


def fit(candidate: Contract, request: Contract, budget: Budget | None = None) -> dict:
    """Same-interface one-step refinement; no stronger environment assumptions.

    Complete on the request's finite admitted domain only. This does not decide
    arbitrary reactive realizability, specification adequacy, or deployment.
    """
    budget = budget if budget is not None else Budget()
    if candidate.signature != request.signature:
        return result("incompatible-interface", candidate=candidate.revision, request=request.revision)
    if not request.rows:
        return result("vacuous-request")
    table = candidate.table()
    try:
        for x, permitted in request.rows:
            budget.spend()
            if not permitted:
                return result("inconsistent-request", witness=x)
            if x not in table:
                return result("stronger-assumptions", witness=x)
            if not table[x]:
                return result("nonimplementable-candidate", witness=x)
            for y in table[x]:
                budget.spend()
                if y not in permitted:
                    return result("counterexample", witness={"input": x, "candidate_output": y, "permitted": permitted})
        return result("covers-within-finite-profile", cases=len(request.rows), work=budget.used,
                      candidate=candidate.revision, request=request.revision)
    except Incomplete:
        return result("incomplete", work=budget.used)


def meet(left: Contract, right: Contract, budget: Budget | None = None) -> Contract:
    """Conjoin requirements for ONE component; do not compose two implementations.

    Different admitted domains refuse: no silent environment restriction.
    """
    need(left.signature == right.signature, "conjunction requires the same interface and initial state")
    need(tuple(x for x, _ in left.rows) == tuple(x for x, _ in right.rows), "explicit domain reconciliation required")
    budget = budget if budget is not None else Budget()
    rows = []
    for (x, a), (_, b) in zip(left.rows, right.rows):
        budget.spend(1 + len(a) + len(b))
        rows.append((x, tuple(sorted(set(a) & set(b)))))
    return Contract(left.inputs, left.outputs, tuple(rows), left.state, left.owns)


def independent_product(left: Contract, right: Contract, budget: Budget | None = None) -> Contract:
    """Synchronous pair of independent steps, NOT atomic shared-state wiring.

    Resource names are declarations, not authenticated evidence of real-world
    non-aliasing. General wiring and shared-resource protocols are unsupported.
    """
    need(not (set(left.owns) & set(right.owns)), "overlapping exclusive resource declarations")
    budget = budget if budget is not None else Budget()
    def rename(ports: tuple[Port, ...], prefix: str) -> tuple[Port, ...]:
        return tuple(Port(prefix + p.name, p.role, p.unit, p.values) for p in ports)
    ins, outs = rename(left.inputs, "a_" ) + rename(right.inputs, "b_"), rename(left.outputs, "a_") + rename(right.outputs, "b_")
    domain(ins)
    domain(outs)
    need(len(left.rows) * len(right.rows) <= MAX_ROWS, "product row cap")
    rows, total = [], 0
    for (x, a), (z, b) in product(left.rows, right.rows):
        total += len(a) * len(b)
        need(total <= MAX_CHOICES, "product choice cap")
        budget.spend(1 + len(a) * len(b))
        rows.append((x + z, tuple(sorted(y + w for y, w in product(a, b)))))
    state = tuple(sorted((prefix + a, prefix + b, initial)
                         for prefix, c in (("a_", left), ("b_", right)) for a, b, initial in c.state))
    return Contract(ins, outs, tuple(sorted(rows)), state, tuple(sorted(left.owns + right.owns)))


def build(inputs: tuple[Port, ...], outputs: tuple[Port, ...], table: dict,
          state: tuple = (), owns: tuple = ()) -> Contract:
    return Contract(inputs, outputs, tuple(sorted((x, tuple(sorted(set(ys)))) for x, ys in table.items())),
                    tuple(sorted(state)), tuple(sorted(owns)))


def slot(*, reusable: bool, guarded: bool = True, resource: str = "slot") -> Contract:
    """One capacity unit and one identity. Not a general reservation implementation."""
    need(type(reusable) is bool and type(guarded) is bool, "parameters must be exact booleans")
    name(resource)
    states = ("held", "released", "unseen")
    ins = (Port("state", "lifecycle", "control", states),
           Port("event", "operation", "control", ("release", "reserve")),
           Port("authorized", "authorization_observation", "boolean", ("no", "yes")))
    outs = (Port("outcome", "business_decision", "control", ("accept", "reject")),
            Port("next_state", "lifecycle", "control", states),
            Port("reason", "decision_reason", "control", ("duplicate", "not_held", "ok", "unauthorized")))
    table = {}
    for s, event, auth in domain(ins):
        if guarded and auth == "no":
            y = ("reject", s, "unauthorized")
        elif event == "release":
            y = ("accept", "released", "ok") if s == "held" else ("reject", s, "not_held")
        elif s == "unseen" or (reusable and s == "released"):
            y = ("accept", "held", "ok")
        else:
            y = ("reject", s, "duplicate")
        table[s, event, auth] = (y,)
    return build(ins, outs, table, (("state", "next_state", "unseen"),), (resource,))


def requirement(template: Contract, kind: str) -> Contract:
    need(kind in {"authorized_only", "positive_admission", "once_after_release", "reuse_after_release"}, "unknown requirement constructor")
    universe = domain(template.outputs)
    table = {}
    for x, _ in template.rows:
        s, _event, auth = x
        permitted = universe
        if kind == "authorized_only" and auth == "no":
            permitted = (("reject", s, "unauthorized"),)
        elif kind == "positive_admission" and x == ("unseen", "reserve", "yes"):
            permitted = (("accept", "held", "ok"),)
        elif kind.endswith("after_release") and x == ("released", "reserve", "yes"):
            permitted = (("reject", "released", "duplicate"),) if kind == "once_after_release" else (("accept", "held", "ok"),)
        table[x] = permitted
    return build(template.inputs, template.outputs, table, template.state, template.owns)


def machine_table(c: Contract) -> dict:
    need(c.state == (("state", "next_state", "unseen"),), "unsupported machine layout or initial state")
    need(tuple(p.name for p in c.inputs) == ("state", "event", "authorized") and
         tuple(p.name for p in c.outputs) == ("outcome", "next_state", "reason"), "unsupported machine ports")
    need(len(c.rows) == len(domain(c.inputs)) and all(len(ys) == 1 for _, ys in c.rows), "requires a total deterministic machine")
    return {x: ys[0] for x, ys in c.rows}


def distinguish(left: Contract, right: Contract, budget: Budget | None = None) -> dict:
    """Shortest trace distinguishing outcome/reason; internal state is hidden."""
    a, b = machine_table(left), machine_table(right)
    need(left.inputs[1:] == right.inputs[1:] and left.outputs[0] == right.outputs[0] and left.outputs[2] == right.outputs[2], "different observation interface")
    budget = budget if budget is not None else Budget()
    start = ("unseen", "unseen")
    queue, seen = deque([(start, ())]), {start}
    alphabet = tuple(product(left.inputs[1].values, left.inputs[2].values))
    try:
        while queue:
            (s, t), trace = queue.popleft()
            for event, auth in alphabet:
                budget.spend()
                y, z = a[s, event, auth], b[t, event, auth]
                next_trace = trace + ((event, auth),)
                if (y[0], y[2]) != (z[0], z[2]):
                    return result("distinguishing-trace", trace=next_trace, left=y, right=z,
                                  observations=["outcome", "reason"], work=budget.used)
                pair = (y[1], z[1])
                if pair not in seen:
                    seen.add(pair)
                    queue.append((pair, next_trace))
        return result("observationally-equivalent-finite-machines", pairs=len(seen), work=budget.used)
    except Incomplete:
        return result("incomplete", work=budget.used)


def check_projection(c: Contract, mapping: dict[str, str], budget: Budget | None = None) -> dict:
    """Check a deterministic Mealy quotient over every concrete state/input.

    A failure forbids advertising this projection as an exact alternate view;
    it does not forbid a separately labelled over-approximation or illustration.
    """
    table = machine_table(c)
    need(type(mapping) is dict and set(mapping) == set(c.inputs[0].values), "projection must be total and exact-domain")
    for value in mapping.values():
        name(value)
    budget = budget if budget is not None else Budget()
    seen = {}
    try:
        for (s, event, auth), (outcome, target, reason) in sorted(table.items()):
            budget.spend()
            key, projected = (mapping[s], event, auth), (outcome, mapping[target], reason)
            if key in seen and seen[key][1] != projected:
                return result("not-an-exact-quotient", witness={"abstract_input": key, "left": seen[key], "right": (s, projected)})
            seen[key] = (s, projected)
        return result("exact-quotient-within-finite-model", concrete_states=len(mapping),
                      abstract_states=len(set(mapping.values())), cases=len(table), work=budget.used,
                      initial=mapping["unseen"], source=c.revision, mapping_digest=digest(mapping))
    except Incomplete:
        return result("incomplete", work=budget.used)


@dataclass(frozen=True)
class Entry:
    id: str
    title: str
    tags: tuple[str, ...]
    meaning: Contract
    dependencies: tuple[tuple[str, str], ...] = ()
    license: str = "MIT"

    def __post_init__(self) -> None:
        name(self.id)
        need(type(self.title) is str and 1 <= len(self.title) <= 160, "invalid title")
        need(type(self.tags) is tuple and len(self.tags) <= 32 and all(type(t) is str and len(t) <= 64 for t in self.tags), "invalid tags")
        need(type(self.meaning) is Contract, "entry needs a closed finite meaning")
        need(type(self.dependencies) is tuple and len(self.dependencies) <= 16, "invalid dependencies")
        for dependency in self.dependencies:
            need(type(dependency) is tuple and len(dependency) == 2, "invalid dependency")
            key, pin = dependency
            name(key)
            need(type(pin) is str and re.fullmatch(r"[a-f0-9]{64}", pin) is not None, "invalid dependency pin")
        need(len({key for key, _ in self.dependencies}) == len(self.dependencies), "duplicate dependency")
        need(self.license == "MIT", "pilot accepts only these originally authored MIT entries")

    @property
    def revision(self) -> str:
        return digest({"meaning": self.meaning.data(), "dependencies": self.dependencies})


class Library:
    def __init__(self, entries: tuple[Entry, ...]):
        self.entries = entries  # Frozen Entry values; no executable plugins.
        self.current_entries()

    def current_entries(self) -> tuple[Entry, ...]:
        """Validate and capture this call's collection before any iteration."""
        entries = self.entries
        need(type(entries) is tuple and len(entries) <= 128 and all(type(e) is Entry for e in entries), "invalid library")
        need(len({e.id for e in entries}) == len(entries), "duplicate entry ID")
        return entries

    def resolve(self, entry_id: str, pin: str | None = None, stack: tuple = ()) -> Entry:
        # Resolve shared dependency graphs once per call. A cache from an older
        # library could hide a changed pin; this cache is never retained.
        entries = {entry.id: entry for entry in self.current_entries()}
        revisions, resolved = {}, set()

        def visit(key, expected, active):
            name(key)
            need(key not in active, "dependency cycle")
            need(key in entries, "unknown entry")
            entry = entries[key]
            if key not in revisions:
                revisions[key] = entry.revision
            need(expected is None or expected == revisions[key], "stale semantic dependency")
            if key not in resolved:
                for dependency, revision in entry.dependencies:
                    visit(dependency, revision, active + (key,))
                resolved.add(key)
            return entry

        return visit(entry_id, pin, stack)

    def discover(self, text: str) -> list[dict]:
        """Lexical/curated-tag retrieval ONLY; a hit asserts no semantic fit."""
        need(type(text) is str and len(text) <= 1024, "query exceeds bound")
        words = set(re.findall(r"[a-z0-9_]+", text.lower()))
        hits = []
        for e in self.current_entries():
            terms = set(re.findall(r"[a-z0-9_]+", (e.title + " " + " ".join(e.tags)).lower()))
            score = len(words & terms)
            if score:
                hits.append({"id": e.id, "revision": e.revision, "title": e.title, "retrieval_score": score,
                             "semantic_fit": "not-checked", "authority": "none"})
        return sorted(hits, key=lambda h: (-h["retrieval_score"], h["id"]))


def complete_product(left: Contract, target: Contract, library: Library,
                     budget: Budget | None = None) -> dict:
    """Enumerate a missing right-hand component from a finite library.

    This is a restricted completion search, NOT a general contract quotient.
    It preserves the exact target and never invents an assumption or choice.
    """
    budget = budget if budget is not None else Budget()
    checked, matches, incomplete = [], [], False
    need(type(library) is Library, "requires a finite library")
    catalog = Library(library.current_entries())
    for entry in catalog.entries:
        try:
            catalog.resolve(entry.id, entry.revision)
            combined = independent_product(left, entry.meaning, budget)
            verdict = fit(combined, target, budget)
            checked.append({"entry": entry.id, "status": verdict["status"]})
            if verdict["status"] == "covers-within-finite-profile":
                matches.append({"entry": entry.id, "entry_revision": entry.revision,
                                "composed_revision": combined.revision})
            incomplete |= verdict["status"] == "incomplete"
        except Incomplete:
            incomplete = True
            checked.append({"entry": entry.id, "status": "incomplete"})
            break
        except Refused as exc:
            checked.append({"entry": entry.id, "status": "unsupported-or-stale", "reason": str(exc)})
    status = "incomplete" if incomplete else "checked-completions" if matches else "no-certified-completion-in-library"
    return result(status, target=target.revision, matches=matches, checked=checked,
                  library_exhausted=not incomplete, work=budget.used)


def render_table(c: Contract, english: bool = False) -> str:
    """Two exact research-table views, not a new production ZAL grammar.

    Declarations are canonical JSON data; each relation row has a distinct
    symbolic / controlled-English surface. A UI should progressively disclose
    these rows instead of presenting the whole table as an explanation.
    """
    data = c.data()
    rows = data.pop("rows")
    lines = [("Meaning table " if english else "TABLE ") + canonical(data).decode()]
    for row in rows:
        x, ys = canonical(row["input"]).decode(), canonical(row["outputs"]).decode()
        lines.append(f"For input {x}, permit exactly {ys}." if english else f"{x} => {ys}")
    return "\n".join(lines) + "\n"


def parse_table(source: str, english: bool = False) -> Contract:
    need(type(source) is str and source.isascii() and len(source.encode()) <= MAX_BYTES, "table source exceeds profile")
    lines = source.splitlines()
    prefix = "Meaning table " if english else "TABLE "
    need(bool(lines) and lines[0].startswith(prefix) and len(lines) <= MAX_ROWS + 1, "bad table header")
    try:
        data = json.loads(lines[0][len(prefix):], object_pairs_hook=no_duplicates)
        need(type(data) is dict and "rows" not in data, "rows belong in table body")
        rows = []
        for line in lines[1:]:
            if english:
                need(line.startswith("For input ") and line.endswith("."), "bad controlled table sentence")
                pieces = line[len("For input "):-1].split(", permit exactly ")
            else:
                pieces = line.split(" => ")
            need(len(pieces) == 2, "bad table row")
            rows.append({"input": json.loads(pieces[0]), "outputs": json.loads(pieces[1])})
        data["rows"] = rows
        return from_data(data)
    except (json.JSONDecodeError, TypeError, KeyError, RecursionError) as exc:
        raise Refused("malformed table") from exc


def emit_zal(c: Contract, machine: str = "reservation") -> str:
    """Emit the existing finite-fsm/1 syntax; native qualification is separate.

    The prototype's accept reason `ok` maps to native Accept's absence of a
    rejection reason. Only this named complete, effect-free machine layout fits.
    """
    name(machine)
    table = machine_table(c)
    need(tuple((p.name, p.role, p.unit) for p in c.inputs) == (
        ("state", "lifecycle", "control"),
        ("event", "operation", "control"),
        ("authorized", "authorization_observation", "boolean")),
        "unsupported native input port meanings")
    need(tuple((p.name, p.role, p.unit) for p in c.outputs) == (
        ("outcome", "business_decision", "control"),
        ("next_state", "lifecycle", "control"),
        ("reason", "decision_reason", "control")),
        "unsupported native output port meanings")
    need(c.outputs[0].values == ("accept", "reject"), "unsupported native decision encoding")
    # The research profile admits 48-character names; finite-fsm/1 admits
    # 32 and reserves grammar words. Do not emit source outside that language.
    reserved = {"none", "true", "false", "state", "not", "and", "or", "is", "default"}
    native_names = (machine, *c.inputs[0].values, *c.inputs[1].values,
                    *c.outputs[2].values)
    need(all(len(value) <= 32 and value not in reserved for value in native_names),
         "native finite-fsm/1 identifiers must be at most 32 characters and not reserved")
    need(c.inputs[2].values == ("no", "yes"), "authorization must be Boolean")
    need(len(c.inputs[0].values) <= 8 and len(c.inputs[1].values) <= 8
         and len(table) <= 32, "native finite-fsm/1 shape limit")
    lines = ["zal 1;", f"machine {machine};", "states " + ", ".join(c.inputs[0].values) + ";",
             "events " + ", ".join(c.inputs[1].values) + ";", "context authorized;", "initial unseen;",
             "step atomic; frame control_only;", "default reject unmatched unchanged;"]
    for i, ((s, event, auth), (decision, target, reason)) in enumerate(sorted(table.items())):
        need(decision in {"accept", "reject"}, "unsupported decision")
        need((decision == "accept" and reason == "ok") or (decision == "reject" and target == s), "native observation mapping would lose meaning")
        guard = "authorized" if auth == "yes" else "!authorized"
        outcome = f"accept {target}" if decision == "accept" else f"reject {reason} unchanged"
        lines.append(f"rule r{i}: {s} + {event} [{guard}] -> {outcome};")
    return "\n".join(lines) + "\n"


def demo() -> dict:
    once, reuse = slot(reusable=False), slot(reusable=True)
    entries = (
        Entry("reservation_once", "Replay-safe one-identity reservation", ("hold", "capacity", "seat", "inventory", "replay", "reservation"), once),
        Entry("reservation_reusable", "Reusable one-identity reservation", ("hold", "capacity", "seat", "inventory", "reuse", "reservation"), reuse),
    )
    lib = Library(entries)
    contradiction = meet(requirement(once, "once_after_release"), requirement(once, "reuse_after_release"))
    paired = independent_product(once, slot(reusable=False, resource="other_slot"))
    projection = {"unseen": "idle", "held": "held", "released": "idle"}
    return result("pilot-results", profile=PROFILE,
                  retrieval=lib.discover("reserve a seat hold capacity"),
                  exact_fit=fit(once, once), wrong_fit=fit(reuse, once),
                  witness=distinguish(once, reuse),
                  inconsistent_conjunction=fit(once, contradiction),
                  independent_product={"rows": len(paired.rows), "revision": paired.revision, "semantics": "synchronous-disjoint-product"},
                  unsafe_abstraction=check_projection(once, projection),
                  safe_abstraction=check_projection(reuse, projection),
                  zero_budget=fit(once, once, Budget(0)),
                  completion=complete_product(once, paired, Library((
                      Entry("reusable_right", "Reusable right slot", (), slot(reusable=True, resource="other_slot")),
                      Entry("once_right", "Replay-safe right slot", (), slot(reusable=False, resource="other_slot")),
                  ))),
                  surface_roundtrips={"symbolic": parse_table(render_table(once)) == once,
                                      "controlled_table": parse_table(render_table(once, True), True) == once},
                  native_zal={"status": "emitted-not-qualified", "profile": "zal/finite-fsm/1", "source": emit_zal(once)})


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["demo"])
    parser.parse_args()
    print(json.dumps(demo(), indent=2, sort_keys=True))
