#!/usr/bin/env python3
"""Upgrade a live withdrawal-queue store through the application command line.

The withdrawal-queue template is built as an isolated package and its journey
run, which leaves a store under contract version 1 at the genesis state with
twelve commits and both payouts delivered. An application is then built from
the adopted withdrawal-queue contract (version 2, the 100-node candidate) and
upgrades that store with `--upgrade`, audits it with `--audit`, and is refused
when it tries to audit before upgrading or to upgrade twice. Dependencies
resolve exactly as in check_generated_application.py: only the new consumer
locks change, against the reviewed external graph.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

import check_generated_application as applications

ROOT = applications.ROOT
FIXTURE = ROOT / "crates/zeno-fcis-cli/tests/fixtures/withdrawal-queue-adopted"
EXPECTED_JOURNEY = {"status": "passed", "bundles": 12, "pending": 0, "deliveries": 2, "balance": 0}


def cli() -> str:
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    return str((ROOT / target / "debug/zeno-fcis").resolve())


def prepare(app: Path, packages: dict[str, Path], version: str) -> dict:
    """Bind the generated manifest to this checkout and admit its resolved graph."""
    applications.bind_generated_dependencies(app / "Cargo.toml", packages, version)
    allowed = {(name, version): path / "Cargo.toml" for name, path in packages.items()}
    consumer = tomllib.loads((app / "Cargo.toml").read_text())["package"]
    allowed[(consumer["name"], consumer["version"])] = app / "Cargo.toml"
    return applications.resolve_reviewed_graph(app, allowed, dict(os.environ))


def run_app(app: Path, arguments: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["cargo", "+1.97.1", "run", "--locked", "--offline", "--", *arguments],
        cwd=app, check=False, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )


def expect_refusal(completed: subprocess.CompletedProcess[str], word: str, label: str) -> None:
    if completed.returncode != 1 or word not in completed.stderr:
        raise RuntimeError(f"{label}: expected a refusal naming {word}: "
                           f"exit {completed.returncode}, stderr {completed.stderr.strip()!r}")


def check(directory: Path) -> dict:
    applications.run(["cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"], ROOT)
    packages = {tomllib.loads(manifest.read_text())["package"]["name"]: manifest.parent
                for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml"))}
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]

    template = directory / "withdrawal-queue"
    applications.run([cli(), "new", str(template), "--template", "withdrawal-queue"], ROOT)
    template_graph = prepare(template, packages, version)
    store = directory / "vault.sqlite"
    journey = run_app(template, [str(store)])
    if journey.returncode != 0:
        raise RuntimeError(f"journey failed: {journey.stderr}")
    summary = json.loads(journey.stdout)
    if any(summary.get(key) != value for key, value in EXPECTED_JOURNEY.items()):
        raise RuntimeError(f"journey differs from the expected store: {summary}")

    adopted = directory / "adopted"
    applications.run([cli(), "new", str(adopted), "--contract", str(FIXTURE)], ROOT)
    applications.run([cli(), "generate", "contract", str(adopted), "--check"], ROOT)
    adopted_graph = prepare(adopted, packages, version)
    # Version 2 alone cannot replay a store that ran version 1 without the record.
    expect_refusal(run_app(adopted, ["--audit", str(store)]), "Identity", "audit before upgrade")
    upgraded = run_app(adopted, ["--upgrade", str(store)])
    if upgraded.returncode != 0:
        raise RuntimeError(f"upgrade failed: {upgraded.stderr}")
    upgraded = json.loads(upgraded.stdout)
    expected = {"status": "upgraded", "ordinal": 1, "at_commit": 12, "contract_version": 2,
                "commits": 12, "pending": 0, "upgrades": 1}
    if any(upgraded.get(key) != value for key, value in expected.items()):
        raise RuntimeError(f"upgrade report differs: {upgraded}")
    audited = run_app(adopted, ["--audit", str(store)])
    if audited.returncode != 0:
        raise RuntimeError(f"audit failed: {audited.stderr}")
    audited = json.loads(audited.stdout)
    if audited != {"status": "audited", "contract_version": 2, "commits": 12, "pending": 0, "upgrades": 1}:
        raise RuntimeError(f"audit report differs: {audited}")
    expect_refusal(run_app(adopted, ["--upgrade", str(store)]), "SameContract", "second upgrade")
    expect_refusal(run_app(adopted, ["--migrate", str(store)]), "Schema(10)", "migrating a v10 store")
    # The version 1 build only creates stores; an existing one is refused.
    expect_refusal(run_app(template, [str(store)]), "Schema", "version 1 over the upgraded store")
    return {"schema": "zeno-fcis/contract-upgrade-check/1", "status": "passed", "authority": "none",
            "journey": summary, "upgrade": upgraded, "audit": audited,
            "template_graph_lock_sha256": template_graph["lock_sha256"],
            "adopted_graph_lock_sha256": adopted_graph["lock_sha256"]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--keep", type=Path, help="retain artifacts in a new directory")
    args = parser.parse_args()
    os.environ.setdefault("CARGO_BUILD_JOBS", "2")
    os.environ.setdefault("CARGO_INCREMENTAL", "0")
    os.environ.setdefault("CARGO_PROFILE_DEV_DEBUG", "0")
    os.environ.setdefault("CARGO_PROFILE_TEST_DEBUG", "0")
    os.environ.setdefault("CARGO_TARGET_DIR", str(ROOT / "target"))
    if args.keep:
        directory = args.keep.resolve()
        directory.mkdir()
        report = check(directory)
    else:
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-contract-upgrade-") as directory:
            report = check(Path(directory))
    print(json.dumps(report, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
