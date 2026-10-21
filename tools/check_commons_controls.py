#!/usr/bin/env python3
"""Planted controls for Zeno Commons: each mutation must make its named test fail.

Copies integrations/commons to a temporary directory, applies one mutation at a time, and runs the named
test. Exit 0 only if every mutation is caught and the unmutated copy passes the same tests.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "integrations" / "commons"

CONTROLS = [
    ("lease-prefix-ignored", "        return b.startswith(a.rstrip(\"/\") + \"/\") or a.startswith(b.rstrip(\"/\") + \"/\")",
     "        return False", "CommonsTests.test_lease_on_a_folder_protects_its_parent_and_children"),
    ("receipt-moves-backwards", "            new = max(current, through)  # read receipts never move backwards",
     "            new = through", "CommonsTests.test_messages_and_read_receipts_only_move_forward"),
    ("done-with-leases", "                if held:\n                    raise CommonsError(\"release your leases before posting done: \" + \", \".join(held))",
     "                pass", "CommonsTests.test_done_requires_released_leases"),
    ("chain-omits-body", "(prev + _canonical([at, actor, kind, subject, json.loads(body)])).encode()",
     "(prev + _canonical([at, actor, kind, subject, {}])).encode()", "CommonsTests.test_tampering_with_history_is_detected"),
    ("advise-keeps-superseded", "            if record[\"status\"] in (\"superseded\", \"stale\", \"rejected\"):\n                continue",
     "            pass", "CommonsTests.test_superseded_lessons_are_not_advice"),
    ("reimport-writes-events", "                raise _Existing()  # idempotent: same content, same id, and no event is written",
     "                return {\"seq\": seq, \"id\": record_id, \"existing\": True}",
     "CommonsTests.test_import_is_idempotent_and_writes_no_events_the_second_time"),
]


def run_test(directory: Path, test: str) -> int:
    return subprocess.run([sys.executable, "-m", "unittest", f"test_commons.{test}"], cwd=directory,
                          capture_output=True, text=True, timeout=300).returncode


def main() -> int:
    failures = []
    with tempfile.TemporaryDirectory() as tmp:
        for name, before, after, test in CONTROLS:
            copy = Path(tmp) / name
            shutil.copytree(SOURCE, copy, ignore=shutil.ignore_patterns("__pycache__"))
            if run_test(copy, test) != 0:
                failures.append(f"{name}: the unmutated test {test} does not pass")
                continue
            target = copy / "commons.py"
            text = target.read_text()
            if text.count(before) != 1:
                failures.append(f"{name}: the mutation site is missing or ambiguous")
                continue
            target.write_text(text.replace(before, after))
            caught = run_test(copy, test) != 0
            print(f"{name}: {'caught by ' + test if caught else 'NOT CAUGHT'}")
            if not caught:
                failures.append(f"{name}: {test} still passes with the mutation")
    for failure in failures:
        print("FAIL", failure)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
