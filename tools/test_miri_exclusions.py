"""Planted controls and the repository check for the pinned Miri exclusions."""

from __future__ import annotations

import unittest

import miri_exclusions


class MiriExclusionTests(unittest.TestCase):
    def test_planted_controls_are_refused(self) -> None:
        miri_exclusions.self_test()

    def test_repository_exclusions_match_the_workflow(self) -> None:
        rows = miri_exclusions.workflow_rows(miri_exclusions.WORKFLOW.read_text(encoding="utf-8"))
        miri_exclusions.check_static(miri_exclusions.ROOT, miri_exclusions.load_exclusions(), rows)


if __name__ == "__main__":
    unittest.main()
