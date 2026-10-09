#!/usr/bin/env python3
"""Compile target migration claims and exercise the actual generated SQLite lineage.

Builds the supported spend-approval witness from MIGRATION_CLAIMS_BOUNDARY.md:
A has urgent=true genesis and law 504 preserving it; B has urgent=false genesis;
C adds field 125 and inductive claim 600 that urgent stays false. The generated
application's focused Rust test checks refused mixed histories, atomic multihop
refusal, a successful upgrade/audit/continuation, and old-admission replay.
Use --keep to retain all generated sources and the old-admission SQLite fixture.
This command invokes Cargo; it is a focused integration gate, not a syntax check.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib

import check_contract_upgrade as upgrades
import check_generated_application as applications

ROOT = applications.ROOT
FIXTURES = ROOT / "crates/zeno-fcis-cli/tests/fixtures"
PRIORITY = FIXTURES / "spend-approval-priority"
LAW = "\nlaw 504 urgent_is_preserved on commit = post.100.124 == pre.100.124;\n"
CLAIM = "claim 600 urgent_stays_false all inductive assume [504] = pre.100.124 == 0;\n"


def declarations(directory: Path, *, urgent: bool, extended: bool) -> Path:
    project = (PRIORITY / "project.zeno").read_text() + LAW
    policy = json.loads((PRIORITY / "v2/policy.json").read_text())
    policy["law_kinds"]["504"] = "AssetConservation"
    policy["genesis"]["124"] = urgent
    if extended:
        project = project.replace("field 124 100 urgent 108;",
                                  "field 124 100 urgent 108;\nfield 125 100 escalated 108;") + CLAIM
        policy["variables"]["escalated"] = "pre.100.125"
        policy["genesis"]["125"] = False
        for case in policy["cases"]:
            if "124" in case["post"]:
                case["post"]["125"] = "escalated"
    (directory / "v2").mkdir(parents=True)
    (directory / "tests").mkdir()
    (directory / "project.zeno").write_text(project)
    (directory / "v2/policy.json").write_text(json.dumps(policy, indent=2) + "\n")
    # The focused Rust test supplies publications under each exact catalog.
    (directory / "tests/decision-examples.txt").write_text(
        "# Checked migration-claim publications are in tests/migration_claims.rs.\n")
    return directory


def zeno(arguments: list[str]) -> dict:
    completed = subprocess.run([upgrades.cli(), *arguments], text=True, check=False,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return upgrades.report(completed, " ".join(arguments[:2]))


def check(directory: Path) -> dict:
    applications.run(["cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"], ROOT)
    a = declarations(directory / "a", urgent=True, extended=False)
    b = declarations(directory / "b", urgent=False, extended=False)
    c = declarations(directory / "c", urgent=False, extended=True)
    migration = directory / "migration.json"
    fields = {str(field): {"from": field} for field in range(120, 125)}
    fields["125"] = {"default": False}
    migration.write_text(json.dumps({"schema": "zeno-fcis/migration/1", "state": fields}, indent=2) + "\n")
    app = directory / "app"
    applications.run([upgrades.cli(), "new", str(app), "--contract", str(a)], ROOT)
    first = zeno(["contract", "evolve", str(app), "--to", str(b), "--format", "json"])
    upgrades.expect(first, {"kind": "rule-change"}, "A -> B classification")
    second = zeno(["contract", "evolve", str(app), "--to", str(c),
                   "--migration", str(migration), "--format", "json"])
    upgrades.expect(second, {"kind": "layout-change"}, "B -> C classification")
    upgrades.expect(second["evolution"], {"path": "migration", "claims": [600]}, "compiled target claim")
    checked = zeno(["generate", "contract", str(app), "--check", "--format", "json"])
    if checked.get("drift") != []:
        raise RuntimeError(f"generated source drift: {checked}")
    upgrades.expect(checked["summary"]["evolutions"][-1], {"claims": [600]}, "retained target claim")
    shutil.copyfile(FIXTURES / "migration-claims/runtime.rs", app / "tests/migration_claims.rs")
    packages = {tomllib.loads(manifest.read_text())["package"]["name"]: manifest.parent
                for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml"))}
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    upgrades.prepare(app, packages, version)
    artifacts = directory / "stores"
    artifacts.mkdir()
    os.environ["MIGRATION_CLAIMS_ARTIFACTS"] = str(artifacts)
    applications.run(["cargo", "+1.97.1", "test", "--locked", "--offline", "--test", "migration_claims",
                      "--", "--test-threads=1"], app)
    return {"schema": "zeno-fcis/migration-claims-check/1", "status": "passed", "authority": "none",
            "claims": [600], "old_admission_fixture": str(artifacts / "old-admission-violating.sqlite"),
            "checks": ["generated-membership", "mixed-history-refusal", "multihop-no-write",
                       "valid-upgrade-audit-continue", "old-admission-replay-refusal"]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--keep", type=Path, help="retain all artifacts in a new directory")
    args = parser.parse_args()
    os.environ.setdefault("CARGO_BUILD_JOBS", "2")
    os.environ.setdefault("CARGO_INCREMENTAL", "0")
    os.environ.setdefault("CARGO_PROFILE_DEV_DEBUG", "0")
    os.environ.setdefault("CARGO_PROFILE_TEST_DEBUG", "0")
    os.environ.setdefault("CARGO_TARGET_DIR", str(ROOT / "target"))
    if args.keep:
        directory = args.keep.resolve()
        directory.mkdir()
        result = check(directory)
    else:
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-migration-claims-") as path:
            result = check(Path(path))
            result["old_admission_fixture"] = "temporary; pass --keep to retain"
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError, KeyError, ValueError) as error:
        print(f"migration claims: FAIL: {error}", file=sys.stderr)
        sys.exit(1)
