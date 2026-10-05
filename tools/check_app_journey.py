#!/usr/bin/env python3
"""Build, optimize, adopt and upgrade applications through their command lines alone.

Each part runs in a new directory outside the repository, with the CLI this
checkout builds and the applications `zeno-fcis new` writes, and with no
other script:

1. README: `zeno-fcis new escrow --contract <fixture>` builds the app study's
   original escrow, whose split pays out in two deliveries with idempotency
   ordinals 0 and 1. The commands of the generated README's "Build and run"
   section, read from the README and run exactly as written, then build it,
   check its decision examples and run its session.
2. Upgrade: `contract export-program`, `optimize`, `transform replay` and
   `contract adopt --usage new-version` make version 2 of the app study's
   spend-approval contract. A version 1 store with four commits and a
   pending payment upgrades as a program successor, at commit 4, and
   delivers the payment under the identifier version 1's commit bound; the
   version 1 build then refuses the store. A second version 1 store, away
   from genesis after two commits, upgrades at commit 2 and keeps
   committing under version 2 with `--decide`; its audit replays both
   segments. A store at another version refuses `--decide` and keeps its
   bytes.
3. Release build: a CLI built as `tools/rc_package.py build` builds its
   release binaries, with paths remapped and `ZENO_FCIS_BUILD_TREE` empty,
   holds no path of this checkout, and its `new` refuses without `--source`
   and binds with it. The same build without the variable is the control:
   it does hold the checkout's path.

The applications pin this checkout through the binding `new` wrote, which
`check_generated_application.check_generated_binding` checks; only each new
consumer's lock changes, against the reviewed external graph.
"""

from __future__ import annotations

import argparse
import json
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

import check_contract_upgrade as upgrades
import check_generated_application as applications
import check_release_privacy
import rc_package

ROOT = applications.ROOT
FIXTURES = ROOT / "crates/zeno-fcis-cli/tests/fixtures"
ESCROW = FIXTURES / "escrow"
SPEND = FIXTURES / "spend-approval"
# The escrow session: the journey's rejects, funding, shipping, a dispute and
# the split that pays the buyer and the seller in two deliveries.
ESCROW_SUMMARY = {
    "status": "passed",
    "decisions": ["Reject", "Reject", "Reject", "Accept", "Reject", "Reject", "Reject", "Reject",
                  "Accept", "Reject", "Reject", "Accept", "Reject", "Reject", "Accept", "Reject"],
    "bundles": 4, "pending": 0, "deliveries": 2,
}
# The first six examples of the spend-approval fixture: a tier 3 request is
# created and the CFO approves it, so the store ends away from genesis.
SPEND_SESSION = """\
# A tier 3 request, created and approved by the CFO.
150 0 0 0 170 3 160 0 1 | reject 200 150 0 0 0 | -
150 0 0 0 170 3 162 0 1 | reject 202 150 0 0 0 | -
150 0 0 0 170 3 161 0 1 | accept - 151 3 0 0 | -
151 3 0 0 171 0 161 0 1 | reject 202 151 3 0 0 | -
151 3 0 0 172 0 161 1 1 | reject 205 151 3 0 0 | -
151 3 0 0 171 0 162 0 1 | accept - 151 3 1 0 | -
"""


def readme_commands(app: Path) -> list[list[str]]:
    """The commands of the `sh` block in the generated README's "Build and run" section."""
    readme = (app / "README.md").read_text()
    if readme.count("\n## Build and run\n") != 1:
        raise RuntimeError("generated README has no single Build and run section")
    section = readme.split("\n## Build and run\n", 1)[1].split("\n## ", 1)[0]
    if section.count("```sh\n") != 1:
        raise RuntimeError("generated README's Build and run section has no single sh block")
    block = section.split("```sh\n", 1)[1].split("```", 1)[0]
    return [shlex.split(line) for line in block.splitlines() if line.strip()]


def as_written(command: list[str], database: Path) -> list[str]:
    return [str(database) if word == "NEW_DATABASE_PATH" else word for word in command]


def holds(path: Path, marker: str) -> bool:
    """Whether the release privacy scan, given `marker` as a private
    identifier, finds it in the file."""
    scan = check_release_privacy.Scan(private_markers=(marker.encode(),))
    scan.file(path, path.name)
    report = scan.report()
    if report["errors"]:
        raise RuntimeError(f"privacy scan could not read {path.name}: {report['errors']}")
    return any(finding["code"] == "private-marker" for finding in report["findings"])


def cli() -> str:
    return upgrades.cli()


def zeno(arguments: list[str]) -> dict:
    """One `zeno-fcis` command that must succeed and print a JSON report."""
    completed = subprocess.run([cli(), *arguments], check=False, text=True,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return upgrades.report(completed, " ".join(arguments[:2]))


def refused_without_write(binary: Path, arguments: list[str], store: Path, word: str,
                          label: str) -> None:
    """The command refuses, naming `word`, and leaves the store's bytes as they were."""
    before = store.read_bytes()
    upgrades.expect_refusal(upgrades.run_app(binary, arguments), word, label)
    if store.read_bytes() != before:
        raise RuntimeError(f"{label}: the refusal changed the store")


def readme_journey(directory: Path, packages: dict[str, Path], version: str) -> dict:
    root = directory / "readme"
    root.mkdir()
    applications.run([cli(), "new", "escrow", "--contract", str(ESCROW)], root)
    app = root / "escrow"
    binding = applications.check_generated_binding(app, packages, version)
    commands = readme_commands(app)
    if [command[:2] for command in commands] != [["cargo", "test"], ["cargo", "run"]]:
        raise RuntimeError(f"the README's steps are not a cargo test and a cargo run: {commands}")
    # The application's rust-toolchain.toml selects the toolchain, as for
    # anyone who follows the README in a new shell.
    environment = {key: value for key, value in os.environ.items() if key != "RUSTUP_TOOLCHAIN"}
    database = root / "escrow.sqlite"
    outputs = [applications.run(as_written(command, database), app, capture=True,
                                environment=environment)
               for command in commands]
    print(outputs[0], end="")
    if "test every_example_is_the_authority_decision ... ok" not in outputs[0]:
        raise RuntimeError("cargo test did not check the decision examples")
    summary = json.loads(outputs[1].strip().splitlines()[-1])
    upgrades.expect(summary, ESCROW_SUMMARY, "escrow session")
    return {"commands": commands, "binding": binding, "summary": summary}


def application(root: Path, name: str, contract: Path, packages: dict[str, Path],
                version: str) -> Path:
    """Creates an application and builds it at once; see `check_contract_upgrade.build_app`."""
    app = root / name
    applications.run([cli(), "new", str(app), "--contract", str(contract)], ROOT)
    upgrades.prepare(app, packages, version)
    return upgrades.build_app(app)


def upgrade_journey(directory: Path, packages: dict[str, Path], version: str) -> dict:
    root = directory / "spend-approval"
    root.mkdir()
    full, partial, adopted = (root / name for name in ("contract-v1", "contract-v1-session",
                                                         "contract-v2"))
    for contract in (full, partial, adopted):
        shutil.copytree(SPEND, contract)
    (partial / "tests/decision-examples.txt").write_text(SPEND_SESSION)
    program, candidate, receipt = root / "program.zcve", root / "candidate.zcve", root / "receipt.json"
    exported = zeno(["contract", "export-program", str(adopted), "--out", str(program),
                     "--format", "json"])
    upgrades.expect(exported, {"status": "exported", "version": 1}, "export-program")
    optimized = zeno(["optimize", "--program", str(program), "--candidate-out", str(candidate),
                      "--receipt", str(receipt)])
    upgrades.expect(optimized, {"status": "improved"}, "optimize")
    replayed = zeno(["transform", "replay", "--receipt", str(receipt), "--original", str(program),
                     "--candidate", str(candidate)])
    upgrades.expect(replayed, {"status": "replayed"}, "transform replay")
    adoption = zeno(["contract", "adopt", str(adopted), "--candidate", str(candidate), "--receipt",
                     str(receipt), "--usage", "new-version", "--format", "json"])["adoption"]
    upgrades.expect(adoption, {"version": 2, "usage": "new-version",
                               "program_nodes": {"before": exported["program"]["nodes"],
                                                 "after": optimized["detail"]["best"]["nodes"]}},
                    "contract adopt")

    # One package name, one target directory: each application is built
    # right after it is created.
    old = application(root, "version-1", full, packages, version)
    old_session = application(root, "version-1-session", partial, packages, version)
    new = application(root, "version-2", adopted, packages, version)

    # A store with four commits and a pending payment upgrades at commit 4.
    paid = root / "paid.sqlite"
    decided = upgrades.report(upgrades.run_app(old, ["--decide", str(paid)]), "version 1 decide")
    upgrades.expect(decided, {"bundles": 4, "pending": 1}, "version 1 decide")
    pending = upgrades.pending_ids(paid)
    if len(pending) != 1:
        raise RuntimeError(f"expected one pending payment, found {pending}")
    before = upgrades.report(upgrades.run_app(new, ["--audit", str(paid)]), "audit before upgrade")
    upgrades.expect(before, {"contract_version": 1, "commits": 4, "pending": 1, "upgrades": 0},
                    "audit before upgrade")
    refused_without_write(new, ["--decide", str(paid)], paid, "upgrade it",
                          "version 2 decide before the upgrade")
    upgraded = upgrades.report(upgrades.run_app(new, ["--upgrade", str(paid)]), "upgrade at commit 4")
    upgrades.expect(upgraded, {"status": "upgraded", "kind": "program-successor",
                               "premises": upgrades.PREMISES, "at_commit": 4,
                               "receipts": [adoption["receipt_sha256"]], "contract_version": 2,
                               "pending": 1, "upgrades": 1}, "upgrade at commit 4")
    if upgrades.pending_ids(paid) != pending:
        raise RuntimeError("the upgrade changed the pending payment")
    delivered = upgrades.report(upgrades.run_app(new, ["--deliver", str(paid)]), "deliver")
    upgrades.expect(delivered, {"deliveries": pending, "contract_version": 2, "commits": 4,
                                "pending": 0, "upgrades": 1}, "deliver")
    upgrades.expect_refusal(upgrades.run_app(old, ["--audit", str(paid)]), "Identity",
                            "version 1 over the upgraded store")
    refused_without_write(old, ["--decide", str(paid)], paid, "Identity",
                          "version 1 decide over the upgraded store")

    # A store away from genesis keeps committing under version 2.
    open_request = root / "open.sqlite"
    first = upgrades.report(upgrades.run_app(old_session, ["--decide", str(open_request)]),
                            "version 1 session")
    upgrades.expect(first, {"decisions": ["Reject", "Reject", "Accept", "Reject", "Reject", "Accept"],
                            "bundles": 2, "pending": 0}, "version 1 session")
    upgraded_open = upgrades.report(upgrades.run_app(new, ["--upgrade", str(open_request)]),
                                    "upgrade at commit 2")
    upgrades.expect(upgraded_open, {"kind": "program-successor", "premises": upgrades.PREMISES,
                                    "at_commit": 2, "contract_version": 2}, "upgrade at commit 2")
    continued = upgrades.report(upgrades.run_app(new, ["--decide", str(open_request)]),
                                "version 2 decide")
    upgrades.expect(continued, {"decisions": ["Reject", "Reject", "Accept", "Reject", "Accept", "Reject"],
                                "bundles": 4, "pending": 1}, "version 2 decide")
    executed = upgrades.report(upgrades.run_app(new, ["--deliver", str(open_request)]), "deliver payment")
    if len(executed["deliveries"]) != 1:
        raise RuntimeError(f"expected the payment delivered: {executed}")
    audited = upgrades.report(upgrades.run_app(new, ["--audit", str(open_request)]), "audit both segments")
    upgrades.expect(audited, {"status": "audited", "contract_version": 2, "commits": 4,
                              "pending": 0, "upgrades": 1}, "audit both segments")
    return {"exported": exported["program"], "optimized_nodes": optimized["detail"]["best"]["nodes"],
            "adoption": adoption, "paid": {"decided": decided, "upgrade": upgraded,
                                           "delivered": delivered},
            "open": {"decided": first, "upgrade": upgraded_open, "continued": continued,
                     "audit": audited}}


def release_build(directory: Path) -> dict:
    """The CLI as the release build makes it holds no path of this checkout,
    so its `new` needs `--source`; the same build without the variable holds
    one, which shows the scan would find it."""
    target = Path(os.environ["CARGO_TARGET_DIR"]).resolve() / "app-journey-release-environment"
    environment = rc_package.remapped_compiler_environment(dict(os.environ), ROOT, "/zeno-fcis-source")
    environment["CARGO_TARGET_DIR"] = str(target)
    binary = target / "debug" / "zeno-fcis"
    results = {}
    for name, value in (("control", None), ("release", "")):
        build = dict(environment)
        build.pop("ZENO_FCIS_BUILD_TREE", None)
        if value is not None:
            build["ZENO_FCIS_BUILD_TREE"] = value
        applications.run(["cargo", "+1.97.1", "build", "--locked", "--offline", "-p", "zeno-fcis-cli",
                          "--bin", "zeno-fcis"], ROOT, environment=build)
        copy = directory / f"zeno-fcis-{name}"
        shutil.copy2(binary, copy)
        results[name] = holds(copy, str(ROOT))
    if not results["control"]:
        raise RuntimeError("control: a build without ZENO_FCIS_BUILD_TREE should hold the checkout's path")
    if results["release"]:
        raise RuntimeError("a build with ZENO_FCIS_BUILD_TREE empty holds the checkout's path")
    release = directory / "zeno-fcis-release"
    root = directory / "release-new"
    root.mkdir()
    refused = subprocess.run([str(release), "new", "escrow", "--contract", str(ESCROW)], cwd=root,
                             check=False, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if refused.returncode != 2 or "--source" not in refused.stderr:
        raise RuntimeError(f"new without a source tree: exit {refused.returncode}, {refused.stderr!r}")
    if any((root / "escrow").iterdir()):
        raise RuntimeError("new without a source tree wrote a file")
    bound = subprocess.run([str(release), "new", "bound", "--contract", str(ESCROW), "--source",
                            str(ROOT)], cwd=root, check=False, text=True,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if bound.returncode != 0 or not (root / "bound" / "Cargo.lock").is_file():
        raise RuntimeError(f"new with --source: exit {bound.returncode}, {bound.stderr!r}")
    return {"control_holds_checkout_path": results["control"],
            "release_holds_checkout_path": results["release"],
            "new_without_source": {"exit": refused.returncode, "message": refused.stderr.strip()}}


def check(directory: Path) -> dict:
    applications.run(["cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"], ROOT)
    packages = {tomllib.loads(manifest.read_text())["package"]["name"]: manifest.parent
                for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml"))}
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    return {"schema": "zeno-fcis/app-journey-check/1", "status": "passed", "authority": "none",
            "readme": readme_journey(directory, packages, version),
            "upgrade": upgrade_journey(directory, packages, version),
            "release_build": release_build(directory)}


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
        result = check(directory)
    else:
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-app-journey-") as directory:
            result = check(Path(directory))
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"app journey: FAIL: {error}", file=sys.stderr)
        sys.exit(1)
