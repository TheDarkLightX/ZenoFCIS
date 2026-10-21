#!/usr/bin/env python3
"""Build five isolated contract-table mutants and require named test failures."""

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
                "-p", "zeno-fcis-spec", "--test", "tables"]
PARSER = "crates/zeno-fcis-spec/src/parser.rs"
CONTROLS = (
    ("dropped-last-entry", PARSER,
     "for k in 0..size {\n            let entry = ",
     "for k in 0..size - 1 {\n            let entry = ",
     "tables_expand_every_entry"),
    ("forall-off-by-one", PARSER,
     "let entries: Vec<RelExpr> = self.per_entry(&variable, size, |parser| {",
     "let entries: Vec<RelExpr> = self.per_entry(&variable, size - 1, |parser| {",
     "tables_forall_covers_every_entry_exactly"),
    ("count-strict", "crates/zeno-fcis-spec/src/tables.rs",
     "CompareOp::LessEq => at_most(conditions, k),",
     "CompareOp::LessEq => at_most(conditions, k.saturating_sub(1)),",
     "tables_count_matches_direct_counting_on_every_assignment"),
    ("declaration-order-ids", PARSER,
     "let entry = id.get().checked_add(k).and_then(StableId::new);",
     "let entry = id.get().checked_add(k + self.table_infos.len() as u32).and_then(StableId::new);",
     "tables_entry_ids_do_not_depend_on_declaration_order"),
    ("collision-check-removed", PARSER,
     "if let Some((id, kind, _)) = clash {",
     "if let Some((id, kind, _)) = clash.filter(|_| false) {",
     "tables_refuse_entry_ids_that_meet_any_other_declaration"),
)


def run(command, cwd, log):
    with log.open("w") as output:
        result = subprocess.run(command, cwd=cwd, stdout=output,
                                stderr=subprocess.STDOUT, timeout=900, check=False)
    return result.returncode


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path,
                        help="directory for mutant copies and logs; default: a new temporary directory")
    parser.add_argument("--remove-targets", action="store_true",
                        help="delete each mutant's Cargo target and source copy once its result is recorded")
    args = parser.parse_args()
    if args.out is None:
        args.out = Path(tempfile.mkdtemp(prefix="table-controls-"))
    args.out.mkdir(parents=True, exist_ok=True)
    targets = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "table-controls"
    results = []
    for name, relative, before, after, test in CONTROLS:
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
        print(f"table-controls: {name}: {'PASS' if killed else 'FAIL'}", flush=True)
        (args.out / "results.json").write_text(json.dumps({
            "schema": "zeno-fcis/table-controls/1", "results": results,
            "target_dir_is_external": bool(os.environ.get("CARGO_TARGET_DIR")),
        }, indent=2) + "\n")
        if args.remove_targets:
            shutil.rmtree(target, ignore_errors=True)
            shutil.rmtree(source, ignore_errors=True)
        if not killed:
            raise RuntimeError(f"{name}: control did not build and fail its named test")
    print(f"table-controls: PASS ({len(CONTROLS)} built mutants, {len(CONTROLS)} named failures)")


if __name__ == "__main__":
    main()
