#!/usr/bin/env python3
"""The Spot native-to-contract refinement gate.

Checks, in order, and writes one JSON report plus logs to a new directory:

1. freshness: the Verus program description and evaluator type copy match
   src/v2_contract.rs and evaluation/mod.rs (gen_refinement_program --check);
2. trusted-declaration scan of the Verus and Lean proof sources;
3. Verus: the pinned verifier proves verification/verus/spot_refinement.rs as
   a whole crate with --no-cheating, the reviewed obligation count and every
   required theorem;
4. Lean: NativeBridge.lean checks with the pinned Lean 4.30.0 and its four
   theorems depend on exactly propext and Quot.sound;
5. replay (with --packets): recorded native probe decisions equal the
   evaluator-specification rendering of the parsed program;
6. finite: every point of refinement_contract's declared domain agrees with
   the independent oracle and meets the executable contracts;
7. planted defects: each mutated program fails the Verus proof and the
   finite check.

Evidence only; it grants no decision, publication or release authority.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

import gen_refinement_program
import native_program
import planted_defects
import refinement_contract

APP = native_program.APP
ROOT = APP.parents[1]
sys.path.insert(0, str(ROOT / "tools"))
import check_verus  # noqa: E402

HARNESS = Path("verification/verus/spot_refinement.rs")
PROOFS = [Path(f"verification/verus/{name}") for name in (
    "spot_refinement.rs", "spot_value.rs", "spot_evaluation.rs", "spot_evaluator_types.rs",
    "spot_refinement_base.rs", "spot_native_program.rs", "spot_refinement_proof.rs")]
PRODUCTION = [Path("crates/zeno-fcis-synthesis/src/finite") / name for name in (
    "evaluation/spec.rs", "value/scalar.rs", "value/wide.rs")]
LEAN = APP / "proofs/NativeBridge.lean"
TOOLING = ("native_program.py", "native_terms.py", "gen_refinement_program.py", "refinement_contract.py",
           "planted_defects.py", "check_refinement.py", "spot_reference.py")
EXPECTED_VERIFIED = 1504
REQUIRED = ("spot_refinement_proof::spot_refinement", "spot_refinement_proof::exact_meets_contract",
            "spot_refinement_proof::accepted_terms", "spot_refinement_proof::coalition",
            "spot_native_program::program_terms", "spot_native_program::values_are_terms",
            "spot_refinement_base::prefix_evaluated", "spot_refinement_base::projection_values")
LEAN_THEOREMS = ("step_product", "trace_invariants", "closed_no_profit", "creation_no_profit")
VERUS_FORBIDDEN = ("assume(", "admit(", "external_body", "verifier::external", "verifier(external",
                   "assume_specification", "unsafe fn", "unsafe {", "unsafe impl")
LEAN_FORBIDDEN = ("sorry", "admit", "native_decide", "Classical", "grind", "axiom ", "implemented_by",
                  "unsafe", "@[extern")


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command, cwd, log: Path, environment=None, timeout=1800):
    started = time.monotonic()
    result = subprocess.run(command, cwd=cwd, env=environment, text=True, capture_output=True,
                            timeout=timeout, check=False)
    log.mkdir(parents=True, exist_ok=True)
    (log / "stdout").write_text(result.stdout)
    (log / "stderr").write_text(result.stderr)
    record = {"command": [str(c) for c in command], "cwd": str(cwd), "exit": result.returncode,
              "seconds": round(time.monotonic() - started, 2)}
    (log / "command.json").write_text(json.dumps(record, indent=2) + "\n")
    return result, record


def scan() -> dict:
    found = {}
    for path in PROOFS:
        text = (ROOT / path).read_text()
        hits = [token for token in VERUS_FORBIDDEN if token in text]
        if hits:
            found[str(path)] = hits
    text = re.sub(r"/-.*?-/", "", LEAN.read_text(), flags=re.S)
    text = re.sub(r"--[^\n]*", "", text)
    hits = [token for token in LEAN_FORBIDDEN if token in text]
    if hits:
        found[str(LEAN.relative_to(ROOT))] = hits
    if found:
        raise RuntimeError(f"trusted or excluded declarations in proof sources: {found}")
    return {"verus_tokens": VERUS_FORBIDDEN, "lean_tokens": LEAN_FORBIDDEN, "found": {}}


def verus(tools: Path, tree: Path, log: Path, threads: str):
    environment = {k: v for k, v in os.environ.items()
                   if not k.startswith(("VERUS_", "VARGO_")) and k not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")}
    pin = json.loads((ROOT / check_verus.PIN).read_text())
    environment.update(RUSTUP_TOOLCHAIN=pin["rust_toolchain"], VERUS_Z3_PATH=str(tools / "z3"),
                       RUST_MIN_STACK="268435456")
    command = [str(tools / "verus"), "--crate-type=lib", "--edition=2024", "--no-cheating",
               "--no-external-by-default", "--num-threads", threads, "--output-json", str(HARNESS)]
    result, record = run(command, tree, log, environment)
    try:
        report = json.loads(result.stdout)
    except json.JSONDecodeError:
        report = {}
    outcome = report.get("verification-results", {})
    errors = sorted(set(re.findall(r"--> (verification/verus/[\w.]+:\d+)", result.stderr)))
    return report, outcome, record, errors


def positive(tools: Path, out: Path, threads: str) -> dict:
    pin = json.loads((ROOT / check_verus.PIN).read_text())
    report, outcome, record, errors = verus(tools, ROOT, out / "verus-positive", threads)
    functions = report.get("func-details", {})
    missing = [name for name in REQUIRED if f"spot_refinement::{name}" not in functions]
    tool = report.get("verus", {})
    ok = (record["exit"] == 0 and outcome.get("success") is True and outcome.get("errors") == 0
          and outcome.get("verified") == EXPECTED_VERIFIED and outcome.get("is-verifying-entire-crate") is True
          and not missing and tool.get("version") == pin["version"] and tool.get("commit") == pin["commit"])
    result = {"outcome": outcome, "verus": tool, "missing": missing, "errors": errors,
              "seconds": record["seconds"], "expected_verified": EXPECTED_VERIFIED}
    if not ok:
        raise RuntimeError(f"Verus refinement proof did not pass: {result}")
    return result


def lean(root: Path, out: Path) -> dict:
    binary = root / "bin/lean"
    version, _ = run([binary, "--version"], ROOT, out / "lean-version", timeout=60)
    if "4.30.0" not in version.stdout:
        raise RuntimeError(f"wrong Lean pin: {version.stdout.strip()}")
    result, record = run([binary, "-t", "0", LEAN], ROOT, out / "lean-bridge", timeout=900)
    wanted = [f"'SpotNative.{name}' depends on axioms: [propext, Quot.sound]" for name in LEAN_THEOREMS]
    if record["exit"] != 0 or result.stdout.split("\n")[:-1] != wanted or result.stderr.strip():
        raise RuntimeError(f"Lean bridge failed or changed axioms:\n{result.stdout}\n{result.stderr}")
    return {"version": version.stdout.strip(), "binary_sha256": digest(binary), "axioms": wanted,
            "source_sha256": digest(LEAN), "seconds": record["seconds"]}


def replay(packets: Path, native) -> dict:
    states = [field for source, field in native.bindings if source == "State"]
    matched = accepts = 0
    for path in sorted(packets.glob("*.json")):
        if path.name.endswith(".command.json"):
            continue
        example = json.loads(path.read_text())["result"]["authority"]["example"]
        sections = example.split("|")
        tokens = sections[-2].split()
        accept, reason, post = native_program.decide(native, list(map(int, sections[0].split())))
        wanted = (tokens[0], None if tokens[1] == "-" else int(tokens[1]), list(map(int, tokens[2:])))
        got = ("accept" if accept else "reject", reason, [post[f] for f in states])
        if got != wanted:
            raise RuntimeError(f"native packet {path.name} differs from the parsed program")
        matched += 1
        accepts += accept
    if matched == 0:
        raise RuntimeError("no native packets to replay")
    return {"packets": matched, "accepts": accepts, "rejects": matched - accepts, "directory": str(packets)}


def probe_sample(native, cli: Path, out: Path, per_class: int = 8) -> dict:
    """Fresh native decisions on a stratified sample of the finite domain:
    up to `per_class` points per (command, reference verdict), each run by the
    library CLI on this tree's application and compared with the parsed
    program's evaluator-specification rendering."""
    from concurrent.futures import ThreadPoolExecutor
    chosen, seen = [], {}
    for world, command in refinement_contract.points():
        kind = (command.action, refinement_contract.step(world, command).reason)
        if seen.get(kind, 0) < per_class:
            seen[kind] = seen.get(kind, 0) + 1
            chosen.append(refinement_contract.world_inputs(world, command))
    folder = out / "native-probes"
    folder.mkdir()
    states = [field for source, field in native.bindings if source == "State"]

    def one(item):
        index, inputs = item
        packet = folder / f"{index:04d}.json"
        command = [str(cli), "--no-kb", "contract", "probe", "explain", str(APP), "--input",
                   " ".join(map(str, inputs)), "--out", str(packet)]
        result = subprocess.run(command, capture_output=True, text=True, timeout=120, check=False)
        if result.returncode != 0:
            raise RuntimeError(f"probe {index} exit {result.returncode}: {result.stderr}")
        example = json.loads(packet.read_text())["result"]["authority"]["example"]
        sections = example.split("|")
        tokens = sections[-2].split()
        wanted = (tokens[0], None if tokens[1] == "-" else int(tokens[1]), list(map(int, tokens[2:])))
        accept, reason, post = native_program.decide(native, inputs)
        got = ("accept" if accept else "reject", reason, [post[f] for f in states])
        return got == wanted, accept

    with ThreadPoolExecutor(max_workers=3) as pool:
        results = list(pool.map(one, enumerate(chosen)))
    if not all(ok for ok, _ in results):
        bad = [i for i, (ok, _) in enumerate(results) if not ok]
        raise RuntimeError(f"fresh native probes differ from the parsed program: {bad[:20]}")
    return {"probes": len(results), "accepts": sum(a for _, a in results), "classes": len(seen),
            "cli": str(cli), "cli_sha256": digest(cli), "per_class": per_class}


def mutant_tree(base: Path, text: str) -> Path:
    for path in PROOFS + PRODUCTION:
        (base / path).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / path, base / path)
    native = native_program.load_text(text)
    rendered = gen_refinement_program.render(native, hashlib.sha256(text.encode()).hexdigest())
    (base / "verification/verus/spot_native_program.rs").write_text(rendered)
    return base


def defects(tools: Path, out: Path, threads: str, scratch: Path) -> dict:
    text = native_program.SOURCE.read_text()
    results = {}
    for name, changed in planted_defects.applied(text).items():
        entry = {"defect": planted_defects.DEFECTS[name][1]}
        tree = scratch / name
        try:
            mutant_tree(tree, changed)
            _, outcome, record, errors = verus(tools, tree, out / f"defect-{name}", threads)
            entry["verus"] = {"exit": record["exit"], "outcome": outcome, "failing": errors,
                              "rejected": record["exit"] != 0 and outcome.get("success") is not True}
        except ValueError as error:
            entry["verus"] = {"rejected": True, "generation_refused": str(error)}
        finite = refinement_contract.finite_check(native_program.load_text(changed), examples=1)
        entry["finite"] = {"failures": sorted(finite["failures"]), "counts": finite["counts"],
                           "first": next(iter(finite["failures"].values()), [None])[0],
                           "rejected": bool(finite["failures"])}
        entry["caught"] = entry["verus"]["rejected"] and entry["finite"]["rejected"]
        results[name] = entry
        shutil.rmtree(tree, ignore_errors=True)
    if not all(entry["caught"] for entry in results.values()):
        raise RuntimeError(f"a planted defect was not rejected: {json.dumps(results)[:4000]}")
    return results


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--lean-root", type=Path, required=True)
    parser.add_argument("--verus-cache", type=Path, default=check_verus.cache_directory())
    parser.add_argument("--packets", type=Path)
    parser.add_argument("--cli", type=Path, help="library CLI for fresh native probes")
    parser.add_argument("--threads", default="3")
    parser.add_argument("--skip-defects", action="store_true")
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    pin = json.loads((ROOT / check_verus.PIN).read_text())
    sources = {str(p): digest(ROOT / p) for p in PROOFS + PRODUCTION}
    sources.update({str(native_program.SOURCE.relative_to(ROOT)): digest(native_program.SOURCE),
                    str(LEAN.relative_to(ROOT)): digest(LEAN)})
    for name in TOOLING:
        sources[f"core-apps/zenodex-spot/tools/{name}"] = digest(APP / "tools" / name)
    report = {"schema": "zenodex-spot/refinement-check/1", "authority": "none", "sources": sources}
    native = native_program.load()
    if gen_refinement_program.stale():
        raise RuntimeError(f"generated Verus sources are stale: {gen_refinement_program.stale()}")
    report["freshness"] = "fresh"
    report["scan"] = scan()
    tools = check_verus.prepare_tools(args.verus_cache, pin, False)
    report["verus"] = positive(tools, out, args.threads)
    report["lean"] = lean(args.lean_root, out)
    if args.packets:
        report["replay"] = replay(args.packets, native)
    if args.cli:
        report["fresh_native"] = probe_sample(native, args.cli.resolve(), out)
    finite = refinement_contract.finite_check(native)
    if finite["failures"]:
        raise RuntimeError(f"finite check failed: {json.dumps(finite['failures'])[:4000]}")
    report["finite"] = {"label": "finite checked over the declared domain", **finite}
    if not args.skip_defects:
        report["defects"] = defects(tools, out, args.threads, out / "mutants")
    check_verus.verify_tool_files(tools, pin)
    after = {str(p): digest(ROOT / p) for p in PROOFS + PRODUCTION}
    if any(sources[k] != v for k, v in after.items()):
        raise RuntimeError("sources changed during the check")
    report["status"] = "passed"
    (out / "report.json").write_text(json.dumps(report, indent=2, default=list) + "\n")
    print(json.dumps({"status": "passed", "verified": report["verus"]["outcome"]["verified"],
                      "finite_points": finite["counts"]["points"], "defects": sorted(report.get("defects", {})),
                      "report": str(out / "report.json")}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
