#!/usr/bin/env python3
"""Compile the umbrella crate's API custody fixtures and require their exact outcome.

Each file in crates/zeno-fcis/tests/ui is a standalone program compiled against
`zeno_fcis` built with all features, so an optional feature cannot reopen a
retired or private route. A refusal fixture passes only when rustc rejects it
with its expected error code; a positive fixture must compile. Any fixture
without an expectation, or any expectation without a fixture, fails the check.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "crates/zeno-fcis/tests/ui"
TOOLCHAIN = "+1.97.1"

# Expected rustc error code per fixture; None means the fixture must compile.
EXPECTED: dict[str, str | None] = {
    # Positive routes: the normal checked program API and the inert legacy data.
    "normal_positive": None,
    "legacy_oracle_positive": None,
    # Retired native routes cannot be named from the root, modules or preludes.
    "root_budget": "E0432",
    "root_candidate_sealer": "E0432",
    "root_claimed_usage": "E0432",
    "root_delivery_interpreter": "E0432",
    "root_law_engine": "E0432",
    "root_native_trait": "E0432",
    "module_budget": "E0432",
    "module_domain_machine": "E0432",
    "module_native_program": "E0432",
    "module_sealer": "E0432",
    "module_transition": "E0432",
    "module_legacy_synthesis": "E0433",
    "legacy_budget_retired": "E0432",
    "legacy_core_budget": "E0432",
    # The legacy re-export of the receipt crate still hides the candidate sealer.
    "legacy_candidate_sealer": "E0603",
    "prelude_budget": "E0433",
    # Capabilities cannot be forged, copied or given caller-chosen contents.
    "private_program": "E0451",
    "private_publication": "E0451",
    "private_genesis": "E0451",
    "private_usage": "E0451",
    "program_not_clone": "E0277",
    "publication_not_clone": "E0277",
    "genesis_not_clone": "E0277",
    "no_claimed_invocation_usage": "E0560",
    "no_supplied_identity": "E0061",
    # No callback, raw value, raw pre-state or mutable descriptor reaches the route.
    "no_native_callback": "E0308",
    "no_raw_value": "E0308",
    "no_raw_prestate_getter": "E0599",
    "no_mutable_descriptor": "E0308",
    # Protocol enums stay open to additions.
    "resource_non_exhaustive": "E0004",
}


def build(target: Path) -> tuple[Path, Path]:
    command = ["cargo", TOOLCHAIN, "build", "--locked", "--offline", "-p", "zeno-fcis",
               "--all-features", "--message-format=json"]
    environment = {**os.environ, "CARGO_TARGET_DIR": str(target)}
    result = subprocess.run(command, cwd=ROOT, env=environment, capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(f"zeno-fcis build failed:\n{result.stderr[-4000:]}")
    library = None
    for line in result.stdout.splitlines():
        try:
            item = json.loads(line)
        except json.JSONDecodeError:
            continue
        if item.get("reason") == "compiler-artifact" and item["target"]["name"] == "zeno_fcis":
            library = next((Path(p) for p in item["filenames"] if p.endswith(".rlib")), library)
    if library is None:
        raise RuntimeError("zeno_fcis rlib not reported by cargo")
    return library, target / "debug/deps"


def compile_fixture(fixture: Path, library: Path, dependencies: Path, out: Path) -> tuple[int, list[str], str]:
    command = ["rustc", TOOLCHAIN, "--edition=2024", "--crate-type=bin", "--error-format=json",
               "--extern", f"zeno_fcis={library}", "-L", f"dependency={dependencies}",
               str(fixture), "-o", str(out / fixture.stem)]
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
    codes = []
    for line in result.stderr.splitlines():
        try:
            item = json.loads(line)
        except json.JSONDecodeError:
            continue
        if item.get("level") == "error" and item.get("code"):
            codes.append(item["code"]["code"])
    return result.returncode, codes, result.stderr


def check(target: Path, discover: bool) -> int:
    fixtures = sorted(p for p in FIXTURES.glob("*.rs"))
    names = {p.stem for p in fixtures}
    if not discover and names != set(EXPECTED):
        missing = sorted(names - set(EXPECTED))
        stale = sorted(set(EXPECTED) - names)
        print(f"api-refusals: FAIL: unlisted fixtures {missing}, listed without fixture {stale}", file=sys.stderr)
        return 1
    library, dependencies = build(target)
    failures = []
    with tempfile.TemporaryDirectory(prefix="zeno-fcis-api-refusals-") as directory:
        for fixture in fixtures:
            code, codes, stderr = compile_fixture(fixture, library, dependencies, Path(directory))
            if discover:
                first = next((json.loads(l)["message"] for l in stderr.splitlines()
                              if l.startswith("{") and json.loads(l).get("level") == "error"), "")
                print(f"{fixture.stem}\texit={code}\tcodes={codes}\t{first[:140]}")
                continue
            expected = EXPECTED[fixture.stem]
            if expected is None:
                if code != 0:
                    failures.append(f"{fixture.stem}: positive fixture did not compile: {codes}")
            elif code == 0:
                failures.append(f"{fixture.stem}: compiled, expected {expected}")
            elif codes[:1] != [expected]:
                failures.append(f"{fixture.stem}: first error {codes[:1]}, expected {expected}")
    if discover:
        return 0
    if failures:
        for failure in failures:
            print(f"api-refusals: FAIL: {failure}", file=sys.stderr)
        return 1
    refusals = sum(value is not None for value in EXPECTED.values())
    print(f"api-refusals: PASS ({refusals} refused with their expected codes, "
          f"{len(EXPECTED) - refusals} positive fixtures compiled, all features)")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--discover", action="store_true", help="print each fixture's outcome; checks nothing")
    arguments = parser.parse_args()
    target = Path(os.environ.get("CARGO_TARGET_DIR", str(ROOT / "target")))
    return check(target, arguments.discover)


if __name__ == "__main__":
    raise SystemExit(main())
