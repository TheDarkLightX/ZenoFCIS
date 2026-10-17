#!/usr/bin/env python3
"""Fault controls for the bounded loop's doors on every core-component instance.

Instances are generated afresh with ZenoFCIS's own tools (``tools/instantiate_core.py``,
``zeno-fcis contract export-program``, ``zeno-fcis optimize``) in a scratch directory. Then:

* **(a) audit**: in every output of each instance's optimize result, a single-tuple fault is planted at
  the first, the last and one seeded random ordinal. ``transform check`` must report a counterexample at
  exactly that ordinal, and the independent reference (``loop_doors_reference.py``) must see the planted
  program differ there and only there;
* **(b) repair**: four seeded single-position faults per instance in the optimize result, written as
  expressions: a constant off by one, swapped select branches, ``+`` and ``-`` exchanged, ``==`` and ``<``
  exchanged. Each goes through ``loop candidate --expressions``. Reported per fault: whether the program is
  wrong, whether it got a "checked repair", whether the top site covers the fault (the site is the faulty
  position or contains it), and whether the repair is no larger than the program the fault was planted in.
  This is a report, not a threshold;
* **(c) ZAL**: one loop workspace per family receives its first instance's faulted programs; every
  transcript event is re-parsed in both surfaces and must give back its recorded act;
* **(d) MCP**: no answer or say tool; the existing tools' entries equal the baseline file's when one is
  given (``--baseline-mcp``);
* **(e) meaning keys**: per instance, meaning-preserving respellings (commutative operands swapped, a test
  negated with its branches exchanged, a comparison flipped, a test negated twice) and the four planted
  faults of the optimize result's expressions. Each is keyed by ``loop meaning`` and judged against the
  original by ``transform check``; equal keys must coincide exactly with "equivalent". The stubbed
  self-test repeats the comparison with the key comparison replaced by "always equal" and by "never
  equal", and must then find disagreements;
* **(f) memory by meaning**: in each family's workspace, every program the judge refuted is resubmitted
  in each of its respellings. A reply "same" must name a program that ``transform check`` finds
  equivalent (a wrong answer from memory), and a respelling not answered from memory is counted.

Every verdict comes from the CLI or the reference; this tool counts. ``--out DIR`` receives
``controls.json`` and ``controls.md`` with every row.

This tool is LLM-generated.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
from pathlib import Path
import random
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "integrations" / "zal"))
sys.path.insert(0, str(ROOT / "tools"))

import loop_doors  # noqa: E402
import loop_doors_reference as reference  # noqa: E402
import loop_language as Z  # noqa: E402
import loop_workspace as LW  # noqa: E402

FAMILIES = ("reservation-pool", "rate-limiter", "approval-queue", "bounded-counter", "consumable-budget",
            "versioned-register", "idempotency-slot", "retry-budget", "finite-phase-machine", "logical-deadline")
FAULTS = ("constant-off-by-one", "swapped-select-branches", "plus-minus-exchanged", "eq-lt-exchanged")
SEED = 20261012


def run(cli: Path, *arguments) -> tuple[int, dict]:
    done = subprocess.run([str(cli), *map(str, arguments)], capture_output=True, timeout=300)
    try:
        return done.returncode, json.loads(done.stdout or b"{}")
    except ValueError:
        return done.returncode, {"unreadable": done.stdout[-400:].decode(errors="replace")}


def single_manifest(scratch: Path, label: str, program: Path, vocabulary: dict, parameters: dict) -> Path:
    """A one-instance manifest beside a copy of ``program``."""
    folder = scratch / f"{label}-instance"
    folder.mkdir()
    (folder / "program.zcve").write_bytes(program.read_bytes())
    manifest = folder / "manifest.json"
    manifest.write_text(json.dumps({"schema": "zeno-fcis/loop-family-manifest/1", "family": "control",
                                    "vocabulary": vocabulary, "instances": [
                                        {"name": label, "parameters": parameters, "program": "program.zcve"}]}))
    return manifest


def program_json(cli: Path, path: Path, scratch: Path, vocabulary: dict) -> dict:
    """The program in the fixture vocabulary: from a loop session's view, or, where the session's check work
    cannot cover the domain, from the family door judging ``keep`` on every output (it reports the lowered
    program, which is the program itself, and checks it)."""
    session = Path(tempfile.mkdtemp(prefix="open-", dir=scratch))
    session.rmdir()
    code, report = run(cli, "loop", "open", "--original", path, "--session", session, "--profile", "checked-i64-v1")
    if code == 0:
        return report["detail"]["view"]["original"]["program"]
    manifest = single_manifest(scratch, f"view-{path.stem}", path, vocabulary, {})
    for outputs in range(1, 17):
        document = scratch / f"keep-{path.stem}-{outputs}.json"
        document.write_text(json.dumps({"schema": "zeno-fcis/loop-expressions/1", "outputs": ["keep"] * outputs}))
        code, family = run(cli, "loop", "family", "--instances", manifest, "--program", document)
        row = ((family.get("detail") or {}).get("rows") or [{}])[0]
        if row.get("verdict") == "accepted":
            return row["program"]["program"]
    raise SystemExit(f"no fixture view of {path}: {report}")


# --- (a) audit ----------------------------------------------------------------------------------------

def plant_tuple(program: dict, point: tuple, j: int) -> dict:
    """The program with output ``j`` changed at exactly one tuple."""
    nodes = [list(n) for n in program["nodes"]]
    roots = list(program["roots"])

    def add(node):
        nodes.append(node)
        return len(nodes) - 1

    condition = None
    for k, value in enumerate(point):
        source = add(["Input", k])
        if program["inputs"][k]["kind"] == "Bool":
            test = source if value == 1 else add(["Not", source])
        else:
            test = add(["Eq", source, add(["Int", str(value)])])
        condition = test if condition is None else add(["And", condition, test])
    root = roots[j]
    if program["outputs"][j]["kind"] == "Bool":
        wrong = add(["Not", root])
    else:
        domain = program["outputs"][j]
        # off by one, staying inside a bounded domain when there is room
        wrong = add(["Add", root, add(["Int", "1"])])
        if int(domain["max"]) < (1 << 62):
            wrong = add(["Select", add(["Lt", root, add(["Int", domain["max"]])]), wrong,
                         add(["Sub", root, add(["Int", "1"])])])
    roots[j] = add(["Select", condition, wrong, root])
    return {**program, "nodes": nodes, "roots": roots}


def audit(cli: Path, name: str, original: Path, optimized: Path, program: dict, scratch: Path, rng) -> list:
    rows = []
    points = list(reference.tuples(program))
    expected = reference.table(program)
    for j in range(len(program["outputs"])):
        for where, ordinal in (("first", 0), ("last", len(points) - 1), ("random", rng.randrange(len(points)))):
            planted = plant_tuple(program, points[ordinal], j)
            source = scratch / f"{name}-a-{j}-{where}.json"
            target = scratch / f"{name}-a-{j}-{where}.zcve"
            source.write_text(json.dumps(planted))
            code, encoded = run(cli, "loop", "encode", "--program", source, "--out", target)
            if code != 0:
                rows.append({"instance": name, "output": j, "where": where, "ordinal": ordinal,
                             "result": "planted program refused", "detail": encoded})
                continue
            code, checked = run(cli, "transform", "check", "--original", original, "--candidate", target)
            detail = checked.get("detail") or {}
            reported = detail.get("ordinal", (detail.get("counterexample") or {}).get("ordinal"))
            observed = reference.table(planted)
            differing = [t for t, (a, b) in enumerate(zip(expected, observed)) if a != b]
            rows.append({"instance": name, "output": j, "where": where, "ordinal": ordinal,
                         "checker": checked.get("status"), "checker_ordinal": reported,
                         "reference_differs_at": differing,
                         "caught": checked.get("status") == "counterexample" and reported == ordinal
                         and differing == [ordinal]})
    return rows


# --- (b) repair ---------------------------------------------------------------------------------------

def positions(node, path=()):
    out = [(path, node)]
    tag = node[0]
    children = [node[1]] if tag == "not" else list(node[2:]) if tag == "bin" else list(node[1:]) if tag == "if" else []
    for i, child in enumerate(children):
        out.extend(positions(child, path + (i,)))
    return out


def replace(node, path, new):
    if not path:
        return new
    i, rest = path[0], path[1:]
    tag = node[0]
    if tag == "not":
        return ("not", replace(node[1], rest, new))
    if tag == "bin":
        parts = [node[2], node[3]]
        parts[i] = replace(parts[i], rest, new)
        return ("bin", node[1], *parts)
    parts = list(node[1:])
    parts[i] = replace(parts[i], rest, new)
    return ("if", *parts)


def literal_kinds(node, want: str, vocab, path=(), out=None) -> dict:
    """The kind each position takes (an Int literal is off by one only where an integer is wanted)."""
    out = {} if out is None else out
    out[path] = want
    tag = node[0]
    if tag == "not":
        literal_kinds(node[1], "Bool", vocab, path + (0,), out)
    elif tag == "if":
        literal_kinds(node[1], "Bool", vocab, path + (0,), out)
        literal_kinds(node[2], want, vocab, path + (1,), out)
        literal_kinds(node[3], want, vocab, path + (2,), out)
    elif tag == "bin":
        op, a, b = node[1], node[2], node[3]
        if op in Z.ARITH:
            side = "Int"
        elif op in Z.BOOLEAN:
            side = "Bool"
        else:
            side = Z.kind_of(a, vocab) or Z.kind_of(b, vocab) or "Int"
        literal_kinds(a, side, vocab, path + (0,), out)
        literal_kinds(b, side, vocab, path + (1,), out)
    return out


def fault_sites(outputs: list, fault: str, vocab, output_kinds: list) -> list:
    sites = []
    for j, node in enumerate(outputs):
        kinds = literal_kinds(node, output_kinds[j], vocab)
        for path, sub in positions(node):
            if fault == "constant-off-by-one" and sub[0] == "int" and kinds[path] == "Int":
                sites.append((j, path, ("int", sub[1] + 1)))
            elif fault == "swapped-select-branches" and sub[0] == "if":
                sites.append((j, path, ("if", sub[1], sub[3], sub[2])))
            elif fault == "plus-minus-exchanged" and sub[0] == "bin" and sub[1] in ("+", "-"):
                sites.append((j, path, ("bin", "-" if sub[1] == "+" else "+", sub[2], sub[3])))
            elif fault == "eq-lt-exchanged" and sub[0] == "bin" and sub[1] in ("==", "<") \
                    and Z.kind_of(sub[2], vocab) != "Bool" and Z.kind_of(sub[3], vocab) != "Bool":
                sites.append((j, path, ("bin", "<" if sub[1] == "==" else "==", sub[2], sub[3])))
    return sites


def repair_controls(cli: Path, name: str, original: Path, optimized_program: dict, vocabulary: dict,
                    parameters: dict, scratch: Path, rng) -> list:
    outputs = LW.from_program(optimized_program, vocabulary["inputs"])
    kinds = ["Bool" if d["kind"] == "Bool" else "Int" for d in optimized_program["inputs"]]
    vocab = Z.Vocabulary(vocabulary["inputs"], kinds, [d["kind"] for d in optimized_program["outputs"]])
    output_kinds = [d["kind"] for d in optimized_program["outputs"]]
    session = scratch / f"{name}-session"
    code, opened = run(cli, "loop", "open", "--original", original, "--session", session, "--profile",
                       "checked-i64-v1")
    # A session whose check work cannot cover the domain is refused (work-infeasible); the repair then runs
    # through the stateless transfer door (direct, then repair), which reports no sites.
    via_transfer = code != 0
    manifest = single_manifest(scratch, name, original, vocabulary, parameters) if via_transfer else None
    rows = []
    for fault in FAULTS:
        sites = fault_sites(outputs, fault, vocab, output_kinds)
        if not sites:
            rows.append({"instance": name, "fault": fault, "result": "no such position"})
            continue
        j, path, new = rng.choice(sites)
        faulted = list(outputs)
        faulted[j] = replace(outputs[j], path, new)
        texts = [Z.print_symbolic(n) for n in faulted]
        document = scratch / f"{name}-b-{fault}.json"
        document.write_text(json.dumps(loop_doors.document(vocabulary, texts)))
        started = time.monotonic()
        if via_transfer:
            source = scratch / f"{name}-b-{fault}-source.json"
            source.write_text(json.dumps(loop_doors.document(vocabulary, texts, parameters or {"P": 0})))
            code, moved = run(cli, "loop", "transfer", "--from", source, "--to-instances", manifest)
            item = ((moved.get("detail") or {}).get("rows") or [{}])[0]
            direct = (item.get("direct") or {}).get("verdict")
            checked = (item.get("check") or {}).get("verdict") == "equivalent" and \
                (item.get("check") or {}).get("replay") == "matched"
            row = {"instance": name, "fault": fault, "output": j, "path": list(path), "via": "transfer door",
                   "session": opened.get("detail", {}).get("reason"), "seconds": round(time.monotonic() - started, 3),
                   "outcome": "different" if direct == "differs" else direct, "wrong": direct == "differs"}
            if direct == "differs":
                row.update(repaired=item.get("status") == "repair" and checked, proposed=item.get("proposed"),
                           top_site=None, top_covers=None,
                           repair_nodes=(item.get("program") or {}).get("nodes"),
                           planted_in_nodes=len(optimized_program["nodes"]))
                row["no_larger"] = bool(row["repaired"]) and row["repair_nodes"] <= row["planted_in_nodes"]
            rows.append(row)
            continue
        code, report = run(cli, "loop", "candidate", "--session", session, "--expressions", document)
        seconds = round(time.monotonic() - started, 3)
        detail = report.get("detail") or {}
        door = detail.get("door") or {}
        outcome = (detail.get("outcome") or {}).get("outcome")
        row = {"instance": name, "fault": fault, "output": j, "path": list(path), "seconds": seconds,
               "outcome": outcome, "wrong": outcome == "different"}
        if outcome == "different":
            explanation = door.get("explanation") or {}
            repair = explanation.get("repair") or {}
            sites_found = [s for s in explanation.get("sites", []) if s.get("output") == f"out{j}"]
            top = sites_found[0] if sites_found else None
            row.update(repaired=repair.get("label") == "checked repair",
                       proposed=repair.get("proposed"),
                       top_site=top and {"path": top["path"], "kind": top.get("kind"),
                                         "subexpression": top.get("subexpression")},
                       top_covers=bool(top) and tuple(top["path"]) == tuple(path)[:len(top["path"])],
                       repair_nodes=(repair.get("program") or {}).get("nodes"),
                       planted_in_nodes=len(optimized_program["nodes"]))
            row["no_larger"] = row["repaired"] and row["repair_nodes"] <= row["planted_in_nodes"]
            if explanation.get("status"):
                row["explanation"] = explanation.get("status")
        elif outcome is None:
            row["result"] = report.get("status")
        rows.append(row)
    return rows


# --- (c) ZAL and (d) MCP ------------------------------------------------------------------------------

def zal_family(cli: Path, family: str, contract: Path, directory: Path, documents: list) -> dict:
    path = directory / f"{family}.loop.json"
    workspace = LW.LoopWorkspace(path, cli)
    workspace.init(contract=contract)
    ctx = Z.Context(workspace.load()["subject"])
    head = Z.HEADER["symbolic"] + "\n" + Z.subject_line(ctx.subject_ref, False) + "\nauthor model;\n"
    for k, texts in enumerate(documents):
        body = "".join(f"program f{k} out{j} = {Z.print_symbolic(Z.read(t, ctx, False, allow_keep=True))};\n"
                       for j, t in enumerate(texts))
        workspace.submit(head + body, "model")
    state = workspace.load()
    statements = mismatches = 0
    for event in state["events"]:
        for english in (False, True):
            statements += 1
            if Z.parse_statement(event["english" if english else "symbolic"], ctx, english).data() != event["act"]:
                mismatches += 1
    return {"family": family, "events": len(state["events"]), "statements": statements, "mismatches": mismatches}


def mcp_control(baseline: Path | None) -> dict:
    import mcp
    names = [t["name"] for t in mcp.TOOLS]
    out = {"tools": names, "answer_or_say_tool": [n for n in names if "answer" in n or "say" in n or "accept" in n]}
    if baseline:
        spec = importlib.util.spec_from_file_location("baseline_mcp", baseline)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        out["existing_tools_unchanged"] = mcp.TOOLS[:len(module.TOOLS)] == module.TOOLS
    return out


# --- (e) meaning keys and (f) memory by meaning ----------------------------------------------------------

def respellings(node, rng) -> list:
    """Meaning-preserving rewrites of an expression, one per rule, each at a seeded applicable position."""
    rules = {
        "commuted": lambda n: n[0] == "bin" and n[1] in ("==", "!=", "&&", "||", "+")
        and ("bin", n[1], n[3], n[2]),
        "negated-test": lambda n: n[0] == "if" and ("if", ("not", n[1]), n[3], n[2]),
        "flipped-comparison": lambda n: n[0] == "bin" and n[1] in ("<", "<=")
        and ("bin", {"<": ">", "<=": ">="}[n[1]], n[3], n[2]),
        "double-negation": lambda n: n[0] == "if" and ("if", ("not", ("not", n[1])), n[2], n[3]),
    }
    out = []
    for rule, rewrite in rules.items():
        sites = [(path, rewrite(sub)) for path, sub in positions(node) if rewrite(sub)]
        if sites:
            path, new = rng.choice(sites)
            out.append((rule, replace(node, path, new)))
    return out


def encoded(cli: Path, program: dict, path: Path) -> Path:
    source = path.with_suffix(".json")
    source.write_text(json.dumps(program))
    code, report = run(cli, "loop", "encode", "--program", source, "--out", path)
    if code != 0:
        raise SystemExit(f"loop encode failed: {report}")
    return path


def keyed(cli: Path, original: Path, document: Path) -> dict:
    code, report = run(cli, "loop", "meaning", "--original", original, "--expressions", document)
    return report.get("detail") or {} if code == 0 and report.get("status") == "keyed" else {"refused": report}


def checked_equivalent(cli: Path, a: Path, b: Path) -> bool | None:
    code, report = run(cli, "transform", "check", "--original", a, "--candidate", b)
    if code == 0:
        return True
    return False if report.get("status") == "counterexample" else None


def meaning_controls(cli: Path, name: str, original: Path, optimized_program: dict, vocabulary: dict,
                     scratch: Path, rng) -> list:
    outputs = LW.from_program(optimized_program, vocabulary["inputs"])
    kinds = ["Bool" if d["kind"] == "Bool" else "Int" for d in optimized_program["inputs"]]
    vocab = Z.Vocabulary(vocabulary["inputs"], kinds, [d["kind"] for d in optimized_program["outputs"]])
    output_kinds = [d["kind"] for d in optimized_program["outputs"]]
    variants = []
    for j, node in enumerate(outputs):
        for rule, new in respellings(node, rng):
            variants.append((f"respelled-{rule}", j, new))
    for fault in FAULTS:
        sites = fault_sites(outputs, fault, vocab, output_kinds)
        if sites:
            j, path, new = rng.choice(sites)
            variants.append((f"planted-{fault}", j, replace(outputs[j], path, new)))
    rows = []
    for k, (kind, j, new) in enumerate(variants):
        texts = [Z.print_symbolic(n) for n in outputs]
        texts[j] = Z.print_symbolic(new)
        document = scratch / f"{name}-e-{k}.json"
        document.write_text(json.dumps(loop_doors.document(vocabulary, texts)))
        detail = keyed(cli, original, document)
        row = {"instance": name, "variant": kind, "output": j}
        if "refused" in detail:
            row["result"] = "not keyed"
            row["reason"] = (detail["refused"].get("detail") or {}).get("reason") or detail["refused"].get("status")
            rows.append(row)
            continue
        candidate = encoded(cli, detail["program"]["program"], scratch / f"{name}-e-{k}.zcve")
        row.update(same_key=detail["same_as_original"], equivalent=checked_equivalent(cli, original, candidate))
        row["agrees"] = row["equivalent"] is not None and row["same_key"] == row["equivalent"]
        rows.append(row)
    return rows


def stubbed_meaning(rows: list) -> dict:
    """The comparison with the key comparison stubbed out must disagree with the checker somewhere."""
    judged = [r for r in rows if r.get("equivalent") is not None]
    out = {}
    for stub, value in (("always-equal", True), ("never-equal", False)):
        out[stub] = sum(1 for r in judged if value != r["equivalent"])
    out["self_test"] = "passed" if judged and all(n > 0 for n in out.values()) else "failed"
    return out


def memory_control(cli: Path, workspace_path: Path, scratch: Path, rng) -> list:
    """Every refuted program, resubmitted in each respelling: answered from memory only when equivalent."""
    workspace = LW.LoopWorkspace(workspace_path, cli)
    state = workspace.load()
    ctx = Z.Context(state["subject"])
    head = Z.HEADER["symbolic"] + "\n" + Z.subject_line(ctx.subject_ref, False) + "\nauthor model;\n"
    programs = {}
    for event in state["events"]:
        act = event["act"]
        if act["kind"] == "program":
            programs.setdefault(act["id"], {})[act["output"]] = event
    refuted = {e["act"]["id"] for e in state["events"] if e["act"]["kind"] == "refuted"}
    rows = []
    count = 0
    for pid in sorted(refuted):
        acts = programs[pid]
        given = {out: Z.parse_statement(ev["symbolic"], ctx, False).get("expr") for out, ev in acts.items()}
        for out, node in sorted(given.items()):
            for rule, new in respellings(node, rng):
                count += 1
                rid = f"{pid}_r{count}"[:32]
                body = "".join(f"program {rid} {o} = {Z.print_symbolic(new if o == out else n)};\n"
                               for o, n in sorted(given.items()))
                events = workspace.submit(head + body, "model")
                same = [e for e in events if e["act"]["kind"] == "same"]
                row = {"workspace": workspace_path.stem, "earlier": pid, "respelling": rule, "output": out,
                       "answered_from_memory": bool(same)}
                if same:
                    named = same[0]["act"]["as"]
                    row["names"] = named
                    documents = sorted((workspace.data / "documents").glob("*.json"))
                    first = next(d for d in documents if d.name.endswith(f"-program-{named}.json"))
                    earlier = json.loads(first.read_text())
                    texts = list(earlier["outputs"])
                    texts[int(out[3:])] = Z.print_symbolic(new)
                    again = scratch / f"{rid}.json"
                    again.write_text(json.dumps(loop_doors.document(earlier["vocabulary"], texts)))
                    original = workspace.data / state.get("original_program", "original.zcve")
                    a, b = keyed(cli, original, first), keyed(cli, original, again)
                    if "refused" in a or "refused" in b:
                        row["equivalent"] = None
                    else:
                        pa = encoded(cli, a["program"]["program"], scratch / f"{rid}-a.zcve")
                        pb = encoded(cli, b["program"]["program"], scratch / f"{rid}-b.zcve")
                        row["equivalent"] = checked_equivalent(cli, pa, pb)
                    row["wrong"] = row["equivalent"] is not True
                rows.append(row)
    return rows


# --- driver --------------------------------------------------------------------------------------------

def markdown(report: dict) -> str:
    lines = ["# Loop door fault controls", "", f"CLI `{report['cli']}`; seed {SEED}.", "", "## (a) Audit", "",
             "| Instance | Output | Where | Ordinal | Checker | Checker ordinal | Reference differs at | Caught |",
             "|---|---:|---|---:|---|---:|---|---|"]
    for r in report["audit"]:
        lines.append(f"| {r['instance']} | out{r['output']} | {r['where']} | {r['ordinal']} | {r.get('checker')} | "
                     f"{r.get('checker_ordinal')} | {r.get('reference_differs_at')} | {r.get('caught')} |")
    s = report["summary"]
    lines += ["", f"Caught {s['audit_caught']} of {s['audit_planted']}.", "", "## (b) Repair", "",
              "| Instance | Fault | Output | Path | Wrong | Repaired | Top site | Covers | Repair nodes | Planted in | No larger |",
              "|---|---|---:|---|---|---|---|---|---:|---:|---|"]
    for r in report["repair"]:
        top = r.get("top_site") or {"path": "(transfer door: no sites)" if r.get("via") else ""}
        lines.append(f"| {r['instance']} | {r['fault']} | {r.get('output', '')} | {r.get('path', r.get('result', ''))} | "
                     f"{r.get('wrong', '')} | {r.get('repaired', '')} | {top.get('path', '')} {top.get('kind', '')} | "
                     f"{r.get('top_covers', '')} | {r.get('repair_nodes', '')} | {r.get('planted_in_nodes', '')} | "
                     f"{r.get('no_larger', '')} |")
    lines += ["", "| Measure | Count |", "|---|---:|"]
    for key in ("repair_planted", "repair_no_position", "repair_wrong", "repair_repaired", "repair_top_covers",
                "repair_no_larger", "repair_via_transfer_door"):
        lines.append(f"| {key.replace('_', ' ')} | {s[key]} |")
    lines += ["", "## (c) ZAL", "", "| Family | Events | Statements re-parsed | Mismatches |", "|---|---:|---:|---:|"]
    for r in report["zal"]:
        lines.append(f"| {r['family']} | {r['events']} | {r['statements']} | {r['mismatches']} |")
    lines += ["", "## (d) MCP", "", f"```\n{json.dumps(report['mcp'], indent=1)}\n```", ""]
    lines += ["## (e) Meaning keys", "", "| Instance | Variant | Output | Same key | Transform check equivalent | Agrees |",
              "|---|---|---:|---|---|---|"]
    for r in report["meaning"]:
        lines.append(f"| {r['instance']} | {r['variant']} | out{r['output']} | {r.get('same_key', r.get('result'))} | "
                     f"{r.get('equivalent', r.get('reason', ''))} | {r.get('agrees', '')} |")
    lines += ["", f"Stubbed self-test: `{json.dumps(report['meaning_stubbed'])}`", "",
              "## (f) Memory by meaning", "",
              "| Workspace | Earlier | Respelling | Output | From memory | Names | Equivalent | Wrong |",
              "|---|---|---|---|---|---|---|---|"]
    for r in report["memory"]:
        lines.append(f"| {r['workspace']} | {r['earlier']} | {r['respelling']} | {r['output']} | "
                     f"{r['answered_from_memory']} | {r.get('names', '')} | {r.get('equivalent', '')} | "
                     f"{r.get('wrong', '')} |")
    lines += ["", "| Measure | Count |", "|---|---:|"]
    for key in ("meaning_variants", "meaning_not_keyed", "meaning_agree", "meaning_disagree",
                "memory_resubmitted", "memory_answered", "memory_wrong", "memory_missed"):
        lines.append(f"| {key.replace('_', ' ')} | {s[key]} |")
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--cli", type=Path)
    parser.add_argument("--work", type=Path, required=True, help="new scratch directory outside the source tree")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--family", action="append", choices=FAMILIES)
    parser.add_argument("--baseline-mcp", type=Path)
    options = parser.parse_args()
    cli = loop_doors.resolve_cli(options.cli)
    os.environ["ZENO_FCIS_CLI"] = str(cli)
    options.work.mkdir(parents=True, exist_ok=False)
    rng = random.Random(SEED)
    report = {"cli": str(cli), "seed": SEED, "audit": [], "repair": [], "zal": [], "meaning": [], "memory": [],
              "authority": "none"}
    for family in options.family or FAMILIES:
        directory = options.work / family
        manifest = loop_doors.generate_family(cli, family, directory,
                                              lambda contract: LW.vocabulary(LW.fields_of(contract)))
        listed = json.loads(manifest.read_text())
        vocabulary = listed["vocabulary"]
        documents = []
        for entry in listed["instances"]:
            name = entry["name"]
            original = directory / entry["program"]
            optimized = directory / entry["optimize"] if "optimize" in entry else original
            scratch = options.work / "scratch" / name
            scratch.mkdir(parents=True)
            program = program_json(cli, optimized, scratch, vocabulary)
            report["audit"] += audit(cli, name, original, optimized, program, scratch, rng)
            rows = repair_controls(cli, name, original, program, vocabulary, entry["parameters"], scratch, rng)
            report["repair"] += rows
            report["meaning"] += meaning_controls(cli, name, original, program, vocabulary, scratch, rng)
            if not documents:
                documents = [json.loads((scratch / f"{name}-b-{r['fault']}.json").read_text())["outputs"]
                             for r in rows if "output" in r]
                first = entry
        zal_dir = options.work / "zal"
        zal_dir.mkdir(exist_ok=True)
        report["zal"].append(zal_family(cli, family, directory / first["name"] / "contract", zal_dir, documents))
        memory_scratch = options.work / "scratch" / f"{family}-memory"
        memory_scratch.mkdir(parents=True)
        report["memory"] += memory_control(cli, zal_dir / f"{family}.loop.json", memory_scratch, rng)
        print(f"{family}: done", file=sys.stderr, flush=True)
    report["mcp"] = mcp_control(options.baseline_mcp)
    planted = [r for r in report["repair"] if "output" in r]
    report["summary"] = {
        "instances": len({r["instance"] for r in report["audit"]}),
        "audit_planted": len(report["audit"]), "audit_caught": sum(1 for r in report["audit"] if r.get("caught")),
        "repair_planted": len(planted),
        "repair_no_position": sum(1 for r in report["repair"] if r.get("result") == "no such position"),
        "repair_wrong": sum(1 for r in planted if r.get("wrong")),
        "repair_repaired": sum(1 for r in planted if r.get("repaired")),
        "repair_top_covers": sum(1 for r in planted if r.get("top_covers")),
        "repair_via_transfer_door": sum(1 for r in planted if r.get("via") == "transfer door"),
        "repair_no_larger": sum(1 for r in planted if r.get("no_larger")),
        "zal_statements": sum(r["statements"] for r in report["zal"]),
        "zal_mismatches": sum(r["mismatches"] for r in report["zal"]),
        "meaning_variants": len(report["meaning"]),
        "meaning_not_keyed": sum(1 for r in report["meaning"] if r.get("result") == "not keyed"),
        "meaning_agree": sum(1 for r in report["meaning"] if r.get("agrees")),
        "meaning_disagree": sum(1 for r in report["meaning"] if "agrees" in r and not r["agrees"]),
        "memory_resubmitted": len(report["memory"]),
        "memory_answered": sum(1 for r in report["memory"] if r["answered_from_memory"]),
        "memory_wrong": sum(1 for r in report["memory"] if r.get("wrong")),
        "memory_missed": sum(1 for r in report["memory"] if not r["answered_from_memory"]),
    }
    report["meaning_stubbed"] = stubbed_meaning(report["meaning"])
    options.out.mkdir(parents=True, exist_ok=True)
    (options.out / "controls.json").write_text(json.dumps(report, indent=1, sort_keys=True) + "\n")
    (options.out / "controls.md").write_text(markdown(report))
    print(json.dumps(report["summary"], indent=1))
    s = report["summary"]
    ok = (s["audit_caught"] == s["audit_planted"] and s["zal_mismatches"] == 0
          and not report["mcp"]["answer_or_say_tool"] and report["mcp"].get("existing_tools_unchanged", True)
          and s["meaning_disagree"] == 0 and s["memory_wrong"] == 0
          and report["meaning_stubbed"]["self_test"] == "passed")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
