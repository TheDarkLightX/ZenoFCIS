#!/usr/bin/env python3
"""Restore the rejected transition premise, build it, and require assertion failures."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates/zeno-fcis-cli/src/contract/review/numeric_symbolic/lemma.rs"
PREFIX = "contract::review::numeric_symbolic::tests::lemma_tests::"
CARGO = ["nice", "-n", "19", "ionice", "-c3", "cargo", "+1.97.1", "test",
         "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked", "--offline"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    before = SOURCE.read_bytes()
    original = before.decode()
    needle = "pre_state.clone(),"
    assert original.count(needle) == 2, "control must restore both former premises"
    results = []
    cases = [
        ("old-transition-premises", 2, "pinned_transition_lemma_repairs_match_exhaustive_counts"),
        ("old-nonvacuity-premise", 1, "pinned_transition_nonvacuity_includes_only_unlawful_repairs"),
    ]
    for name, occurrences, test in cases:
        try:
            mutated = original.replace(needle, "e.lawful.clone(),", occurrences).encode()
            SOURCE.write_bytes(mutated)
            build_command = [*CARGO, PREFIX + test, "--no-run"]
            build = subprocess.run(build_command, cwd=ROOT, capture_output=True, text=True)
            (args.out / f"{name}.build.log").write_text(build.stdout + build.stderr)
            assert build.returncode == 0, f"control must build: {name}"
            command = [*CARGO, PREFIX + test, "--", "--exact", "--ignored", "--nocapture",
                       "--test-threads=1"]
            run = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
            output = run.stdout + run.stderr
            (args.out / f"{name}.test.log").write_text(output)
            killed = (run.returncode == 101
                      and f"test {PREFIX + test} ... FAILED" in output
                      and "assertion `left == right` failed" in output)
            result = dict(control=name, build_command=build_command, build_exit=build.returncode,
                          test=PREFIX + test, command=command, exit=run.returncode,
                          assertion_failed=killed,
                          original_sha256=hashlib.sha256(before).hexdigest(),
                          mutated_sha256=hashlib.sha256(mutated).hexdigest())
            results.append(result)
            (args.out / "results.json").write_text(json.dumps(results, indent=2) + "\n")
            assert killed, f"control must fail its named assertion: {name}"
        finally:
            SOURCE.write_bytes(before)
            assert SOURCE.read_bytes() == before, "source bytes must be restored"
    print(f"{len(results)} controls built and failed their named assertions; source restored")


if __name__ == "__main__":
    main()
