#!/usr/bin/env python3
"""Check normal Lean induction results, then compile and kill seven exporter faults.

Requires the repository-pinned Lean distribution in ZENO_FCIS_LEAN{,_ROOT}.
Each mutation is applied to an isolated source copy. A Rust build failure or
infrastructure failure never counts as a killed mutation. The optional concrete
genesis feature is not implemented, so its optional control does not apply.
"""

from pathlib import Path
import os
import shutil
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


def run(root, env, test, exact):
    command = ["cargo", "+1.97.1", "test", "-p", "zeno-fcis-formal-tools",
               "--locked", "--offline", "-j", "2", "--lib", test, "--",
               "--ignored", "--nocapture", "--test-threads=1"]
    if exact:
        command.append("--exact")
    return subprocess.run(command, cwd=root, env=env, stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT, text=True, timeout=900)


def main():
    env = dict(os.environ)
    for name in ("ZENO_FCIS_LEAN", "ZENO_FCIS_LEAN_ROOT"):
        if not env.get(name):
            raise SystemExit(f"{name} is required; no substituted tool is allowed")
    env.setdefault("CARGO_TARGET_DIR", str(ROOT / "target" / "lean-inductive-mutations"))
    env.setdefault("CARGO_PROFILE_DEV_DEBUG", "0")
    # Avoid overwriting the normal-source artifacts with mutation-source files.
    env.pop("ZENO_FCIS_LEAN_EVIDENCE", None)
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
            result = run(copy, env, PREFIX + fixture, True)
            path.write_text(original)
            print(result.stdout, flush=True)
            if result.returncode == 0:
                raise SystemExit(f"SURVIVED: {name}")
            if "test result: FAILED." not in result.stdout or "assertion `left == right` failed" not in result.stdout:
                raise SystemExit(f"mutation did not reach the expected result-flip assertion: {name}")
            print(f"KILLED: {name} by {PREFIX + fixture}", flush=True)
    # The real source must still pass after all isolated mutations.
    restored = run(ROOT, env, PREFIX, False)
    print(restored.stdout, flush=True)
    if restored.returncode or "7 passed; 0 failed" not in restored.stdout:
        raise SystemExit("real-source controls did not pass after mutation runs")
    print(f"PASS: normal controls, {len(MUTATIONS)} compiled result flips, restored controls")


if __name__ == "__main__":
    main()
