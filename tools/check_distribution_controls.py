#!/usr/bin/env python3
"""Build isolated finite-distribution and reward mutants and require named test failures."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
WRAP = ["nice", "-n", "19", "ionice", "-c3"]
CARGO = WRAP + ["cargo", "+1.97.1", "test", "--locked", "--offline",
                "-p", "zeno-fcis-contract", "--test", "distribution_laws"]
CONTROLS = (
    ("modulo-bias", "examples/distribution-laws/reporter-choice/v2/policy.json",
     '"110": "context.102.130"',
     '"110": "context.102.130 - 3 * div_floor(context.102.130, 3)"',
     "distribution_examples_check_exactly_with_their_premise"),
    ("random-width", "crates/zeno-fcis-contract/src/distribution.rs",
     "if !width_matches {", "if !width_matches && false {",
     "distribution_random_width_must_match_the_entire_input_domain"),
    ("weight-total", "crates/zeno-fcis-spec/src/distribution.rs",
     "if total != Probability::try_new(1, 1)? {",
     "if total != Probability::try_new(1, 1)? && false {",
     "distribution_weights_must_sum_exactly_to_one"),
    ("partial-domain", "crates/zeno-fcis-contract/src/distribution.rs",
     "if !advance(&positions, &random_indices, &mut tuple) {",
     "if visited >= random_size / 2 || !advance(&positions, &random_indices, &mut tuple) {",
     "distribution_counts_every_random_value"),
    ("dropped-premise", "crates/zeno-fcis-contract/src/distribution.rs",
     '"premise": PREMISE,', '"missing_premise": PREMISE,',
     "distribution_every_verdict_retains_the_randomness_premise"),
    ("reward-variance-sign", "crates/zeno-fcis-contract/src/distribution.rs",
     "moment\n            .checked_sub(mean.checked_mul(mean).ok_or(\"reward arithmetic bound\")?)",
     "mean.checked_mul(mean).ok_or(\"reward arithmetic bound\")?.checked_sub(moment)",
     "reward_variance_uses_second_moment_minus_squared_mean"),
    ("reward-threshold-strict", "crates/zeno-fcis-contract/src/distribution.rs",
     "i128::from(value >= t)", "i128::from(value > t)",
     "reward_threshold_includes_equality"),
    ("reward-excess-unclamped", "crates/zeno-fcis-contract/src/distribution.rs",
     "integer(value.checked_sub(b)?.max(0))", "integer(value.checked_sub(b)?)",
     "reward_excess_is_clamped_at_zero"),
    ("reward-unnormalized", "crates/zeno-fcis-contract/src/distribution.rs",
     "u128::from(bin.mass.denominator())", "1",
     "reward_expectation_is_normalized"),
    ("reward-dropped-premise", "crates/zeno-fcis-contract/src/distribution.rs",
     '\"premise\": PREMISE,', '\"missing_premise\": PREMISE,',
     "reward_every_verdict_retains_the_randomness_premise"),
    ("failure-distinction", "crates/zeno-fcis-contract/src/distribution.rs",
     "Class::CommittedFailure => Some(DistributionOutcome::CommittedFailure),",
     "Class::CommittedFailure => projected(&decision.post, *field),",
     "distribution_committed_failure_is_one_distinct_outcome"),
)


def run(command, cwd, log):
    with log.open("w") as output:
        result = subprocess.run(command, cwd=cwd, stdout=output,
                                stderr=subprocess.STDOUT, timeout=600, check=False)
    return result.returncode


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--remove-targets", action="store_true",
                        help="delete each mutant's Cargo target once its result is recorded")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    results = []
    for name, relative, before, after, test in CONTROLS:
        directory = Path(tempfile.mkdtemp(prefix=name+"-", dir=args.out))
        source = directory / "source"
        shutil.copytree(ROOT, source, ignore=shutil.ignore_patterns(
            ".git", ".agents", ".aws", ".codex", "target", "node_modules", "__pycache__"))
        path = source / relative
        text = path.read_text()
        if text.count(before) != 1:
            raise RuntimeError(f"{name}: mutation anchor is not unique")
        changed = text.replace(before, after)
        path.write_text(changed)
        # A distinct target prevents absolute dep-info from a different copy
        # from reusing an unmutated test executable. Fail closed without it.
        target = Path(os.environ["CARGO_TARGET_DIR"]) / "distribution-controls" / directory.name
        command = CARGO + ["--target-dir", str(target)]
        build = run(command + ["--no-run"], source, directory / "build.log")
        failure = None if build else run(command + [test, "--", "--exact"],
                                         source, directory / "test.log")
        output = (directory / "test.log").read_text() if failure is not None else ""
        killed = build == 0 and failure == 101 and f"test {test} ... FAILED" in output
        results.append({"control": name, "directory": directory.name, "path": relative, "test": test,
                        "before_sha256": hashlib.sha256(text.encode()).hexdigest(),
                        "after_sha256": hashlib.sha256(changed.encode()).hexdigest(),
                        "build_exit": build, "test_exit": failure, "killed": killed})
        print(f"distribution-controls: {name}: {'PASS' if killed else 'FAIL'}", flush=True)
        (args.out / "results.json").write_text(json.dumps({
            "schema": "zeno-fcis/distribution-controls/1", "results": results,
            "target_dir_is_external": bool(os.environ.get("CARGO_TARGET_DIR")),
        }, indent=2) + "\n")
        if args.remove_targets:
            shutil.rmtree(target, ignore_errors=True)
        if not killed:
            raise RuntimeError(f"{name}: control did not build and fail its named test")
    print(f"distribution-controls: PASS ({len(CONTROLS)} built mutants, {len(CONTROLS)} named failures)")


if __name__ == "__main__":
    main()
