#!/usr/bin/env python3
"""Check model proofs on two installed Lean versions, without CI or downloads."""

from pathlib import Path
import argparse
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PROOFS = ROOT / "verification" / "lean"
VERSIONS = ("4.28.0", "4.32.0")
ALLOWED = {"propext", "Quot.sound", "Classical.choice"}
AXIOMS = re.compile(
    r"'([^']+)' (?:does not depend on any axioms|depends on axioms: \[([^\]]*)\])"
)


def lean(version, *args):
    command = ["nice", "-n", "19", "ionice", "-c3", "elan", "run",
               f"leanprover/lean4:v{version}", "lean", *map(str, args)]
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True,
                            timeout=180)
    return result.returncode, result.stdout + result.stderr


def check_output(source, output):
    """Require one kernel axiom report for every theorem in the source."""
    if re.search(r"\b(?:sorry|admit|axiom)\b", source):
        raise ValueError("placeholder or axiom declaration in proof source")
    if re.search(r"\b(?:error|warning):", output):
        raise ValueError("Lean emitted an error or warning")
    expected = re.findall(r"^theorem\s+(\w+)", source, re.M)
    found = AXIOMS.findall(output)
    if not expected or len(found) != len(expected):
        raise ValueError("missing or duplicate theorem axiom reports")
    if sorted(name.rsplit(".", 1)[-1] for name, _ in found) != sorted(expected):
        raise ValueError("theorem names and axiom reports differ")
    for name, axioms in found:
        used = {a.strip() for a in axioms.split(",") if a.strip()}
        if used - ALLOWED:
            raise ValueError(f"unexpected axioms for {name}: {sorted(used - ALLOWED)}")


def controls(version, path, source):
    # First the unmodified model is compiled by main. Each added assertion
    # states a concrete false claim, so rejection must be a proof failure.
    if path.name == "Collections.lean":
        cases = {"role_uniformity_control": """
theorem role_uniformity_control :
    Collections.zeroEntry
      (Collections.roleStep (fun _ : Bool => none) false true true) := by
  change (some true : Option Bool) ≠ some true
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
    else:
        raise ValueError(f"no defect control defined for {path.name}")
    for name, assertion in cases.items():
        with tempfile.TemporaryDirectory(prefix="lean-proof-control-") as directory:
            mutant = Path(directory) / path.name
            mutant.write_text(source + assertion)
            code, output = lean(version, mutant)
        print(f"CONTROL {version} {name}: exit {code}\n{output}", flush=True)
        if code != 1 or not re.search(r"error: Tactic `decide` proved that the proposition\n[\s\S]*?\nis false", output):
            raise ValueError(f"{name} did not fail on the expected false assertion")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--controls", action="store_true")
    args = parser.parse_args()
    files = sorted(PROOFS.glob("*.lean"))
    if not files:
        raise ValueError("no proof files")
    for version in VERSIONS:
        code, output = lean(version, "--version")
        if code or f"version {version}," not in output:
            raise ValueError(f"wrong or unavailable Lean {version}: {output}")
        print(output.strip(), flush=True)
        for path in files:
            source = path.read_text()
            code, output = lean(version, path)
            print(f"CHECK {version} {path.relative_to(ROOT)}: exit {code}\n{output}", flush=True)
            if code:
                raise ValueError(f"Lean rejected {path.name}")
            check_output(source, output)
            if args.controls:
                controls(version, path, source)
    print("PASS: all proof files, theorem axiom reports and requested controls", flush=True)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.TimeoutExpired) as error:
        raise SystemExit(str(error)) from error
