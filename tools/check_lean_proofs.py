#!/usr/bin/env python3
"""Check model proofs on two installed Lean versions, without CI or downloads."""

from pathlib import Path
import argparse
import hashlib
import json
import os
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PROOFS = ROOT / "verification" / "lean"
VERSIONS = ("4.28.0", "4.32.0")
ALLOWED = frozenset({"propext", "Quot.sound"})
AUDITOR = ROOT / "tools" / "lean" / "AuditModule.lean"
FORBIDDEN = re.compile(r"\b(?:sorry|admit|axiom|opaque|native_decide|implemented_by|extern|unsafe|csimp"
                       r"|skipKernelTC|set_option|run_cmd|run_elab|run_meta|addDecl)\b|#eval\b")
IMPORT = re.compile(r"\bimport\s+(?:all\s+)?([^\s]+)")
AXIOMS = re.compile(
    r"'([^']+)' (?:does not depend on any axioms|depends on axioms: \[([^\]]*)\])"
)
TRANSCRIPT = []


def emit(message):
    """Retain the complete transcript independently of a caller's stdout sink."""
    TRANSCRIPT.append(message)
    print(message, flush=True)


def proof_files(root):
    """Refuse every symlink before taking the regular proof-source snapshot.

    Path.rglob does not descend into directory symlinks. Refusing those entries
    avoids a second module set that compilation could see but this gate skips.
    Non-source links, broken links and a linked proof root are refused as well.
    """
    if root.is_symlink():
        raise ValueError(f"symlink in proof tree: {root}")
    files = []
    for path in root.rglob("*"):
        if path.is_symlink():
            raise ValueError(f"symlink in proof tree: {path}")
        if path.is_file() and path.suffix == ".lean":
            files.append(path)
    return sorted(files)


def lean(version, *args, search_path=None, tool="lean"):
    command = ["nice", "-n", "19", "ionice", "-c3", "elan", "run",
               f"leanprover/lean4:v{version}", tool, *map(str, args)]
    env = os.environ.copy()
    # Only this invocation's freshly compiled modules precede the standard library.
    env["LEAN_PATH"] = str(search_path) if search_path is not None else ""
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True,
                            timeout=180, env=env)
    return result.returncode, result.stdout + result.stderr


def check_source(source, modules=frozenset()):
    """Defence in depth; this does not discover declarations or prove safety.

    Deliberately conservative: reserved tokens are refused even in comments.
    Only Std and other proof modules may be imported: the elaborator commands
    that can add declarations without the kernel need Lean itself.
    The kernel replay and the semantic audit are independent of this refusal.
    """
    if match := FORBIDDEN.search(source):
        raise ValueError(f"forbidden proof-source token: {match.group()}")
    for name in IMPORT.findall(source):
        if name != "Std" and name not in modules:
            raise ValueError(f"forbidden proof-source import: {name}")


def check_output(output, expected, allowed=ALLOWED):
    """Compare named reports with the explicit manifest, never source patterns."""
    if re.search(r"\b(?:error|warning):", output):
        raise ValueError("Lean emitted an error or warning")
    found = AXIOMS.findall(output)
    if len(found) != len(expected) or len({name for name, _ in found}) != len(found):
        raise ValueError("missing or duplicate theorem axiom reports")
    if {name for name, _ in found} != set(expected):
        raise ValueError("theorem names and axiom reports differ")
    for name, axioms in found:
        used = {a.strip() for a in axioms.split(",") if a.strip()}
        if used - allowed:
            raise ValueError(f"unexpected axioms for {name}: {sorted(used - allowed)}")
        if sorted(used) != sorted(expected[name]):
            raise ValueError(f"axiom manifest differs for {name}")


def compile_module(version, path, source_root, build):
    """Compile one snapshot with a stable module identity and fresh output."""
    relative = path.relative_to(source_root).with_suffix("")
    module = ".".join(relative.parts)
    output = build / relative.with_suffix(".olean")
    output.parent.mkdir(parents=True, exist_ok=True)
    code, text = lean(version, "--root", source_root, "-o", output, path,
                      search_path=build)
    if code or re.search(r"\b(?:error|warning):", text):
        raise ValueError(f"Lean rejected {module}: exit {code}\n{text}")
    return module, text


def replay_module(version, module, build):
    """Re-check every declaration of the compiled module in Lean's kernel.

    Compilation alone is not enough: elaborator options can add a declaration
    the kernel never saw, and the axiom audit reads the same environment.
    """
    code, text = lean(version, module, search_path=build, tool="leanchecker")
    if code:
        raise ValueError(f"Lean kernel replay rejected {module}: exit {code}\n{text}")
    return text


def audit_module(version, module, build, auditor=AUDITOR):
    """Lean owns the module-wide verdict; return its packet and actual exit."""
    code, text = lean(version, "--run", auditor, module, search_path=build)
    try:
        packet = json.loads(text)
    except json.JSONDecodeError as error:
        raise ValueError(f"invalid module audit: exit {code}\n{text}") from error
    if packet.get("schema") != "zeno-fcis/lean-module-audit/1" or packet.get("module") != module:
        raise ValueError("wrong module audit identity")
    return code, packet


def require_audit(code, packet):
    """Check the transport and independently enforce the packet's invariant."""
    records = packet["constants"]
    if (code != 0 or packet["accepted"] is not True or packet["rejected_count"] != 0
            or not records or packet["constant_count"] != len(records)
            or len({r["name"] for r in records}) != len(records)):
        raise ValueError(f"module axiom audit rejected: exit {code}\n{json.dumps(packet)}")
    allowed = ALLOWED
    for record in records:
        if (record["unsafe"] or record["forbidden"] or set(record["axioms"]) - allowed
                or record["kind"] in {"axiom", "opaque"}):
            raise ValueError(f"module axiom audit contains forbidden declaration: {record}")


def check_module(version, path, source_root, build, named, auditor=AUDITOR, modules=frozenset()):
    check_source(path.read_text(), modules)
    module, text = compile_module(version, path, source_root, build)
    emit(f"CHECK {version} {module}: exit 0\n{text}")
    replay_module(version, module, build)
    emit(f"REPLAY {version} {module}: exit 0")
    code, packet = audit_module(version, module, build, auditor)
    emit(f"AUDIT {version} {module}: exit {code}\n{json.dumps(packet, sort_keys=True)}")
    require_audit(code, packet)
    actual = {r["name"]: r for r in packet["constants"]}
    expected = {name: axioms for name, axioms in named.items() if name in actual}
    allowed = ALLOWED
    check_output(text, expected, allowed)
    for name, axioms in expected.items():
        if actual[name]["kind"] != "theorem" or sorted(actual[name]["axioms"]) != sorted(axioms):
            raise ValueError(f"named theorem differs from imported kernel declaration: {name}")
    return packet, set(expected)


def controls(version, path, source, search_path=None):
    # First the unmodified model is compiled by main. Each added assertion
    # states a concrete false claim, so rejection must be a proof failure.
    if path.name == "CollectionsAggregate.lean":
        anchor = "    (delta : after + namedTotal amount rows writes = before + writtenTotal amount writes) :"
        if source.count(anchor) != 1:
            raise ValueError("summary-delta control anchor is not unique")
        mutant_source = source.replace(anchor, "    :")
        digest = lambda text: hashlib.sha256(text.encode()).hexdigest()
        with tempfile.TemporaryDirectory(prefix="lean-summary-control-") as directory:
            mutant = Path(directory) / path.name
            mutant.write_text(mutant_source)
            code, output = lean(version, "--root", directory, mutant, search_path=search_path)
            emit(f"CONTROL {version} summary_delta_dropped: exit {code}\n{output}")
            if code != 1 or "error: omega could not prove the goal" not in output:
                raise ValueError("summary-delta deletion did not fail on the proof obligation")
            mutant.write_text(source)
            restored_code, restored_output = lean(version, "--root", directory, mutant, search_path=search_path)
            if restored_code or re.search(r"\b(?:error|warning):", restored_output):
                raise ValueError("restored summary proof did not compile")
            restored_hash = hashlib.sha256(mutant.read_bytes()).hexdigest()
        result = {"control": "summary_delta_dropped", "version": version,
                  "normal": 0, "mutated": code, "restored": restored_code,
                  "normal_sha256": digest(source), "mutated_sha256": digest(mutant_source),
                  "restored_sha256": restored_hash}
        if result["normal_sha256"] != restored_hash:
            raise ValueError("summary proof restoration differs bytewise")
        emit("CONTROL_RESTORED " + json.dumps(result, sort_keys=True))
        return [result]
    if path.name in {
        "CollectionsPatterns.lean", "CollectionsPatternsExecutable.lean", "CollectionsReduction.lean", "CollectionsReductionGenesis.lean",
        "CollectionsReductionTallies.lean", "CollectionsReductionValidation.lean",
        "CollectionsSignedTallies.lean", "CollectionsValidationWitnesses.lean",
    }:
        # The reduction modules share the deleted-premise control above. The
        # enumeration implementation has a separate exhaustive differential control.
        return []
    if path.name == "Collections.lean":
        cases = {"role_uniformity_control": """
theorem role_uniformity_control :
    Collections.zeroEntry
      (Collections.roleStep (fun _ : Bool => none) false true true) := by
  change (some true : Option Bool) ≠ some true
  decide
"""}
    elif path.name == "CollectionsRequiresEM.lean":
        cases = {"entry_law_false_control": """
theorem entry_law_false_control : CollectionsRequiresEM.entryLaw (some False) := by
  change False
  decide
"""}
    elif path.name == "FixedPoints.lean":
        cases = {"top_seed_control": """
-- Incorrect least-chain seed: identity is monotone and stable from top,
-- but its alleged least result must be below the fixed point bottom.
theorem top_seed_control :
    FixedPoints.iter (fun x : FixedPoints.Bits 1 => x)
      FixedPoints.top 0 (0 : Fin 1) = FixedPoints.bottom (0 : Fin 1) := by
  change true = false
  decide
""", "monotonicity_control": """
-- Without monotonicity the one-bit negation chain oscillates. It need not
-- repeat by comparison n+1 (here n=1).
theorem monotonicity_control :
    FixedPoints.iter (fun x : FixedPoints.Bits 1 => fun i => !(x i))
      FixedPoints.bottom 1 (0 : Fin 1) =
    FixedPoints.iter (fun x : FixedPoints.Bits 1 => fun i => !(x i))
      FixedPoints.bottom 2 (0 : Fin 1) := by
  change true = false
  decide
"""}
    elif path.name == "IndexedFixedPoints.lean":
        cases = {"indexed_middle_seed_control": """
theorem indexed_middle_seed_control :
    FixedPoints.iter
      (IndexedFixedPoints.middle IndexedFixedPoints.middleIdentity 0 FixedPoints.top)
      FixedPoints.top 1 (0 : Fin 1) =
    IndexedFixedPoints.value IndexedFixedPoints.middleIdentity (0 : Fin 1) := by
  change true = false
  decide
"""}
    else:
        raise ValueError(f"no defect control defined for {path.name}")
    for name, assertion in cases.items():
        with tempfile.TemporaryDirectory(prefix="lean-proof-control-") as directory:
            mutant = Path(directory) / path.name
            mutant.write_text(source + assertion)
            code, output = lean(version, "--root", directory, mutant, search_path=search_path)
        emit(f"CONTROL {version} {name}: exit {code}\n{output}")
        if code != 1 or not re.search(r"error: Tactic `decide` proved that the proposition\n[\s\S]*?\nis false", output):
            raise ValueError(f"{name} did not fail on the expected false assertion")
    return []


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--controls", action="store_true")
    parser.add_argument("--report", type=Path, help="retain all module audits as JSON")
    parser.add_argument("--transcript", type=Path, help="retain the complete compilation and control transcript")
    args = parser.parse_args()
    files = proof_files(PROOFS)
    if not files:
        raise ValueError("no proof files")
    named = json.loads((PROOFS / "AXIOMS.json").read_text())
    snapshots = {path.relative_to(PROOFS): path.read_bytes() for path in files}
    auditor_bytes = AUDITOR.read_bytes()
    results = {"schema": "zeno-fcis/lean-proof-check/2", "source_sha256": {
        str(p): hashlib.sha256(b).hexdigest() for p, b in snapshots.items()},
        "auditor_sha256": hashlib.sha256(auditor_bytes).hexdigest(), "toolchains": {}, "controls": []}
    for version in VERSIONS:
        code, output = lean(version, "--version")
        if code or f"version {version}," not in output:
            raise ValueError(f"wrong or unavailable Lean {version}: {output}")
        emit(output.strip())
        # Nothing is imported from an old build. Compilation and auditing consume
        # immutable byte snapshots in invocation-owned temporary directories.
        with tempfile.TemporaryDirectory(prefix="lean-proof-check-") as directory:
            temp = Path(directory)
            source_root, build = temp / "source", temp / "build"
            build.mkdir()
            auditor = temp / "AuditModule.lean"
            auditor.write_bytes(auditor_bytes)
            for relative, content in snapshots.items():
                path = source_root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(content)
            packets, seen = [], set()
            modules = frozenset(".".join(p.with_suffix("").parts) for p in snapshots)
            for relative, content in snapshots.items():
                path = source_root / relative
                packet, reports = check_module(version, path, source_root, build, named, auditor, modules)
                if seen & reports:
                    raise ValueError("named theorem reported by multiple proof modules")
                seen |= reports
                packets.append(packet)
                if args.controls:
                    results["controls"].extend(controls(version, path, content.decode(), search_path=build))
            if seen != set(named):
                raise ValueError(f"named axiom manifest coverage differs: {sorted(set(named) - seen)}")
            results["toolchains"][version] = packets
    if args.report:
        args.report.write_text(json.dumps(results, indent=2, sort_keys=True) + "\n")
    emit("PASS: kernel replay, every module constant, named axiom manifest and requested controls")
    if args.transcript:
        args.transcript.write_text("\n".join(TRANSCRIPT) + "\n")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.TimeoutExpired) as error:
        raise SystemExit(str(error)) from error
