#!/usr/bin/env python3
"""Run the pinned-solver symbolic tests when CVC5 and Z3 are configured.

The `pinned_symbolic_*` tests in crates/zeno-fcis-cli/tests/symbolic_cli.rs
need the pinned CVC5 and Z3 named by ZENO_FCIS_CVC5 and ZENO_FCIS_Z3. With
both set, every one of them must run and pass. Without them this prints the
SKIP line documented in docs/SYMBOLIC_CHECKS.md and passes.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TESTS = ROOT / "crates" / "zeno-fcis-cli" / "tests" / "symbolic_cli.rs"
SKIP = (
    "atdd: SKIP pinned symbolic solver tests: ZENO_FCIS_CVC5 and ZENO_FCIS_Z3 "
    "are not both set (docs/SYMBOLIC_CHECKS.md)"
)
COMMAND = (
    "cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--test", "symbolic_cli",
    "pinned_symbolic_", "--", "--ignored",
)
PASSED = re.compile(r"^test result: ok\. (\d+) passed;", re.MULTILINE)
PINNED = re.compile(r"^fn (pinned_symbolic_[a-z0-9_]+)\(", re.MULTILINE)


def configured(environment: dict[str, str]) -> bool:
    """Whether both pinned solvers are named."""
    return bool(environment.get("ZENO_FCIS_CVC5")) and bool(environment.get("ZENO_FCIS_Z3"))


def expected_tests() -> int:
    """The number of pinned tests the test file declares."""
    return len(PINNED.findall(TESTS.read_text(encoding="utf-8")))


def passed(output: str) -> int:
    """The tests cargo reports as passed."""
    return sum(int(count) for count in PASSED.findall(output))


def main() -> int:
    if not configured(dict(os.environ)):
        print(SKIP, flush=True)
        return 0
    expected = expected_tests()
    if expected == 0:
        print("symbolic-solver-tests: no pinned_symbolic_ tests are declared", file=sys.stderr)
        return 1
    completed = subprocess.run(COMMAND, cwd=ROOT, stdout=subprocess.PIPE, text=True, check=False)
    sys.stdout.write(completed.stdout)
    sys.stdout.flush()
    if completed.returncode != 0:
        return completed.returncode
    count = passed(completed.stdout)
    if count != expected:
        print(f"symbolic-solver-tests: {count} pinned tests passed, {expected} declared", file=sys.stderr)
        return 1
    print(f"atdd: pinned symbolic solver tests PASS ({count} tests)", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
