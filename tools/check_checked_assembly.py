#!/usr/bin/env python3
"""Check the checked-assembly proofs with the pinned verifier.

The positive proof is unfiltered: it verifies the harness's three proof
modules together with the production closure of the evaluator specification
the route lemma is stated against. Three planted controls each change one
premise or constructor and must fail the intended obligation in a run
restricted to the proof modules. The receipt is scoped Verus/Z3 evidence, not
an authorization or a release.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import tempfile
import zipfile

import check_verus as verifier
from v2_proof_sources import execution_sources
from verus_coverage import inventory

ROOT = Path(__file__).resolve().parents[1]
PROFILE = Path("verification/verus/checked-assembly.json")
HARNESS = Path("verification/verus/checked_assembly.rs")
JOINT = Path("verification/verus/checked_assembly_joint.rs")
TABLES = Path("verification/verus/checked_assembly_tables.rs")
ROUTE = Path("verification/verus/checked_assembly_route.rs")
PROOFS = (JOINT, TABLES, ROUTE)
MODULES = ("joint", "tables", "route")
NAMESPACES = tuple(f"checked_assembly::{module}::" for module in MODULES)
SPEC = Path("crates/zeno-fcis-synthesis/src/finite/evaluation/spec.rs")
# The harness root and the proof modules hold no trusted declarations of
# their own.
FORBIDDEN = re.compile(r"\bassume\b|\badmit\b|external|verifier::|\bunsafe\b")
# What an application-level claim rests on that is tested, not proved: the
# composition result's `TRUSTED_BASE`, word for word.
SHARED_TRUSTED_BASE = (
    "The checker's exhaustive enumeration of each leaf's local, StateFact and interface tables with the leaf's bound Authority.",
    "The group checks of equation 2.8: their tables and the discharge and rise of each child's assumption.",
    "The assembly validator's canonical terms, route specialization and case pairing, compared with the library evaluator on the register ring and chain and constructor by constructor in unit tests.",
    "The assembly validator's comparison of classes, reasons, assignments and frames, its field, genesis and routing checks, and its law-implication tables.",
    "The interval analysis showing that no eager node can trap, and the meter check with its cost model of one Step per program or law node.",
    "The link from the bound Authorities' actual decisions (case selection, assignments, law and meter refusals) to the abstract decision functions of the Verus theorem.",
)

# name: (file, anchor, replacement, function whose obligation must fail)
CONTROLS = {
    # A leaf may name itself as its own provider: circular self-support.
    "self_provider": (
        TABLES,
        "0 <= #[trigger] providers(k)[i] < n && providers(k)[i] != k",
        "0 <= #[trigger] providers(k)[i] < n",
        "assumption_from_peers",
    ),
    # The aggregate need only give the selected slot the leaf's successor;
    # it may write every unselected slot.
    "unframed_aggregate": (
        TABLES,
        """        #[trigger] agg(pre, k, c) == match decide(k, pre[k], observation(k, pre, providers, view), c) {
            Outcome::Accept(t) => Aggregate::Commit(pre.update(k, t)),
            Outcome::Reject => Aggregate::Reject,
            Outcome::Refuse => Aggregate::Refuse,
        }""",
        """        match decide(k, pre[k], observation(k, pre, providers, view), c) {
            Outcome::Accept(t) => match #[trigger] agg(pre, k, c) {
                Aggregate::Commit(post) => post.len() == pre.len() && post[k] == t,
                _ => false,
            },
            Outcome::Reject => agg(pre, k, c) == Aggregate::Reject,
            Outcome::Refuse => agg(pre, k, c) == Aggregate::Refuse,
        }""",
        "aggregate_step_is_routed",
    ),
    # `true && x = x` for any `x`, not only a Boolean one.
    "unguarded_conjunction": (
        ROUTE,
        "} else if a == Term::Bool(true) && boolean(b, bools) {",
        "} else if a == Term::Bool(true) {",
        "constructors_preserve_values",
    ),
}


def source_paths() -> tuple[Path, ...]:
    paths = [
        *execution_sources(ROOT), HARNESS, *PROOFS, PROFILE, verifier.PIN, SPEC,
        Path("crates/zeno-fcis-core/src/resource.rs"),
        Path("verification/verus/authority_v2_sources.json"),
        Path("verification/verus/authority_v2_test_sources.json"),
        Path("tools/check_checked_assembly.py"), Path("tools/test_check_checked_assembly.py"),
        Path("tools/check_verus.py"), Path("tools/verus_coverage.py"),
        Path("tools/v2_proof_sources.py"), Path("Cargo.toml"), Path("rust-toolchain.toml"),
    ]
    # Every module file of the finite closure the harness compiles, including
    # any the registered source manifest does not list.
    paths.extend(p.relative_to(ROOT) for p in ROOT.glob("crates/zeno-fcis-synthesis/src/finite/**/*.rs"))
    return tuple(sorted(set(paths)))


def snapshot() -> dict[str, str]:
    result = {}
    for relative in source_paths():
        path = ROOT / relative
        if not path.is_file() or any(p.is_symlink() for p in (path, *path.parents)):
            raise RuntimeError(f"missing or symbolic-link proof source: {relative}")
        result[str(relative)] = verifier.digest(path)
    return result


def forbidden_tokens(sources: dict[Path, str]) -> list[str]:
    """Every trusted-declaration token in the proof modules, by place."""
    found = []
    for path, source in sources.items():
        for number, line in enumerate(source.splitlines(), 1):
            if FORBIDDEN.search(line):
                found.append(f"{path}:{number}")
    return found


def function_region(source: str, name: str) -> tuple[int, int]:
    """Lines from `fn name` to the line before the next function."""
    functions = list(re.finditer(r"\bfn\s+([A-Za-z_][A-Za-z_0-9]*)\b", source))
    matches = [index for index, match in enumerate(functions) if match[1] == name]
    if len(matches) != 1:
        raise ValueError(f"not one intended function: {name}")
    index = matches[0]
    start = source.count("\n", 0, functions[index].start()) + 1
    end = source.count("\n") + 1
    if index + 1 < len(functions):
        end = source.count("\n", 0, functions[index + 1].start())
    return start, end


def control_sources(sources: dict[Path, str]) -> dict[str, tuple[Path, str, str]]:
    """Each control's changed file, its new text and its intended function."""
    controls = {}
    for name, (path, anchor, replacement, function) in CONTROLS.items():
        source = sources[path]
        if source.count(anchor) != 1:
            raise RuntimeError(f"control anchor changed: {name}")
        controls[name] = (path, source.replace(anchor, replacement), function)
    return controls


def positive_accepted(process, report: dict, pin: dict, profile: dict) -> bool:
    report_pin = {**pin, "expected_verified": profile["expected_verified"],
                  "target_functions": profile["target_functions"]}
    return (process.returncode == 0
            and verifier.proof_succeeded(report, pin)
            and verifier.accepted(report, report_pin)
            and not verifier.ERROR_HEADER.search(process.stderr)
            and not verifier.NON_SEMANTIC.search(process.stderr))


def targeted_refusal(process, report: dict, pin: dict, profile: dict, path: Path,
                     specimen: Path, region: tuple[int, int]) -> bool:
    """One intended semantic failure in the restricted run, nothing else."""
    if not isinstance(report, dict):
        return False
    result, tool = report.get("verification-results", {}), report.get("verus", {})
    if not isinstance(result, dict) or not isinstance(tool, dict):
        return False
    if (process.returncode != 1 or tool.get("version") != pin["version"]
            or tool.get("commit") != pin["commit"]
            or str(tool.get("toolchain", "")).split(" ")[0] != pin["rust_toolchain"]
            # A restricted run's report has no overall `success`.
            or ("success" in result and result["success"] is not False)
            or result.get("encountered-error") is not True
            or result.get("encountered-vir-error") is not False
            or result.get("is-verifying-entire-crate") is not False
            or type(result.get("errors")) is not int or result["errors"] != 1
            or type(result.get("verified")) is not int
            or result["verified"] != profile["module_verified"] - 1
            or verifier.NON_SEMANTIC.search(process.stderr)):
        return False
    headers = list(verifier.ERROR_HEADER.finditer(process.stderr))
    if (len(headers) != 2 or headers[0][1] not in verifier.SEMANTIC_FAILURES
            or not headers[0][0].startswith("error: ")
            or re.fullmatch(r"aborting due to 1 previous error(?:; [1-9][0-9]* warnings? emitted)?",
                            headers[1][1]) is None):
        return False
    block = process.stderr[headers[0].end():headers[1].start()]
    primary = re.search(r"^\s*-->\s+(.+):(\d+):(\d+)\s*$", block, re.M)
    if primary is None:
        return False
    observed = Path(primary[1])
    actual = observed if observed.is_absolute() else specimen / observed
    return (os.path.normpath(str(actual)) == os.path.normpath(str(specimen / path))
            and region[0] <= int(primary[2]) <= region[1])


def observed_profile(vir: str) -> dict:
    """The translated proof-module inventory, merged across namespaces."""
    functions = {}
    for namespace in NAMESPACES:
        functions.update(inventory(vir, namespace))
    return dict(sorted(functions.items()))


def counts(functions: dict) -> dict:
    modes = {"Spec": 0, "Proof": 0, "Exec": 0}
    for record in functions.values():
        modes[record["mode"]] += 1
    return modes


def check(cache: Path, install: bool, logs: Path, emit_profile: Path | None = None) -> dict:
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise RuntimeError("this proof profile requires qualified Linux x86-64 tools")
    pin = json.loads((ROOT / verifier.PIN).read_text())
    profile = json.loads((ROOT / PROFILE).read_text())
    before = snapshot()
    proofs = {path: (ROOT / path).read_text() for path in PROOFS}
    forbidden = forbidden_tokens({HARNESS: (ROOT / HARNESS).read_text(), **proofs})
    if forbidden:
        raise RuntimeError(f"trusted declaration in a proof module: {forbidden}")
    controls = control_sources(proofs)
    tools = verifier.prepare_tools(cache, pin, install)
    environment = {k: v for k, v in os.environ.items()
                   if not k.startswith(("VERUS_", "VARGO_"))
                   and k not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")}
    environment.update(RUSTUP_TOOLCHAIN=pin["rust_toolchain"], VERUS_Z3_PATH=str(tools / "z3"))
    command = [str(tools / "verus"), "--crate-type=lib", "--edition=2024",
               "--no-cheating", "--no-external-by-default", "--num-threads", "2",
               "-V", "spinoff-all", "--output-json", "--log", "vir",
               "--log", "vir-option=no_span+no_type+no_fn_details"]
    restricted = [argument for module in MODULES for argument in ("--verify-module", module)]

    def run(label: str, args: list[str], cwd: Path = ROOT):
        folder = logs / label
        folder.mkdir()
        (folder / "command.json").write_text(json.dumps(
            {"command": args, "cwd": str(cwd)}, indent=2) + "\n")
        process = subprocess.Popen(args, cwd=cwd, env=environment, text=True,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   start_new_session=True)
        timed_out = False
        try:
            stdout, stderr = process.communicate(timeout=1800)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(process.pid, signal.SIGTERM)
            try:
                stdout, stderr = process.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                stdout, stderr = process.communicate(timeout=10)
        proc = subprocess.CompletedProcess(args, process.returncode, stdout, stderr)
        (folder / "stdout").write_text(proc.stdout)
        (folder / "stderr").write_text(proc.stderr)
        (folder / "process.json").write_text(json.dumps({
            "exit": proc.returncode, "timeout": timed_out,
            "stdout_sha256": verifier.digest(folder / "stdout"),
            "stderr_sha256": verifier.digest(folder / "stderr"),
        }, indent=2) + "\n")
        if timed_out:
            raise RuntimeError(f"verifier timeout, incomplete evidence: {label}")
        return proc, folder

    (logs / "source-before.json").write_text(json.dumps(before, sort_keys=True, indent=2) + "\n")
    positive, folder = run("positive", [*command, "--log-dir", str(logs / "positive/vir"), str(HARNESS)])
    report = json.loads(positive.stdout)
    functions = observed_profile((folder / "vir/crate.vir").read_text())
    if emit_profile is not None:
        emit_profile.write_text(json.dumps({
            **profile, "expected_verified": report["verification-results"]["verified"],
            "functions": functions, "inventory": counts(functions),
        }, indent=2, sort_keys=True) + "\n")
    if not positive_accepted(positive, report, pin, profile):
        raise RuntimeError("the unfiltered proof failed exact pinned admission")
    if functions != profile["functions"] or counts(functions) != profile["inventory"]:
        changed = sorted(set(functions) ^ set(profile["functions"])
                         | {name for name in set(functions) & set(profile["functions"])
                            if functions[name] != profile["functions"][name]})
        raise RuntimeError(f"translated proof functions differ from the reviewed profile: {changed}")
    print(f"Checked assembly: {profile['expected_verified']} obligations verified", flush=True)
    results = []
    for name, (path, changed, function) in controls.items():
        specimen = logs / (name + "-source")
        for relative in before:
            destination = specimen / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / relative, destination)
        (specimen / path).write_text(changed)
        process, folder = run(name, [*command, "--log-dir", str(logs / name / "vir"),
                                     str(HARNESS), *restricted], specimen)
        parsed = json.loads(process.stdout)
        region = function_region(changed, function)
        killed = targeted_refusal(process, parsed, pin, profile, path, specimen, region)
        results.append({"name": name, "file": str(path), "intended_function": function,
                        "killed": killed, "exit_code": process.returncode,
                        "verification_results": parsed.get("verification-results", {}),
                        "source_sha256": verifier.digest(specimen / path)})
        if not killed:
            raise RuntimeError(f"control {name} survived or failed for another reason")
        print(f"Checked assembly: control {name} fails {function}", flush=True)
    versions = {}
    for name, version_command in {
        "verifier_rust": ["rustc", f"+{pin['rust_toolchain']}", "--version"],
        "z3": [str(tools / "z3"), "--version"],
    }.items():
        version = verifier.run(version_command, ROOT, environment)
        verifier.require_success(version)
        versions[name] = version.stdout.strip()
    verifier.verify_tool_files(tools, pin)
    after = snapshot()
    (logs / "source-after.json").write_text(json.dumps(after, sort_keys=True, indent=2) + "\n")
    if before != after:
        raise RuntimeError("proof or production source changed during verification")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()
    return {
        "schema": "zeno-fcis/checked-assembly-evidence/1", "status": "passed",
        "revision": revision, "working_tree_dirty": bool(dirty), "source_sha256": before,
        "toolchain": pin, "versions": versions, "command": positive.args,
        "exit_code": positive.returncode, "verus_report": report,
        "translated_function_coverage": functions, "inventory": counts(functions),
        "controls": results,
        "proved": [
            "the checker's local, StateFact and interface tables imply the joint local check",
            "from a genesis satisfying every leaf's Init, every history of an aggregate satisfying "
            "the assembly step equation keeps the joint invariant at every state",
            "route specialization and input renaming preserve the evaluator specification's values: "
            "on an input it executes without failure, each output is its canonical term's value, "
            "so equal terms give equal outputs and same-condition selection chains choose paired cases",
        ],
        "premises": ["the local, StateFact and interface tables", "genesis", "the assembly step equation"],
        "scope": "machine-checked over the evaluator specification, with the validator implementation tested, not proved",
        "trusted_base": ["Verus translation and erasure", "bundled vstd and Z3", "Rust compilers",
                         "host platform", "reviewed mathematical specifications and coverage profile",
                         *SHARED_TRUSTED_BASE],
        "unproved": [*SHARED_TRUSTED_BASE, "liveness, fairness and traces",
                     "continuous dynamics", "asynchronous or shared-write coupling",
                     "shell, store and effects"],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install", action="store_true")
    parser.add_argument("--cache", type=Path, default=Path.home() / ".cache/zeno-fcis/verus")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--emit-profile", type=Path,
                        help="also write the observed profile here, for review")
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    logs = Path(tempfile.mkdtemp(prefix="checked-assembly-", dir=args.out.parent))
    try:
        receipt = check(args.cache.resolve(), args.install, logs, args.emit_profile)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError,
            zipfile.BadZipFile) as error:
        receipt = {"schema": "zeno-fcis/checked-assembly-evidence/1", "status": "failed",
                   "error": str(error)}
    receipt["logs"] = str(logs)
    args.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(f"Checked assembly: {receipt['status']}; receipt: {args.out}")
    if receipt["status"] != "passed":
        print(receipt["error"])
        return 1
    print(f"{receipt['inventory']['Proof']} proof functions; "
          f"{receipt['verus_report']['verification-results']['verified']} obligations; "
          f"{len(receipt['controls'])} controls caught")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
