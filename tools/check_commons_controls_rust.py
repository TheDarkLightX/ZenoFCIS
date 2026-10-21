#!/usr/bin/env python3
"""Planted controls for the Rust Commons port."""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "crates" / "zeno-fcis-commons"

CONTROLS = [
    (
        "lease-prefix-ignored",
        "ops.rs",
        'resource.starts_with(&format!("{a}/")) || parent_or_child.starts_with(&format!("{b}/"))',
        "false",
        "lease_on_a_folder_protects_its_parent_and_children",
    ),
    (
        "receipt-moves-backwards",
        "ops.rs",
        "let new = current.max(through);",
        "let new = through;",
        "messages_and_read_receipts_only_move_forward",
    ),
    (
        "done-with-leases",
        "ops.rs",
        "if !held.is_empty() {",
        "if false {",
        "done_requires_released_leases",
    ),
    (
        "chain-omits-body",
        "lib.rs",
        "let payload = json!([at, actor, kind, subject, body]);",
        "let payload = json!([at, actor, kind, subject, {}]);",
        "tampering_with_history_is_detected",
    ),
    (
        "advise-keeps-superseded",
        "ops.rs",
        'if ["superseded", "stale", "rejected"]',
        "if false && [\"superseded\", \"stale\", \"rejected\"]",
        "superseded_lessons_are_not_advice",
    ),
    (
        "reimport-writes-events",
        "ops.rs",
        "ApplyResult::Rollback(json!({",
        "ApplyResult::Commit(json!({",
        "import_is_idempotent_and_writes_no_events_the_second_time",
    ),
    (
        "unregistered-writes",
        "ops.rs",
        "if !registered {",
        "if false {",
        "unregistered_actors_cannot_coordinate_but_emit_is_allowed",
    ),
    (
        "duplicate-keeps-lower-severity",
        "ops.rs",
        "        .min();",
        "        .max();",
        "duplicate_link_folds_once_and_selects_highest_severity",
    ),
    (
        "duplicate-stays-open",
        "ops.rs",
        "UPDATE records SET status='superseded',updated_seq=?1 WHERE id=?2",
        "UPDATE records SET updated_seq=?1 WHERE id=?2",
        "duplicate_link_folds_once_and_selects_highest_severity",
    ),
    (
        "reporters-not-counted",
        "ops.rs",
        "std::cmp::Reverse(reporters.len())",
        "0",
        "gaps_rank_by_severity_then_reporters_then_age",
    ),
]


def cargo_test(test: str | None = None) -> subprocess.CompletedProcess[str]:
    command = [
        "nice",
        "-n",
        "19",
        "ionice",
        "-c3",
        "cargo",
        "+1.97.1",
        "--offline",
        "--locked",
        "test",
        "-p",
        "zeno-fcis-commons",
    ]
    if test:
        command.extend(["--test", "commons", test])
    environment = os.environ.copy()
    environment.setdefault("CARGO_BUILD_JOBS", "3")
    return subprocess.run(command, cwd=ROOT, env=environment, capture_output=True, text=True, timeout=300)


def main() -> int:
    failures = []
    originals = {
        name: (SOURCE / "src" / name).read_text()
        for name in {source_name for _, source_name, *_ in CONTROLS}
    }
    try:
        result = cargo_test()
        if result.returncode:
            print("unmutated Rust crate failed its tests")
            print(result.stdout)
            print(result.stderr)
            return 1
        print("unmutated Rust crate: PASS")
        for name, source_name, before, after, test in CONTROLS:
            source = SOURCE / "src" / source_name
            contents = originals[source_name]
            if contents.count(before) != 1:
                failures.append(f"{name}: mutation site is missing or ambiguous")
                continue
            source.write_text(contents.replace(before, after))
            try:
                result = cargo_test(test)
            finally:
                source.write_text(contents)
            output = result.stdout + result.stderr
            caught = (
                result.returncode != 0
                and f"test {test} ... FAILED" in output
                and "assertion" in output.lower()
            )
            print(f"{name}: {'caught by ' + test if caught else 'NOT CAUGHT'}")
            if not caught:
                failures.append(f"{name}: {test} still passes with the mutation")
                if result.returncode:
                    print(output)
    finally:
        for name, contents in originals.items():
            (SOURCE / "src" / name).write_text(contents)
    for failure in failures:
        print("FAIL", failure)
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
