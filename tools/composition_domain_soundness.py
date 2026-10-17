#!/usr/bin/env python3
"""Fixtures, fresh-seed soundness run and controls for the composed-domain profile.

Subcommands:

- `fixtures --cli BIN [--check]` writes (or, with --check, compares) the committed
  `/3` fixtures under crates/zeno-fcis-contract/tests/fixtures/checked-composition-3:
  the guarded budget, the ten-core ladder with its over-budget variants, and the
  planted faults. Components are the declared core-component instances, made with
  tools/instantiate_core.py and compiled with `generate contract`, and the fixture
  contracts under `contracts/`. Every pin is computed, never copied.
- `run --cli BIN --out DIR [--seed S] [--plain N] [--wired N] [--start K --count M]`
  generates the compositions of the seed (plain and wired), checks each with `/3`,
  explores its reachable product with the independent reference
  (tools/composition_domain_reference.py) and compares the joint step on every
  visited (global state, request) pair with `--joint-table`. One JSON line per
  composition goes to DIR/results-K.jsonl. Planted controls run with every batch:
  a changed joint-step row and a planted unsound record must both be caught, or the
  run fails.
- `summary --out DIR` aggregates every batch: verdict counts, oracle completeness,
  unsound records, joint-step differences and coverage. Exit 0 only with no unsound
  record, no joint-step difference and at least --min-complete complete oracle runs.
- `control --cli BIN --out DIR` plants a wrong joint step in the checker's own joint
  table (a CommittedFailure that commits only its own successor) and runs the joint-step
  comparison on it; it must report differences. The wrong step is never part of a build.
- `self-test --cli BIN --out DIR` runs a small batch with the comparisons stubbed and
  requires the run to fail, then without the stubs and requires it to pass.
- `faults --cli BIN` checks every planted fault fixture and requires the reference to
  reach the same verdict and the same first witness.

The model proposes nothing here: every verdict is the `/3` checker's, and the
reference is a separately written oracle used only to test it.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import random
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
import composition_domain_reference as reference  # noqa: E402
import instantiate_core  # noqa: E402

FIXTURES = ROOT / "crates/zeno-fcis-contract/tests/fixtures/checked-composition-3"
CONTRACTS = FIXTURES / "contracts"
SCHEMA = "zeno-fcis/checked-composition/3"
CHECKER = "zeno-fcis/composed-domain-checker/1"
SEED = 20261020
SMALL = ("bounded-counter-c1", "bounded-counter-c2", "bounded-counter-c3", "consumable-budget-c1",
         "consumable-budget-c2", "consumable-budget-c3", "finite-phase-machine-c1", "finite-phase-machine-c2",
         "retry-budget-c1", "retry-budget-c2", "reservation-pool-c1-q1", "reservation-pool-c2-q1",
         "versioned-register-c1", "idempotency-slot-c1", "logical-deadline-c1", "rate-limiter-n1-w1")
FIXTURE_CONTRACTS = ("ticker", "tripwire")
REQUEST = [{"name": "op", "domain": {"kind": "int", "min": 0, "max": 2}},
           {"name": "a", "domain": {"kind": "int", "min": 0, "max": 1}},
           {"name": "authorized", "domain": {"kind": "bool"}}]


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


# --- component sources -------------------------------------------------------------------------

def instance_tag(family: str, parameters: dict) -> str:
    return f"{family}-{instantiate_core.instance_id(parameters)}"


def instances() -> dict[str, tuple[str, dict]]:
    """Every declared core-component instance by tag: the 77, nothing else."""
    out = {}
    for family in instantiate_core.FAMILIES:
        for parameters in instantiate_core.definition(family)["instances"]:
            out[instance_tag(family, parameters)] = (family, parameters)
    return out


def component_files(tag: str) -> dict[str, bytes]:
    """`project.zeno` and `v2/policy.json` of a declared instance or a fixture contract."""
    if (CONTRACTS / tag).is_dir():
        return {name: (CONTRACTS / tag / name).read_bytes() for name in ("project.zeno", "v2/policy.json")}
    family, parameters = instances()[tag]
    files = instantiate_core.source(family, parameters, include_proof=False)
    return {name: files[name] for name in ("project.zeno", "v2/policy.json")}


def write_component(directory: Path, files: dict[str, bytes]) -> None:
    for name, data in files.items():
        target = directory / name
        target.parent.mkdir(parents=True, exist_ok=True)
        if not target.exists() or target.read_bytes() != data:
            target.write_bytes(data)


class Pins:
    """Each component directory's three pins; the program's comes from the checker itself."""

    def __init__(self, cli: str, scratch: Path):
        self.cli, self.scratch, self.cache, self.generated = cli, scratch, {}, set()

    def generate(self, directory: Path) -> None:
        key = sha256((directory / "project.zeno").read_bytes() + (directory / "v2/policy.json").read_bytes())
        if key in self.generated:
            return
        out = self.scratch / "generated" / key[:16]
        shutil.rmtree(out, ignore_errors=True)
        result = subprocess.run([self.cli, "generate", "contract", str(directory), "--out", str(out),
                                 "--format", "json"], capture_output=True, text=True)
        report = json.loads(result.stdout or "{}")
        if result.returncode != 0 or report.get("status") != "generated":
            raise SystemExit(f"generate contract refused {directory}: {result.stdout} {result.stderr}")
        shutil.rmtree(out, ignore_errors=True)
        self.generated.add(key)

    def of(self, directory: Path) -> dict[str, str]:
        project = (directory / "project.zeno").read_bytes()
        policy = (directory / "v2/policy.json").read_bytes()
        key = sha256(project + policy)
        if key not in self.cache:
            self.generate(directory)
            probe = self.scratch / "probe"
            shutil.rmtree(probe, ignore_errors=True)
            write_component(probe / "c", {"project.zeno": project, "v2/policy.json": policy})
            manifest = {"schema": SCHEMA, "checker": CHECKER, "name": "probe", "request": [],
                        "components": [{"name": "c", "path": "c", "project_sha256": sha256(project),
                                        "policy_sha256": sha256(policy), "program_sha256": "0" * 64,
                                        "owns": [], "participates": "true", "bind": {}, "assume": "true",
                                        "invariant": "true"}], "couplings": []}
            (probe / "probe.json").write_text(json.dumps(manifest))
            result = subprocess.run([self.cli, "contract", "check-composition", str(probe / "probe.json"),
                                     "--format", "json"], capture_output=True, text=True)
            refusal = json.loads(result.stdout).get("refusal") or {}
            program = (refusal.get("witness") or {}).get("bound_program_sha256")
            if refusal.get("obligation") != "program binding" or not program:
                raise SystemExit(f"cannot learn the program identity of {directory}: {result.stdout}")
            self.cache[key] = {"project_sha256": sha256(project), "policy_sha256": sha256(policy),
                               "program_sha256": program}
        return self.cache[key]


def layout(directory: Path, name: str = "c"):
    """A component's fields as the reference reads them."""
    return reference.Component({"name": name, "owns": [], "participates": "true", "bind": {},
                                "assume": "true", "invariant": "true"}, directory)


def manifest_text(name, request, components, couplings, limits=None) -> str:
    manifest = {"schema": SCHEMA, "checker": CHECKER, "name": name, "request": request,
                "components": components, "couplings": couplings}
    if limits is not None:
        manifest["limits"] = limits
    return json.dumps(manifest, indent=2, sort_keys=True) + "\n"


def entry(pins: Pins, base: Path, name: str, path: str, owns: bool, participates: str, bind: dict,
          assume: str = "true", invariant: str = "true") -> dict:
    directory = base / path
    fields = layout(directory, name)
    return {"name": name, "path": path, **pins.of(directory),
            "owns": [f"{name}.{field}" for field in fields.state_names] if owns else [],
            "participates": participates, "bind": bind, "assume": assume, "invariant": invariant}


# --- the committed fixtures ----------------------------------------------------------------------

LADDER = [
    ("budget", "consumable-budget-c8", "op == 0", {"amount": "qty + qty", "authorized": "authorized"},
     "amount >= 2 && amount <= 6", "true"),
    ("pool", "reservation-pool-c4-q3", "op == 0 || op == 1",
     {"action": "choose(op == 0, 150, 152)", "quantity": "qty", "authorized": "authorized"},
     "action == 150 || action == 152", "available + reserved <= 4"),
    ("counter", "bounded-counter-c8", "op == 0", {"action": "150", "authorized": "authorized"}, "true", "true"),
    ("limiter", "rate-limiter-n3-w3", "op == 0", {"command": "150", "now": "now", "authorized": "authorized"},
     "true", "true"),
    ("dedup", "idempotency-slot-c8", "op == 0",
     {"command_key": "key", "command_value": "val", "authorized": "authorized"}, "true", "true"),
    ("retry", "retry-budget-c8", "op == 2", {"finish": "qty == 3", "authorized": "authorized"}, "true", "true"),
    ("register", "versioned-register-c8", "op == 3",
     {"expected_version": "ver", "new_value": "val", "authorized": "authorized"}, "true", "true"),
    ("deadline", "logical-deadline-c8", "op == 3", {"now": "now", "authorized": "authorized"}, "true", "true"),
    ("phase", "finite-phase-machine-c8", "op == 2 || op == 3",
     {"reset": "op == 3 && qty == 1", "authorized": "authorized"}, "true", "true"),
    ("approval", "approval-queue-k1", "op == 1",
     {"action": "159 + qty", "principal": "principal", "authorized": "authorized"}, "true", "true"),
]
LADDER_COUPLINGS = [
    {"name": "count", "members": ["pool", "counter"], "invariant": "counter.value + pool.available <= 4"},
    {"name": "rate", "members": ["counter", "limiter"], "invariant": "counter.value >= limiter.used"},
    {"name": "spend", "members": ["budget", "pool"], "invariant": "budget.remaining + pool.reserved + pool.reserved <= 8"},
]
WIDE = {"name": "wide", "members": ["dedup", "register", "deadline"],
        "invariant": "dedup.key + register.version + deadline.last_seen <= 24"}
LADDER_REQUEST = [
    {"name": "op", "domain": {"kind": "int", "min": 0, "max": 3}},
    {"name": "qty", "domain": {"kind": "int", "min": 1, "max": 3}},
    {"name": "key", "domain": {"kind": "int", "min": 0, "max": 8}},
    {"name": "val", "domain": {"kind": "int", "min": 0, "max": 8}},
    {"name": "now", "domain": {"kind": "int", "min": 0, "max": 6}},
    {"name": "principal", "domain": {"kind": "int", "min": 0, "max": 2}},
    {"name": "ver", "domain": {"kind": "int", "min": 0, "max": 8}},
    {"name": "authorized", "domain": {"kind": "bool"}},
]
FAULT_REQUEST = [{"name": "op", "domain": {"kind": "int", "min": 0, "max": 2}},
                 {"name": "qty", "domain": {"kind": "int", "min": 1, "max": 2}},
                 {"name": "authorized", "domain": {"kind": "bool"}}]


def fixture_sets(pins: Pins, root: Path) -> dict[str, str]:
    """Every committed manifest, by path under `root`, after writing its components."""
    out = {}
    # The guarded budget: a read-only guard authorizes the budget's spend.
    guarded = root / "guarded"
    write_component(guarded / "guard", component_files("guard"))
    write_component(guarded / "budget", component_files("consumable-budget-c3"))
    request = [{"name": "amount", "domain": {"kind": "int", "min": 0, "max": 3}},
               {"name": "authorized", "domain": {"kind": "bool"}}]
    out["guarded/composition.json"] = manifest_text("guarded-budget", request, [
        entry(pins, guarded, "guard", "guard", False, "true",
              {"mark": "0", "request": "0", "authorized": "authorized"}, "true", "mark == 0"),
        entry(pins, guarded, "budget", "budget", True, "true", {"amount": "amount", "authorized": "true"},
              "authorized == 1", "remaining <= 3"),
    ], [])
    # The same guard as a `/1` leaf, which `/1` refuses: a leaf owns state.
    guard_pins = pins.of(guarded / "guard")
    out["guarded/v1-guard.json"] = json.dumps({
        "schema": "zeno-fcis/checked-composition/1", "checker": "zeno-fcis/checked-composition-checker/1",
        "name": "guard-as-a-leaf", "edges": [],
        "leaves": [{"id": 1, "name": "guard", "path": "guard", **guard_pins, "owns": [],
                    "footprint": {"reads": ["pre.100.110", "command.101.120", "context.102.130"], "writes": []},
                    "invariant": "true", "assumption": "true", "guarantees": [], "views": [],
                    "witnesses": ["accept", "reject"]}],
    }, indent=2, sort_keys=True) + "\n"
    # The ten-core ladder, its wide coupling and a lowered total.
    ladder = root / "ladder-10"
    components = []
    for name, tag, participates, bind, assume, invariant in LADDER:
        write_component(ladder / name, component_files(tag))
        components.append(entry(pins, ladder, name, name, True, participates, bind, assume, invariant))
    out["ladder-10/composition.json"] = manifest_text("ladder-10", LADDER_REQUEST, components, LADDER_COUPLINGS)
    out["ladder-10/wide.json"] = manifest_text(
        "ladder-10-wide", LADDER_REQUEST, components,
        sorted(LADDER_COUPLINGS + [WIDE], key=lambda coupling: coupling["name"]))
    out["ladder-10/lowered-total.json"] = manifest_text(
        "ladder-10-lowered-total", LADDER_REQUEST, components, LADDER_COUPLINGS,
        {"max_obligation": 1 << 22, "max_total": 50000})
    # The profile specification's documented cases: a procurement of three
    # components with two couplings, and the components of its other cases.
    documented = root / "documented"
    for name, tag in (("budget", "consumable-budget-c4"), ("pool", "reservation-pool-c2-q1"),
                      ("counter", "bounded-counter-c4"), ("small-budget", "consumable-budget-c1"),
                      ("counter-c2", "bounded-counter-c2"), ("ticker", "ticker"), ("tripwire", "tripwire")):
        write_component(documented / name, component_files(tag))
    out["documented/procurement.json"] = manifest_text("procurement", [
        {"name": "op", "domain": {"kind": "int", "min": 0, "max": 1}},
        {"name": "authorized", "domain": {"kind": "bool"}}], [
        entry(pins, documented, "budget", "budget", True, "op == 0", {"amount": "2", "authorized": "authorized"},
              "true", "remaining <= 4"),
        entry(pins, documented, "pool", "pool", True, "true",
              {"action": "choose(op == 0, 150, 152)", "quantity": "1", "authorized": "authorized"},
              "action == 150 || action == 152", "available + reserved <= 2"),
        entry(pins, documented, "counter", "counter", True, "op == 0", {"action": "150", "authorized": "authorized"}),
    ], [{"name": "count", "members": ["pool", "counter"], "invariant": "counter.value + pool.available <= 2"},
        {"name": "spend", "members": ["budget", "pool"],
         "invariant": "budget.remaining + pool.reserved + pool.reserved <= 4"}])
    # A proved composition whose reachable joint steps include committed
    # failures: two lockstep counters, then a wire that fails on every step.
    lockstep = lambda name: entry(pins, documented, name, f"lockstep-{name}", True, "true",  # noqa: E731
                                  {"action": "choose(op == 0, 150, 151)", "authorized": "authorized"})
    for name in ("a", "b"):
        write_component(documented / f"lockstep-{name}", component_files("bounded-counter-c2"))
    out["documented/lockstep-failure.json"] = manifest_text("lockstep-failure", [
        {"name": "op", "domain": {"kind": "int", "min": 0, "max": 1}},
        {"name": "authorized", "domain": {"kind": "bool"}}], [
        lockstep("a"), lockstep("b"),
        entry(pins, documented, "wire", "tripwire", True, "true", {"fire": "true", "authorized": "authorized"}),
    ], [{"name": "lock", "members": ["a", "b"], "invariant": "a.value == b.value"}])
    # Planted faults over small instances and the fixture contracts.
    faults = root / "faults"
    for name, tag in (("counter", "bounded-counter-c2"), ("budget", "consumable-budget-c2"),
                      ("ticker", "ticker"), ("tripwire", "tripwire"), ("guard", "guard")):
        write_component(faults / name, component_files(tag))
    counter = lambda **kw: entry(pins, faults, "counter", "counter", kw.get("owns", True), "op == 0",  # noqa: E731
                                 kw.get("bind", {"action": "150", "authorized": "authorized"}),
                                 "true", kw.get("invariant", "true"))
    budget = lambda **kw: entry(pins, faults, "budget", "budget", True, kw.get("participates", "op != 2"),  # noqa: E731
                                kw.get("bind", {"amount": "qty", "authorized": "authorized"}),
                                "amount >= 1", "true")
    out["faults/coupling-false-at-genesis.json"] = manifest_text(
        "coupling-false-at-genesis", FAULT_REQUEST, [counter(), budget()],
        [{"name": "k", "members": ["counter", "budget"], "invariant": "counter.value + budget.remaining <= 1"}])
    out["faults/coupling-broken-by-step.json"] = manifest_text(
        "coupling-broken-by-step", FAULT_REQUEST, [counter(), budget(participates="op == 1")],
        [{"name": "k", "members": ["counter", "budget"], "invariant": "counter.value + budget.remaining <= 2"}])
    later = counter(bind={"action": "choose(budget.remaining == 0, 151, 150)", "authorized": "authorized"})
    out["faults/binding-reads-later.json"] = manifest_text(
        "binding-reads-later", FAULT_REQUEST, [later, budget()], [])
    out["faults/binding-out-of-domain.json"] = manifest_text(
        "binding-out-of-domain", FAULT_REQUEST, [counter(), budget(bind={"amount": "qty + 1", "authorized": "authorized"})], [])
    out["faults/invariant-not-preserved.json"] = manifest_text(
        "invariant-not-preserved", FAULT_REQUEST, [counter(invariant="value <= 1"), budget()], [])
    two_owners = budget()
    two_owners["owns"] = ["budget.remaining", "counter.value"]
    out["faults/two-owners.json"] = manifest_text("two-owners", FAULT_REQUEST, [counter(), two_owners], [])
    read_only = counter(owns=False, bind={"value": "0", "action": "150", "authorized": "authorized"})
    out["faults/read-only-changes.json"] = manifest_text(
        "read-only-changes", FAULT_REQUEST, [budget(), read_only], [])
    # A refused successor and a committed failure inside a composition.
    out["faults/ticker-tripwire.json"] = manifest_text("ticker-tripwire", FAULT_REQUEST, [
        entry(pins, faults, "tripwire", "tripwire", True, "op != 2", {"fire": "op == 1", "authorized": "authorized"}),
        entry(pins, faults, "ticker", "ticker", True, "true", {"step": "qty - 1", "authorized": "authorized"}),
        budget(),
    ], [{"name": "k", "members": ["ticker", "budget"], "invariant": "ticker.count + budget.remaining <= 4"}])
    return out


def fixtures(args) -> int:
    scratch = Path(args.scratch)
    scratch.mkdir(parents=True, exist_ok=True)
    pins = Pins(args.cli, scratch)
    target = scratch / "fixtures-check" if args.check else FIXTURES
    if args.check:
        shutil.rmtree(target, ignore_errors=True)
        shutil.copytree(CONTRACTS, target / "contracts")
    manifests = fixture_sets(pins, target)
    for path, text in manifests.items():
        (target / path).write_text(text)
    if not args.check:
        print(json.dumps({"written": sorted(manifests)}))
        return 0
    differences = []
    for path in sorted(p.relative_to(target) for p in target.rglob("*") if p.is_file()):
        committed = FIXTURES / path
        if not committed.is_file() or committed.read_bytes() != (target / path).read_bytes():
            differences.append(str(path))
    print(json.dumps({"checked": len(manifests), "differences": differences}))
    return 1 if differences else 0


# --- random compositions ---------------------------------------------------------------------

def _binding(rng, name, domain, earlier_state=None):
    if domain == [0, 1]:
        if name == "authorized":
            return rng.choice(["authorized", "authorized", "true", "op != 2", "a == 1 || authorized"])
        return rng.choice(["a == 1", "op == 0", "true", "false"])
    lo, hi = min(domain), max(domain)
    options = [str(rng.choice(domain)),
               f"choose(op == {rng.randint(0, 2)}, {rng.choice(domain)}, {rng.choice(domain)})"]
    if hi >= lo + 1:
        options.append(f"{lo} + a")
    if hi >= lo + 2:
        options.append(f"{lo} + op")
    if rng.random() < 0.05:
        options = [str(hi + 1)]  # deliberately outside the field domain
    return rng.choice(options)


def _predicate(rng, fields):
    if not fields or rng.random() < 0.6:
        return "true"
    name, domain = rng.choice(fields)
    if domain == [0, 1]:
        return rng.choice([f"{name} == 1", f"{name} == 0", "true"])
    value = rng.choice(domain)
    return rng.choice([f"{name} <= {value}", f"{name} != {value}", f"{name} >= {value}"])


def _state_atom(rng, earlier):
    name, fields = rng.choice(earlier)
    field, domain = rng.choice(fields)
    return f"{name}.{field}", domain, rng.choice(domain)


def _wired_binding(rng, name, domain, earlier, force):
    if not earlier or (not force and rng.random() < 0.5):
        return _binding(rng, name, domain)
    atom, atom_domain, value = _state_atom(rng, earlier)
    if domain == [0, 1]:
        return rng.choice([f"{atom} == {value}", f"{atom} != {value}", f"{atom} <= {value} || authorized",
                           f"{atom} >= {value} && authorized"])
    lo, hi = min(domain), max(domain)
    options = [f"choose({atom} == {value}, {rng.choice(domain)}, {rng.choice(domain)})",
               f"choose({atom} <= {value}, {lo}, {hi})"]
    if atom_domain != [0, 1] and min(atom_domain) >= lo and max(atom_domain) <= hi:
        options.append(atom)
    options.append(f"{atom} + {lo - min(atom_domain)}")
    return rng.choice(options)


def generate(seed: int, plain: int, wired: int) -> list[dict]:
    """Every composition of the seed: tags, owners and the manifest's formulas."""
    rng = random.Random(seed)
    layouts = {}

    def fields_of(tag):
        if tag not in layouts:
            files = component_files(tag)
            directory = Path(tempfile.gettempdir()) / "composition-domain-layouts" / tag
            write_component(directory, files)
            component = layout(directory)
            layouts[tag] = ([(n, list(component.domains[n])) for n in component.state_names],
                            [(n, list(component.domains[n])) for n in component.input_names],
                            component.genesis)
        return layouts[tag]

    out = []
    for index in range(plain + wired):
        is_wired = index >= plain
        count = rng.randint(2, 3)
        tags = rng.sample(SMALL, count)
        if is_wired and rng.random() < 0.4:
            tags[rng.randrange(1, count) if count > 1 else 0] = rng.choice(FIXTURE_CONTRACTS)
        names = [f"c{k}" for k in range(count)]
        read_only = [False] * count
        if is_wired:
            for k in range(1, count):
                if rng.random() < 0.45:
                    read_only[k] = True
                    if rng.random() < 0.5:
                        tags[k] = "guard"
        components = []
        owned_earlier = []
        for k, (name, tag) in enumerate(zip(names, tags)):
            state, inputs, _genesis = fields_of(tag)
            participates = rng.choice(["true", "op == 0", "op != 1", "a == 1"])
            bind = {}
            if is_wired and owned_earlier:
                forced = rng.randrange(len(inputs))
                if rng.random() < 1 / 3:
                    atom, _, value = _state_atom(rng, owned_earlier)
                    participates = rng.choice([f"{atom} == {value}", f"{atom} != {value}",
                                               f"{atom} <= {value} && op != 2"])
                for j, (field, domain) in enumerate(inputs):
                    bind[field] = _wired_binding(rng, field, domain, owned_earlier, j == forced)
            else:
                for field, domain in inputs:
                    bind[field] = _binding(rng, field, domain)
            if read_only[k]:
                for field, domain in state:
                    copies = [f"{n}.{f}" for n, fs in owned_earlier for f, d in fs if d == domain]
                    if copies and rng.random() < 0.3:
                        bind[field] = rng.choice(copies)
                    else:
                        bind[field] = str(rng.choice(domain))
            components.append({"name": name, "tag": tag, "owns": not read_only[k], "participates": participates,
                               "bind": bind, "assume": _predicate(rng, inputs),
                               "invariant": _predicate(rng, state)})
            if not read_only[k]:
                owned_earlier.append((name, state))
        couplings = []
        owners = [c for c in components if c["owns"]]
        for j in range(rng.randint(0, 2) if len(owners) >= 2 else 0):
            x, y = sorted(rng.sample(range(len(owners)), 2))
            cx, cy = owners[x], owners[y]
            sx, _, gx_all = fields_of(cx["tag"])
            sy, _, gy_all = fields_of(cy["tag"])
            fx, fy = rng.randrange(len(sx)), rng.randrange(len(sy))
            gx, gy = gx_all[fx], gy_all[fy]
            form = rng.choice(["sum", "le", "eq"] if is_wired else ["sum", "le"])
            slack = rng.randint(0, 2)
            ax, ay = f"{cx['name']}.{sx[fx][0]}", f"{cy['name']}.{sy[fy][0]}"
            text = (f"{ax} + {ay} <= {gx + gy + slack}" if form == "sum"
                    else f"{ax} <= {ay} + {gx - gy + slack}" if form == "le"
                    else f"{ax} - {ay} == {gx - gy}")
            couplings.append({"name": f"k{j}", "members": [cx["name"], cy["name"]], "invariant": text})
        out.append({"index": index, "kind": "wired" if is_wired else "plain", "components": components,
                    "couplings": couplings})
    return out


def materialize(spec: dict, pool: Path, pins: Pins) -> Path:
    components = []
    used = set()
    for component in spec["components"]:
        # The profile refuses two components with one directory: a second
        # use of a contract gets its own copy.
        path = component["tag"]
        while path in used:
            path += "-again"
        used.add(path)
        write_component(pool / path, component_files(component["tag"]))
        components.append(entry(pins, pool, component["name"], path, component["owns"],
                                component["participates"], component["bind"], component["assume"],
                                component["invariant"]))
    path = pool / f"composition-{spec['index']:04d}.json"
    path.write_text(manifest_text(f"composition-{spec['index']:04d}", REQUEST, components, spec["couplings"]))
    return path


# --- one composition: the check, the oracle and the joint-step comparison ---------------------------

def cli_json(cli, *args):
    result = subprocess.run([cli, "contract", "check-composition", *map(str, args), "--format", "json"],
                            capture_output=True, text=True)
    try:
        return result.returncode, json.loads(result.stdout)
    except json.JSONDecodeError:
        return result.returncode, {"status": "unreadable", "stdout": result.stdout, "stderr": result.stderr}


def canonical_rows_rust(table: dict) -> dict:
    names = table["state_layout"]
    rows = {}
    for row in table["rows"]:
        state = tuple(sorted(zip(names, row["state"])))
        after = tuple(sorted(zip(names, row["successor"])))
        outcome = row["outcome"]
        detail = outcome.get("reason") if outcome["kind"] in ("Reject", "CommittedFailure") else outcome.get("class")
        rows[(state, tuple(row["request"]))] = (outcome["kind"], outcome.get("component"), detail, after)
    return rows


def canonical_rows_reference(composition: reference.Composition, rows) -> dict:
    names = [f"{c.name}.{f}" for c in composition.owned for f in c.state_names]
    out = {}
    for state, request, outcome, after in rows:
        flat_state = [v for part in state for v in part]
        flat_after = [v for part in after for v in part]
        detail = outcome[2] if len(outcome) > 2 else None
        out[(tuple(sorted(zip(names, flat_state))), tuple(request))] = (
            outcome[0], outcome[1] if len(outcome) > 1 else None, detail, tuple(sorted(zip(names, flat_after))))
    return out


def compare_rows(rust: dict, ours: dict, stub: bool) -> list:
    if stub:
        return []
    differences = []
    for key in sorted(set(rust) | set(ours), key=repr):
        if rust.get(key) != ours.get(key):
            differences.append({"pair": repr(key), "rust": repr(rust.get(key)), "reference": repr(ours.get(key))})
    return differences


def judge(spec, manifest: Path, cli: str, max_pairs: int, stub: bool) -> dict:
    code, checked = cli_json(cli, manifest)
    record = {"index": spec["index"], "kind": spec["kind"], "manifest": manifest.name,
              "tags": [c["tag"] for c in spec["components"]],
              "read_only": sum(not c["owns"] for c in spec["components"]),
              "couplings": len(spec["couplings"])}
    if "result" in checked:
        record["verdict"] = checked["result"]["verdict"]
        record["first_witness"] = checked["result"]["witness"]
        record["evaluated_equals_planned"] = all(
            o["evaluated"] == int(o["size"]) for o in checked["result"]["obligations"] if o["status"] == "proved")
    else:
        record["verdict"] = "validation-refused"
        record["refusal"] = checked.get("refusal") or checked.get("error")
        try:
            reference.Composition(manifest)
            record["reference_validation"] = "accepted"
        except reference.Refused as error:
            record["reference_validation"] = f"refused: {error}"
        return record
    try:
        composition = reference.Composition(manifest)
    except reference.Refused as error:
        record["reference_validation"] = f"refused: {error}"
        return record
    record["reference_validation"] = "accepted"
    oracle = composition.oracle(max_pairs)
    record["oracle_complete"] = oracle["complete"]
    record["oracle_pairs"] = oracle["pairs"]
    record["oracle_violations"] = len(oracle["violations"])
    record["oracle_first_violation"] = oracle["violations"][0] if oracle["violations"] else None
    record["outcomes"] = oracle["outcomes"]
    record["reference_obligations"] = composition.obligations()["verdict"]
    record["coupling_provider_read"] = any(
        len(composition.coupling_spaces(members)[1]) > len(members) for _, members, _ in composition.couplings)
    if oracle["complete"]:
        code, table = cli_json(cli, manifest, "--joint-table", max_pairs)
        if table.get("status") != "explored":
            record["joint_table"] = table.get("status")
            record["differences"] = None
        else:
            rust = canonical_rows_rust(table)
            ours = canonical_rows_reference(composition, oracle["rows"])
            record["compared_pairs"] = len(ours)
            record["differences"] = len(compare_rows(rust, ours, stub))
            record["first_difference"] = (compare_rows(rust, ours, stub) or [None])[0]
            record["refused_delivery"] = any(v[0] == "Refused" and str(v[2]).startswith("binding")
                                             for v in rust.values())
    return record


def unsound(record: dict, stub: bool) -> bool:
    if stub:
        return False
    return (record.get("verdict") == "proved" and record.get("oracle_complete")
            and record.get("oracle_violations", 0) > 0)


def planted_controls(records: list, stub: bool) -> list[str]:
    """Both comparisons must catch a planted fault, or the batch fails."""
    failures = []
    # A changed joint-step row is a difference.
    rust = {("s",): ("Accept", None, None, (("c0.x", 1),))}
    ours = {("s",): ("Accept", None, None, (("c0.x", 2),))}
    if not compare_rows(rust, ours, stub):
        failures.append("a planted joint-step difference was not reported")
    # A planted proved verdict on a composition the oracle finds a violation in is unsound.
    violated = next((r for r in records if r.get("oracle_complete") and r.get("oracle_violations")), None)
    planted = dict(violated or {"oracle_complete": True, "oracle_violations": 1})
    planted["verdict"] = "proved"
    if not unsound(planted, stub):
        failures.append("a planted unsound record was not reported")
    return failures


def run(args) -> int:
    out = Path(args.out)
    pool = out / "pool"
    pool.mkdir(parents=True, exist_ok=True)
    pins = Pins(args.cli, out / "scratch")
    specs = generate(args.seed, args.plain, args.wired)
    chosen = specs[args.start:args.start + args.count] if args.count else specs[args.start:]
    records = []
    with open(out / f"results-{args.start:04d}.jsonl", "w") as stream:
        for spec in chosen:
            manifest = materialize(spec, pool, pins)
            record = judge(spec, manifest, args.cli, args.max_pairs, args.stub_comparison)
            record["unsound"] = unsound(record, args.stub_comparison)
            records.append(record)
            stream.write(json.dumps(record, sort_keys=True, default=str) + "\n")
            stream.flush()
    failures = planted_controls(records, args.stub_comparison)
    bad = [r["index"] for r in records if r.get("unsound") or r.get("differences")]
    print(json.dumps({"batch": args.start, "compositions": len(records), "unsound_or_different": bad,
                      "planted_control_failures": failures}))
    return 1 if failures or bad else 0


def summary(args) -> int:
    out = Path(args.out)
    records = []
    for path in sorted(out.glob("results-*.jsonl")):
        records.extend(json.loads(line) for line in path.read_text().splitlines() if line)
    count = lambda predicate: sum(1 for r in records if predicate(r))  # noqa: E731
    complete = [r for r in records if r.get("oracle_complete")]
    table = {
        "compositions": len(records),
        "plain": count(lambda r: r["kind"] == "plain"),
        "wired": count(lambda r: r["kind"] == "wired"),
        "verdicts": {v: count(lambda r, v=v: r["verdict"] == v)
                     for v in ("proved", "violated", "refused", "validation-refused")},
        "oracle_complete": len(complete),
        "oracle_over_budget": [r["index"] for r in records if r.get("oracle_complete") is False],
        "unsound": [r["index"] for r in records if r.get("unsound")],
        "proved_with_complete_oracle": count(lambda r: r["verdict"] == "proved" and r.get("oracle_complete")),
        "violated_with_oracle_violation": count(lambda r: r["verdict"] == "violated" and r.get("oracle_violations")),
        "violated_with_clean_oracle": count(lambda r: r["verdict"] == "violated" and r.get("oracle_complete")
                                            and not r.get("oracle_violations")),
        "reference_obligations_agree": count(lambda r: r.get("reference_obligations") == r["verdict"]),
        "reference_obligations_disagree": [r["index"] for r in records if "reference_obligations" in r
                                           and r["reference_obligations"] != r["verdict"]],
        "joint_step_pairs_compared": sum(r.get("compared_pairs", 0) for r in records),
        "joint_step_differences": sum(r.get("differences") or 0 for r in records),
        "joint_table_not_compared": [r["index"] for r in complete if r.get("differences") is None],
        "evaluated_not_planned": [r["index"] for r in records if r.get("evaluated_equals_planned") is False],
        "validation_disagreements": [r["index"] for r in records
                                     if (r["verdict"] == "validation-refused")
                                     != str(r.get("reference_validation", "")).startswith("refused")],
        "coverage": {
            "committed_failure_step": count(lambda r: (r.get("outcomes") or {}).get("CommittedFailure")),
            "refused_delivery": count(lambda r: r.get("refused_delivery")),
            "read_only_component": count(lambda r: r.get("read_only")),
            "read_only_components": sum(r.get("read_only", 0) for r in records),
            "components": sum(len(r["tags"]) for r in records),
            "fixture_contract": count(lambda r: set(r["tags"]) & set(FIXTURE_CONTRACTS)),
            "coupling_with_provider_read": count(lambda r: r.get("coupling_provider_read")),
        },
    }
    print(json.dumps(table, indent=2, sort_keys=True))
    ok = (not table["unsound"] and table["joint_step_differences"] == 0 and not table["joint_table_not_compared"]
          and len(complete) >= args.min_complete and not table["evaluated_not_planned"]
          and not table["validation_disagreements"])
    return 0 if ok else 1


def wrong_committed_failure(table: dict) -> dict:
    """The planted wrong step: a CommittedFailure commits only its own component's successor."""
    names = table["state_layout"]
    planted = dict(table, rows=[])
    for row in table["rows"]:
        row = dict(row)
        outcome = row["outcome"]
        if outcome["kind"] == "CommittedFailure":
            owner = outcome["component"] + "."
            row["successor"] = [after if name.startswith(owner) else before
                                for name, before, after in zip(names, row["state"], row["successor"])]
        planted["rows"].append(row)
    return planted


def control(args) -> int:
    """The joint-step comparison on a planted wrong joint step must report differences."""
    out = Path(args.out)
    pool = out / "pool"
    pool.mkdir(parents=True, exist_ok=True)
    pins = Pins(args.cli, out / "scratch")
    specs = [s for s in generate(args.seed, args.plain, args.wired)
             if any(c["tag"] == "tripwire" for c in s["components"])]
    differing, compared, honest_differences = 0, 0, 0
    for spec in specs[:args.count]:
        manifest = materialize(spec, pool, pins)
        _, checked = cli_json(args.cli, manifest)
        if "result" not in checked:
            continue
        composition = reference.Composition(manifest)
        oracle = composition.oracle(args.max_pairs)
        if not oracle["complete"]:
            continue
        _, table = cli_json(args.cli, manifest, "--joint-table", args.max_pairs)
        if table.get("status") != "explored":
            continue
        ours = canonical_rows_reference(composition, oracle["rows"])
        compared += 1
        honest_differences += len(compare_rows(canonical_rows_rust(table), ours, False))
        differing += bool(compare_rows(canonical_rows_rust(wrong_committed_failure(table)), ours, False))
    print(json.dumps({"compositions_with_the_committed_failure_fixture": compared,
                      "differences_with_the_real_step": honest_differences,
                      "with_differences_under_the_planted_step": differing}))
    return 0 if differing > 0 and honest_differences == 0 else 1


def self_test(args) -> int:
    """With the comparisons stubbed the batch must fail; without, it must pass."""
    base = Path(args.out)
    results = {}
    for stub in (True, False):
        namespace = argparse.Namespace(cli=args.cli, out=str(base / ("stubbed" if stub else "real")),
                                       seed=args.seed, plain=args.plain, wired=args.wired, start=0,
                                       count=args.count, max_pairs=args.max_pairs, stub_comparison=stub)
        results[stub] = run(namespace)
    print(json.dumps({"stubbed_exit": results[True], "real_exit": results[False]}))
    return 0 if results[True] != 0 and results[False] == 0 else 1


def _named_witness(composition: reference.Composition, witness: dict) -> dict:
    """The reference's witness with every state written as `component.field` values."""
    def states(value):
        out = {}
        for name, state in value.items():
            fields = composition.by_name[name].state_names
            values = state.values() if isinstance(state, dict) else state
            out.update({f"{name}.{field}": v for field, v in zip(fields, values)})
        return out
    out = {}
    for key in ("genesis", "states", "after", "providers"):
        if key in witness:
            out[key] = states(witness[key])
    if "request" in witness:
        out["request"] = witness["request"]
    return out


def faults(args) -> int:
    """Each planted fault: the checker's verdict and first witness against the reference's."""
    rows, failures = [], []
    for manifest in sorted((FIXTURES / "faults").glob("*.json")):
        _, checked = cli_json(args.cli, manifest)
        try:
            composition = reference.Composition(manifest)
            ours = composition.obligations()
        except reference.Refused as error:
            ours = {"verdict": "validation-refused", "reason": str(error)}
        if "result" not in checked:
            theirs = {"verdict": "validation-refused"}
        else:
            theirs = {"verdict": checked["result"]["verdict"], "witness": checked["result"]["witness"]}
        row = {"manifest": manifest.name, "checker": theirs["verdict"], "reference": ours["verdict"]}
        agree = row["checker"] == row["reference"]
        if agree and theirs.get("witness"):
            kind = theirs["witness"]["obligation"]
            row["obligation"] = kind
            agree = (ours["first"]["kind"], ours["first"]["subject"]) == (kind["kind"], kind["subject"])
            expected = _named_witness(composition, ours["first"]["witness"])
            got = theirs["witness"]["witness"]
            if kind["kind"] == "local":
                fields = composition.by_name[kind["subject"]]
                expected = {"state": dict(zip(fields.state_names, ours["first"]["witness"]["state"])),
                            "inputs": dict(zip(fields.input_names, ours["first"]["witness"]["inputs"]))}
                got = {"state": got.get("state"), "inputs": got.get("inputs")}
            else:
                got = {key: got.get(key) for key in expected}
            row["witness_agrees"] = got == expected
            agree = agree and row["witness_agrees"]
        rows.append(row)
        if not agree:
            failures.append(manifest.name)
    print(json.dumps({"faults": rows, "disagreements": failures}, indent=1, sort_keys=True))
    return 1 if failures or len(rows) < 8 else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    write = commands.add_parser("fixtures")
    write.add_argument("--cli", required=True)
    write.add_argument("--check", action="store_true")
    write.add_argument("--scratch", default=str(Path(tempfile.gettempdir()) / "composition-domain-fixtures"))
    for name in ("run", "control", "self-test"):
        command = commands.add_parser(name)
        command.add_argument("--cli", required=True)
        command.add_argument("--out", required=True)
        command.add_argument("--seed", type=int, default=SEED)
        command.add_argument("--plain", type=int, default=200)
        command.add_argument("--wired", type=int, default=400)
        command.add_argument("--start", type=int, default=0)
        command.add_argument("--count", type=int, default=0 if name == "run" else 8)
        command.add_argument("--max-pairs", type=int, default=200_000)
        command.add_argument("--stub-comparison", action="store_true")
    planted = commands.add_parser("faults")
    planted.add_argument("--cli", required=True)
    total = commands.add_parser("summary")
    total.add_argument("--out", required=True)
    total.add_argument("--min-complete", type=int, default=600)
    args = parser.parse_args()
    return {"fixtures": fixtures, "run": run, "summary": summary, "control": control,
            "self-test": self_test, "faults": faults}[args.command](args)


if __name__ == "__main__":
    sys.exit(main())
