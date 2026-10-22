#!/usr/bin/env python3
"""Build each one-defect checker, require its named test to fail, restore bytes."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / "crates/zeno-fcis-cli/src/contract/review/numeric_symbolic.rs"
ENCODE = BASE.with_suffix("") / "encode.rs"
PREFIX = "contract::review::numeric_symbolic::tests::"
CARGO = ["nice", "-n", "19", "ionice", "-c3", "cargo", "+1.97.1", "test",
         "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked", "--offline"]


def controls():
    wrap = '''let term = {
        let low = if a.kind == Kind::Unsigned { Term::Int(0) } else { Term::Int(i128::MIN) };
        let high = if a.kind == Kind::Unsigned { uint(u128::MAX) } else { Term::Int(i128::MAX) };
        let modulus = Term::mul(uint(1u128 << 127), Term::Int(2));
        Term::ite(Term::lt(high, term.clone()), Term::sub(term.clone(), modulus.clone()),
            Term::ite(Term::lt(term.clone(), low), Term::add(term.clone(), modulus), term))
    };
    guards.push(range(&term, &a.kind)?);'''
    return [
        ("wrapping-arithmetic", ENCODE, "guards.push(range(&term, &a.kind)?);", wrap,
         "pinned_numeric_overflow_boundary_guard_matches_native", True),
        ("one-solver", BASE, "(Answer::Unsat { .. }, Answer::Unsat { .. }) => Verdict::Holds,",
         "(Answer::Unsat { .. }, _) => Verdict::Holds,", "single_solver_cannot_pass", False),
        ("timeout-pass", BASE, "match (&answers.cvc5, &answers.z3) {",
         '''if matches!(answers.cvc5, Answer::Failed(_) | Answer::Unknown)
            || matches!(answers.z3, Answer::Failed(_) | Answer::Unknown) { return Verdict::Holds; }
    match (&answers.cvc5, &answers.z3) {''', "timeout_and_unknown_cannot_pass", False),
        ("guard-bypass", BASE, 'let passed = guarded["status"] == "holds";',
         "let passed = true;", "encoding_mismatch_refuses_symbolic_mode", False),
        ("in-cap-symbolic", BASE, "size(&c).is_none_or(|n| n > cap as u128)",
         "size(&c).is_none_or(|n| n >= cap as u128)", "in_cap_contract_stays_exhaustive", False),
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    results = []
    for name, path, needle, replacement, test, pinned in controls():
        before = path.read_bytes()
        original = before.decode()
        if needle not in original:
            raise RuntimeError(f"control location missing: {name}")
        try:
            path.write_text(original.replace(needle, replacement, 1))
            build = subprocess.run([*CARGO, PREFIX + test, "--no-run"], cwd=ROOT,
                                   capture_output=True, text=True)
            (args.out / f"{name}.build.log").write_text(build.stdout + build.stderr)
            if build.returncode:
                raise RuntimeError(f"control did not build: {name}")
            command = [*CARGO, PREFIX + test, "--", "--exact"]
            if pinned:
                command.append("--ignored")
            run = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
            output = run.stdout + run.stderr
            (args.out / f"{name}.test.log").write_text(output)
            killed = run.returncode != 0 and f"test {PREFIX + test} ... FAILED" in output
            results.append({"control": name, "built": True, "test": PREFIX + test,
                            "exit": run.returncode, "killed": killed,
                            "source_sha256": hashlib.sha256(before).hexdigest()})
            (args.out / "results.json").write_text(json.dumps(results, indent=2) + "\n")
            if not killed:
                raise RuntimeError(f"control survived or failed elsewhere: {name}")
        finally:
            path.write_bytes(before)
            if path.read_bytes() != before:
                raise RuntimeError(f"source restoration failed: {name}")
    print(f"controls: {len(results)} built and failed their named tests; source restored")


if __name__ == "__main__":
    main()
