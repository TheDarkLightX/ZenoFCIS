#!/usr/bin/env python3
"""Check Lean induction controls, seven exporter faults and the axiom ceiling.

Requires the repository-pinned Lean distribution in ZENO_FCIS_LEAN{,_ROOT}.
Each mutation is applied to an isolated source copy. A Rust build failure or
infrastructure failure never counts as a killed mutation. The optional concrete
genesis feature is not implemented, so its optional control does not apply.
"""

from pathlib import Path
import os
import shutil
import signal
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = Path("crates/zeno-fcis-formal-tools/src/lean_inductive.rs")
PREFIX = "lean_inductive::tests::pinned_lean_control_"
MUTATIONS = (
    ("post-invariant assumed", "decrement",
     "let before = &rendered[grouped.len()];",
     "assumptions.push(satisfied(&rendered[grouped.len() + 1]));\n    let before = &rendered[grouped.len()];"),
    ("decision split dropped", "kind_split",
     'Some(kind) => format!("(kind = {kind} → {holds})"),',
     "Some(_kind) => holds,"),
    ("declared domains dropped", "domain",
     "assumptions.push(domain_formula(&term, &domain));",
     "let _ = domain_formula(&term, &domain);"),
    ("domain attached to another path", "wrong_domain",
     "if let Some(domain) = declared_domain(spec, path) {",
     "let swapped = ProjectionPath::try_new(path.root(), path.segments().iter().map(|id| {\n"
     "    match id.get() { 110 => StableId::new(111).unwrap_or_else(|| unreachable!()),\n"
     "    111 => StableId::new(110).unwrap_or_else(|| unreachable!()), _ => *id }\n"
     "}).collect()).ok_or(ExportError::InvalidFormula)?;\n"
     "if let Some(domain) = declared_domain(spec, &swapped) {"),
    ("checked arithmetic definedness dropped", "overflow",
     "conjunction(vec![value.defined.clone(), value.term.clone()])",
     "value.term.clone()"),
    ("all declared laws assumed", "law_scope",
     "let grouped = grouped_laws(&resolved);",
     "let _ = resolved;\n    let grouped = spec.laws().iter().map(|law| (law, None)).collect::<Vec<_>>();"),
    ("projection 110 mapped to 111", "projection",
     'Ok(format!("({owner}.{})", field_name(path)))',
     'let field = field_name(path);\n    Ok(format!("({owner}.{})", if field == "p_100_110" { "p_100_111" } else { &field }))'),
)


def run(root, env, test, exact, ignored=True):
    available = shutil.disk_usage("/").free
    print(f"root free before build: {available // (1024**3)} GiB", flush=True)
    if available < 6 * 1024**3:
        raise SystemExit("root disk below 6 GiB; build stopped")
    command = ["cargo", "+1.97.1", "test", "-p", "zeno-fcis-formal-tools",
               "--locked", "--offline", "-j", "2", "--lib", test, "--",
               "--nocapture", "--test-threads=1"]
    if ignored:
        command.append("--ignored")
    if exact:
        command.append("--exact")
    process = subprocess.Popen(command, cwd=root, env=env, stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT, text=True, start_new_session=True)
    try:
        # This includes multiple full runtime snapshots on a slow drive; each
        # Lean subprocess still has its own explicit five-minute timeout.
        output, _ = process.communicate(timeout=7200)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        output, _ = process.communicate()
        print(output, flush=True)
        raise SystemExit("mutation lane timed out; its process group was stopped")
    return subprocess.CompletedProcess(command, process.returncode, output)


def main():
    env = dict(os.environ)
    for name in ("ZENO_FCIS_LEAN", "ZENO_FCIS_LEAN_ROOT"):
        if not env.get(name):
            raise SystemExit(f"{name} is required; no substituted tool is allowed")
    env.setdefault("CARGO_TARGET_DIR", str(ROOT / "target" / "lean-inductive-mutations"))
    env.setdefault("CARGO_PROFILE_DEV_DEBUG", "0")
    # Avoid overwriting the normal-source artifacts with mutation-source files.
    env.pop("ZENO_FCIS_LEAN_EVIDENCE", None)
    facts_test = "lean_inductive::tests::inductive_lean_and_smt_emit_identical_structured_facts"
    facts = run(ROOT, env, facts_test, True, ignored=False)
    print(facts.stdout, flush=True)
    if facts.returncode or "1 passed; 0 failed" not in facts.stdout:
        raise SystemExit("normal-source rendered-facts baseline did not pass")
    axiom_test = "lean_inductive::tests::inductive_lean_tampered_evidence_is_blocked"
    axiom_baseline = run(ROOT, env, axiom_test, True, ignored=False)
    print(axiom_baseline.stdout, flush=True)
    if axiom_baseline.returncode or "1 passed; 0 failed" not in axiom_baseline.stdout:
        raise SystemExit("normal-source axiom ceiling baseline did not pass")
    baseline = run(ROOT, env, PREFIX, False)
    print(baseline.stdout, flush=True)
    if baseline.returncode or "7 passed; 0 failed" not in baseline.stdout:
        raise SystemExit("normal-source control baseline did not pass all seven tests")
    with tempfile.TemporaryDirectory(prefix="lean-inductive-mutations-") as directory:
        copy = Path(directory) / "source"
        shutil.copytree(ROOT, copy, ignore=shutil.ignore_patterns(
            ".git", "target", "node_modules", "__pycache__", ".zeno-fcis"))
        path = copy / SOURCE
        original = path.read_text()
        for name, fixture, before, after in MUTATIONS:
            if original.count(before) != 1:
                raise SystemExit(f"mutation anchor must match once: {name}")
            path.write_text(original.replace(before, after, 1))
            if fixture in ("kind_split", "domain"):
                comparison = run(copy, env, facts_test, True, ignored=False)
                print(comparison.stdout, flush=True)
                if (comparison.returncode == 0 or "test result: FAILED." not in comparison.stdout
                        or "assertion `left == right` failed" not in comparison.stdout):
                    raise SystemExit(f"rendered-fact comparison did not catch: {name}")
                print(f"FACT COMPARISON KILLED: {name}", flush=True)
            result = run(copy, env, PREFIX + fixture, True)
            path.write_text(original)
            print(result.stdout, flush=True)
            if result.returncode == 0:
                raise SystemExit(f"SURVIVED: {name}")
            if "test result: FAILED." not in result.stdout or "assertion `left == right` failed" not in result.stdout:
                raise SystemExit(f"mutation did not reach the expected result-flip assertion: {name}")
            flipped = (f"{fixture}: Failed(Crash(Some(1)))" if fixture == "domain"
                       else f"{fixture}: KernelChecked")
            if flipped not in result.stdout:
                raise SystemExit(f"named fixture did not flip: {name}")
            if fixture == "domain" and "omega could not prove the goal" not in result.stdout:
                raise SystemExit("domain mutation failed without the required proof diagnostic")
            print(f"KILLED: {name} by {PREFIX + fixture}", flush=True)
        gate_path = copy / "crates/zeno-fcis-formal-tools/src/lib.rs"
        gate_original = gate_path.read_text()
        gate = '''            if let Some(name) = axioms
                .iter()
                .find(|name| !matches!(name.as_str(), "propext" | "Quot.sound"))
            {
                return ToolRunStatus::Blocked(ToolFailure::DisallowedLeanAxiom(name.clone()));
            }
'''
        if gate_original.count(gate) != 1:
            raise SystemExit("axiom ceiling mutation anchor must match once")
        gate_path.write_text(gate_original.replace(gate, "", 1))
        result = run(copy, env, axiom_test, True, ignored=False)
        gate_path.write_text(gate_original)
        print(result.stdout, flush=True)
        if (result.returncode == 0 or "test result: FAILED." not in result.stdout
                or "assertion `left == right` failed" not in result.stdout
                or 'DisallowedLeanAxiom("Classical.choice")' not in result.stdout):
            raise SystemExit("axiom ceiling removal did not reach the named refusal assertion")
        print(f"KILLED: axiom ceiling removed by {axiom_test}", flush=True)
    # The real source must still pass after all isolated mutations.
    restored = run(ROOT, env, PREFIX, False)
    print(restored.stdout, flush=True)
    if restored.returncode or "7 passed; 0 failed" not in restored.stdout:
        raise SystemExit("real-source controls did not pass after mutation runs")
    restored_axioms = run(ROOT, env, axiom_test, True, ignored=False)
    print(restored_axioms.stdout, flush=True)
    if restored_axioms.returncode or "1 passed; 0 failed" not in restored_axioms.stdout:
        raise SystemExit("real-source axiom ceiling did not pass after mutation run")
    print(f"PASS: normal controls, {len(MUTATIONS)} compiled result flips, axiom ceiling, restored controls")


if __name__ == "__main__":
    from telemetry import run_cli
    def _entrypoint():
        main()
    run_cli(_entrypoint)
