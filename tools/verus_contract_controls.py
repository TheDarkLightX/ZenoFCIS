"""Classify contract-only mutation controls rejected by verified production callers.

Omitting, weakening or narrowing a ghost contract leaves runtime code unchanged.
Once verified production callers depend on that contract, whole-crate Verus
must reject those callers. Accept only a completed, pinned, whole-crate
semantic failure whose obligation diagnostics are exactly the ones demonstrated
for that control. Inconclusive solver or frontend outcomes refuse first, even
when an expected-looking diagnostic also appears. This is a development
classifier for retained evidence, not an additional proof checker.
"""
from __future__ import annotations

import json
import re

ERROR = re.compile(r"error(?:\[E\d+\])?: (.+)$")
LOCATION = re.compile(r"^\s*--> (?:.*/)?(crates/[\w./-]+\.rs):(\d+):\d+\s*$")
INCONCLUSIVE = re.compile(
    r"resource limit|rlimit|timed out|timeout|out of memory|memory allocation|unknown"
    r"|internal error|unsupported|panicked|error\[E\d+\]", re.IGNORECASE)


def diagnostics(stderr: str) -> list[tuple[str, str, int]]:
    """Return (diagnostic, repository path, line) for each reported error, in order."""
    lines = stderr.splitlines()
    found = []
    for index, line in enumerate(lines):
        match = ERROR.match(line)
        if match is None or re.fullmatch(r"aborting due to .* previous errors?", match.group(1)):
            continue
        location = next((LOCATION.match(row) for row in lines[index + 1:index + 3] if LOCATION.match(row)), None)
        found.append((match.group(1).strip(), location.group(1) if location else "",
                      int(location.group(2)) if location else 0))
    return found


def contract_rejected(returncode: int | None, stdout: str, stderr: str, pin: dict,
                      expected: frozenset[tuple[str, str, int]]) -> dict:
    """Return the outcome; only the demonstrated caller rejection is accepted."""
    observed = diagnostics(stderr)
    try:
        report = json.loads(stdout)
    except ValueError:
        report = None
    results = report.get("verification-results") if isinstance(report, dict) else None
    verus = report.get("verus") if isinstance(report, dict) else None
    if not isinstance(results, dict) or not isinstance(verus, dict):
        category = "missing or malformed verifier report"
    elif type(returncode) is not int or returncode <= 0:
        category = "verifier did not complete with a failure"
    elif INCONCLUSIVE.search(stderr):
        category = "inconclusive or frontend failure"
    elif verus.get("version") != pin["version"] or verus.get("commit") != pin["commit"]:
        category = "unpinned verifier"
    elif not (results.get("success") is False and type(results.get("errors")) is int
              and results["errors"] > 0 and results.get("encountered-vir-error") is False
              and results.get("is-verifying-entire-crate") is True):
        category = "not a whole-crate semantic failure"
    elif not observed:
        category = "no obligation diagnostic"
    elif set(observed) != expected:
        category = "diagnostics differ from the demonstrated caller rejection"
    else:
        category = "verified caller rejected the changed contract"
    return {"accepted": category == "verified caller rejected the changed contract",
            "category": category, "observed": [list(row) for row in observed],
            "expected": sorted(list(row) for row in expected), "exit_code": returncode,
            "verifier": {key: verus.get(key) for key in ("version", "commit")} if isinstance(verus, dict) else None,
            "whole_crate": results.get("is-verifying-entire-crate") if isinstance(results, dict) else None}
