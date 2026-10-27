#!/usr/bin/env python3
"""Planted controls for Zeno Commons: each mutation must make its named test fail.

Copies integrations/commons to a temporary directory, applies one mutation at a time, and runs the named
test. Exit 0 only if every mutation is caught and the unmutated copy passes the same tests.
"""

from __future__ import annotations

import re
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
    ("archive-drops-unread", "                \"DELETE FROM messages WHERE seq <= ? AND (recipient = '*' \"",
     "                \"DELETE FROM messages WHERE seq <= ? OR (recipient = '*' \"",
     "CommonsTests.test_archive_keeps_unread_messages_and_drops_read_ones"),
    ("archive-past-the-keep-window", "        cut = top - keep_events\n", "        cut = top - keep_events + 1\n",
     "CommonsTests.test_archive_keeps_the_newest_events_and_the_whole_history_still_verifies"),
    ("archive-skips-chain-check",
     "        if not checked[\"ok\"]:\n            raise CommonsError(f\"the event chain is broken at {checked['broken_at']}; "
     "archive refused\")\n        head = checked[\"head\"]",
     "        head = rows[-1][7]",
     "CommonsTests.test_archive_refuses_a_broken_chain_and_writes_nothing"),
    ("record-grows-past-limit", "            if len(new_body) > MAX_TEXT:\n", "            if False:\n",
     "CommonsTests.test_record_title_correction_and_size_cap"),
    ("stale-lease-frees-resource",
     "                if not force_reason.strip():\n                    raise CommonsError(f\"{resource} overlaps {held}: {owner}'s lease went stale",
     "                if False:\n                    raise CommonsError(f\"{resource} overlaps {held}: {owner}'s lease went stale",
     "CommonsTests.test_lease_conflict_expiry_renew_release"),
    ("lease-check-hides-stale",
     "        return [self._lease_view(*row, at) for row in self._unreleased_leases(self.db) if _covers(row[0], resource)]",
     "        return [self._lease_view(*row, at) for row in self._unreleased_leases(self.db) if _covers(row[0], resource) and row[3] > at]",
     "CommonsTests.test_lease_conflict_expiry_renew_release"),
    ("anchors-ignored", "            if seq >= start and seen.get(seq) != digest:", "            if False:",
     "CommonsTests.test_anchors_detect_a_rewritten_history_and_removed_events"),
    ("foreign-schema-accepted", "        if foreign:\n", "        if False:\n",
     "CommonsTests.test_the_store_refuses_links_and_foreign_schemas_before_writing"),
    ("duplicate-keeps-lower-severity", "        level = min((SEVERITIES.index(t[9:])", "        level = max((SEVERITIES.index(t[9:])",
     "CommonsTests.test_a_duplicate_gap_folds_into_its_canonical_gap"),
    ("duplicate-stays-open",
     "        cursor.execute(\"UPDATE records SET status='superseded', updated_seq=? WHERE id=?\", (seq, src))\n",
     "", "CommonsTests.test_a_duplicate_gap_folds_into_its_canonical_gap"),
    ("reporters-not-counted", "            ranked.append((rank, -len(reporters), record[\"created_seq\"], {",
     "            ranked.append((rank, 0, record[\"created_seq\"], {",
     "CommonsTests.test_gaps_rank_by_severity_then_reporters_then_age"),
    ("unregistered-writes", "        if not cursor.execute(\"SELECT 1 FROM identities WHERE name=?\", (self.actor,)).fetchone():\n            raise CommonsError(f\"{self.actor} is not registered; run register first so others can address you\")",
     "        pass", "CommonsTests.test_unregistered_actors_cannot_coordinate_but_tools_can_emit"),
]


def run_test(directory: Path, test: str) -> str:
    """'pass', 'fail' (an assertion refuted the mutant) or 'error' (it crashed, which does not count as caught)."""
    done = subprocess.run(["nice", "-n", "19", "ionice", "-c3", sys.executable, "-m", "unittest",
                           f"test_commons.{test}"], cwd=directory,
                          capture_output=True, text=True, timeout=300)
    if done.returncode == 0:
        return "pass"
    return "fail" if re.search(r"FAILED \(failures=\d+", done.stderr) else "error"


def main() -> int:
    failures = []
    with tempfile.TemporaryDirectory() as tmp:
        for name, before, after, test in CONTROLS:
            copy = Path(tmp) / name
            shutil.copytree(SOURCE, copy, ignore=shutil.ignore_patterns("__pycache__"))
            if run_test(copy, test) != "pass":
                failures.append(f"{name}: the unmutated test {test} does not pass")
                continue
            target = copy / "commons.py"
            text = target.read_text()
            if text.count(before) != 1:
                failures.append(f"{name}: the mutation site is missing or ambiguous")
                continue
            target.write_text(text.replace(before, after))
            verdict = run_test(copy, test)
            print(f"{name}: {'caught by ' + test if verdict == 'fail' else 'NOT CAUGHT (' + verdict + ')'}")
            if verdict != "fail":
                failures.append(f"{name}: {test} gives {verdict} with the mutation, not a failed assertion")
    for failure in failures:
        print("FAIL", failure)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
