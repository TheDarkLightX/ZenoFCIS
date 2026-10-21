#!/usr/bin/env python3
"""Build isolated keyed-collection mutants and require named test failures."""

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
SPEC = ("zeno-fcis-spec", "collections")
CONTRACT = ("zeno-fcis-contract", "collections")
PARSER = "crates/zeno-fcis-spec/src/parser.rs"
SPEC_COLLECTIONS = "crates/zeno-fcis-spec/src/collections.rs"
CHECKER = "crates/zeno-fcis-contract/src/collections.rs"
CONTROLS = (
    ("unnamed-entry-read", SPEC, SPEC_COLLECTIONS,
     "if self.reads_map(value) {",
     "if self.reads_map(value) && false {",
     "collections_refuse_reading_a_map_except_through_named_entries"),
    ("key-order-comparison", SPEC, SPEC_COLLECTIONS,
     '.then_some("keys are compared only with == and !=")',
     '.then_some("keys are compared only with == and !=").filter(|_| false)',
     "collections_compare_keys_only_by_equality"),
    ("aggregate-without-summary", SPEC, PARSER,
     "self.refuse_entries_aggregate(start);\n            return RelExpr::Bool(false);",
     "self.skip_to_declaration_end();\n            return RelExpr::Bool(true);",
     "collections_refuse_aggregates_without_a_summary"),
    ("aliasing-pattern-skipped", CONTRACT, CHECKER,
     "if class > *seen + 1 {",
     "if class > *seen + 1 || class <= *seen {",
     "collections_self_transfer_that_mints_is_refuted_on_the_aliased_pattern"),
)


def run(command, cwd, log):
    with log.open("w") as output:
        result = subprocess.run(command, cwd=cwd, stdout=output,
                                stderr=subprocess.STDOUT, timeout=1800, check=False)
    return result.returncode


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path,
                        help="directory for mutant copies and logs; default: a new temporary directory")
    parser.add_argument("--remove-targets", action="store_true",
                        help="delete each mutant's Cargo target and source copy once its result is recorded")
    args = parser.parse_args()
    if args.out is None:
        args.out = Path(tempfile.mkdtemp(prefix="collections-controls-"))
    args.out.mkdir(parents=True, exist_ok=True)
    targets = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "collections-controls"
    results = []
    for name, (package, test_target), relative, before, after, test in CONTROLS:
        directory = Path(tempfile.mkdtemp(prefix=name + "-", dir=args.out))
        source = directory / "source"
        shutil.copytree(ROOT, source, ignore=shutil.ignore_patterns(
            ".git", "target", "node_modules", "__pycache__"))
        path = source / relative
        text = path.read_text()
        if text.count(before) != 1:
            raise RuntimeError(f"{name}: mutation anchor is not unique")
        changed = text.replace(before, after)
        path.write_text(changed)
        # A distinct target prevents absolute dep-info from a different copy
        # from reusing an unmutated test executable.
        target = targets / directory.name
        command = WRAP + ["cargo", "+1.97.1", "test", "--locked", "--offline", "-p", package,
                          "--test", test_target, "--target-dir", str(target)]
        build = run(command + ["--no-run"], source, directory / "build.log")
        failure = None if build else run(command + [test, "--", "--exact"],
                                         source, directory / "test.log")
        output = (directory / "test.log").read_text() if failure is not None else ""
        killed = build == 0 and failure == 101 and f"test {test} ... FAILED" in output
        results.append({"control": name, "directory": directory.name, "path": relative, "test": test,
                        "package": package,
                        "before_sha256": hashlib.sha256(text.encode()).hexdigest(),
                        "after_sha256": hashlib.sha256(changed.encode()).hexdigest(),
                        "build_exit": build, "test_exit": failure, "killed": killed})
        print(f"collections-controls: {name}: {'PASS' if killed else 'FAIL'}", flush=True)
        (args.out / "results.json").write_text(json.dumps({
            "schema": "zeno-fcis/collections-controls/1", "results": results,
            "target_dir_is_external": bool(os.environ.get("CARGO_TARGET_DIR")),
        }, indent=2) + "\n")
        if args.remove_targets:
            shutil.rmtree(target, ignore_errors=True)
            shutil.rmtree(source, ignore_errors=True)
        if not killed:
            raise RuntimeError(f"{name}: control did not build and fail its named test")
    print(f"collections-controls: PASS ({len(CONTROLS)} built mutants, {len(CONTROLS)} named failures)")


if __name__ == "__main__":
    main()
