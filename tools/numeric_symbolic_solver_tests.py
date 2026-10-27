#!/usr/bin/env python3
"""Run the pinned-solver numeric review tests when CVC5 and Z3 are configured.
The tests under crates/zeno-fcis-cli/src/contract/review/numeric_symbolic/ that are
ignored as requiring both qualification solver binaries need the pinned CVC5 and Z3
named by ZENO_FCIS_CVC5 and ZENO_FCIS_Z3. With both set, every one of them must run
and pass. Without them this prints the SKIP line documented in docs/SYMBOLIC_CHECKS.md
and passes, as tools/symbolic_solver_tests.py does for the symbolic CLI tests.
"""
from __future__ import annotations

import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
TESTS = ROOT / "crates" / "zeno-fcis-cli" / "src" / "contract" / "review" / "numeric_symbolic"
SKIP = (
    "atdd: SKIP pinned numeric review solver tests: ZENO_FCIS_CVC5 and ZENO_FCIS_Z3 "
    "are not both set (docs/SYMBOLIC_CHECKS.md)"
)
COMMAND = (
    "cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked",
    "contract::review::numeric_symbolic::", "--", "--ignored", "--test-threads=1",
)
PASSED = re.compile(r"^test result: ok\. (\d+) passed;", re.MULTILINE)
PINNED = re.compile(r'^#\[ignore = "requires both qualification solver binaries"\]$', re.MULTILINE)


def configured(environment: dict[str, str]) -> bool:
    """Whether both pinned solvers are named."""
    return bool(environment.get("ZENO_FCIS_CVC5")) and bool(environment.get("ZENO_FCIS_Z3"))


def expected_tests() -> int:
    """The number of solver-gated tests the review sources declare."""
    return sum(len(PINNED.findall(path.read_text(encoding="utf-8")))
               for path in sorted(TESTS.rglob("*.rs")))


def passed(output: str) -> int:
    """The tests cargo reports as passed."""
    return sum(int(count) for count in PASSED.findall(output))


def main() -> int:
    if not configured(dict(os.environ)):
        print(SKIP, flush=True)
        return 0
    expected = expected_tests()
    if expected == 0:
        print("numeric-symbolic-solver-tests: no solver-gated tests are declared", file=sys.stderr)
        return 1
    completed = subprocess.run(COMMAND, cwd=ROOT, stdout=subprocess.PIPE, text=True, check=False)
    sys.stdout.write(completed.stdout)
    sys.stdout.flush()
    if completed.returncode != 0:
        return completed.returncode
    count = passed(completed.stdout)
    if count != expected:
        print(f"numeric-symbolic-solver-tests: {count} pinned tests passed, {expected} declared",
              file=sys.stderr)
        return 1
    print(f"atdd: pinned numeric review solver tests PASS ({count} tests)", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
