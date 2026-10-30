#!/usr/bin/env python3
"""Check stores and adopted contracts created by the released 2.6.0 CLI.

The released CLI and its source tree make every artifact here: a version 1
store at genesis, a version 1 store with a pending payout, and the
withdrawal-queue contract adopted with the 2.6.0 checker. The released
version 2 build upgrades both stores, as a control. The adopted contract must
equal the fixture the released tree retains, file for file.

This checkout then meets those artifacts. Its identity differs from 2.6.0
only in the trailing evaluator digest, so its builds refuse a 2.6.0 store
by name and write nothing, and its CLI refuses the 2.6.0 receipt until
`contract refresh-receipts`, which must yield this checkout's own fixture.
`--write-test-data` exports the released inputs the native tests replay,
with their provenance: the tag commit, the source lock, the toolchain, the
exact CLI build command when this tool builds it (`--released-target`), the
CLI's SHA-256 and every generation command.

Without `--write-test-data`, the supplied executable must have the SHA-256
the retained manifest records; any other file is refused by name before a
store is made. The acceptance scenario supplies the executable and the
released checkout through `ZENO_FCIS_RELEASED_26_CLI` and
`ZENO_FCIS_RELEASED_26_SOURCE` when the options are absent.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import tomllib

import check_contract_upgrade as upgrade
import check_generated_application as applications

RELEASE = "2.6.0"
SUBJECT = "7d175c47099866939c86acd463ac85219abba882"
TEMPLATE = Path("crates/zeno-fcis-cli/templates/withdrawal-queue")
FIXTURE = Path("crates/zeno-fcis-cli/tests/fixtures/withdrawal-queue-adopted-2.5")
PROGRAMS = Path("crates/zeno-fcis-cli/tests/fixtures/i128-programs")
INPUTS = ("project.zeno", "v2/policy.json", "v2/schema-origin.json")
EXAMPLES = "tests/decision-examples.txt"
FIXTURE_FILES = (
    "project.zeno", "src/v2_contract.rs", "src/v2_contract_v1.rs", EXAMPLES,
    "v2/adoptions/1/program.zcve", "v2/adoptions/1/receipt.json", "v2/policy.json",
    "v2/policy.zcve", "v2/policy_v1.zcve", "v2/schema-origin.json", "v2/schema.zcve",
)
# The named refusal of a store created by a 2.6.0 build.
REFUSAL = "OtherRelease"
# The acceptance scenario supplies the released inputs through these variables.
RELEASED_CLI_ENV = "ZENO_FCIS_RELEASED_26_CLI"
RELEASED_SOURCE_ENV = "ZENO_FCIS_RELEASED_26_SOURCE"
# The exact build of the released CLI, in the released checkout.
BUILD = ["cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"]
BUILD_ENVIRONMENT = {"CARGO_PROFILE_DEV_DEBUG": "0"}


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def run(command: list[str | Path], cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run([str(item) for item in command], cwd=cwd, check=False, text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def ok(command: list[str | Path], cwd: Path) -> str:
    completed = run(command, cwd)
    if completed.returncode:
        raise RuntimeError(f"command failed ({completed.returncode}): {command}\n"
                           f"{completed.stdout}\n{completed.stderr}")
    return completed.stdout


def packages(source: Path) -> dict[str, Path]:
    return {tomllib.loads(path.read_text())["package"]["name"]: path.parent
            for path in sorted((source / "crates").glob("*/Cargo.toml"))}


def files(directory: Path) -> dict[str, str]:
    return {name: digest((directory / name).read_bytes()) for name in FIXTURE_FILES}


def store_bytes(store: Path) -> dict[str, str]:
    """The store and its side files, so a refusal is shown to write nothing."""
    return {path.name: digest(path.read_bytes())
            for path in sorted(store.parent.glob(store.name + "*")) if path.is_file()}


def copy_store(store: Path, destination: Path) -> Path:
    for path in sorted(store.parent.glob(store.name + "*")):
        shutil.copyfile(path, destination.parent / (destination.name + path.name[len(store.name):]))
    return destination


def prepare(app: Path, source: Path) -> dict:
    """Admit the binding the released `new` wrote and its lock, as written,
    against the released tree's reviewed graph."""
    graph = packages(source)
    binding = applications.check_generated_binding(app, graph, RELEASE)
    package = tomllib.loads((app / "Cargo.toml").read_text())["package"]
    allowed = {(name, RELEASE): path / "Cargo.toml" for name, path in graph.items()}
    allowed[(package["name"], package["version"])] = app / "Cargo.toml"
    metadata = json.loads(ok(["cargo", "+1.97.1", "metadata", "--locked", "--offline",
                              "--format-version", "1"], app))
    applications.validate_resolved_graph(tomllib.loads((source / "Cargo.lock").read_text()),
                                        tomllib.loads((app / "Cargo.lock").read_text()),
                                        metadata, allowed)
    return {**binding, "lock_sha256": digest((app / "Cargo.lock").read_bytes())}


def check_released_source(source: Path) -> None:
    if tomllib.loads((source / "Cargo.toml").read_text())["workspace"]["package"]["version"] != RELEASE:
        raise RuntimeError(f"the released source is not version {RELEASE}")
    if (ok(["git", "rev-parse", "HEAD"], source).strip() != SUBJECT
            or ok(["git", "status", "--porcelain", "--untracked-files=no"], source).strip()):
        raise RuntimeError(f"the released source is not an unmodified checkout of {SUBJECT}")


def build_released_cli(source: Path, target: Path) -> tuple[Path, dict]:
    """Build the released CLI from the released checkout with the recorded command."""
    check_released_source(source)
    environment = {**os.environ, **BUILD_ENVIRONMENT, "CARGO_TARGET_DIR": str(target)}
    subprocess.run(BUILD, cwd=source, check=True, env=environment)
    return target / "debug/zeno-fcis", {"command": BUILD, "environment": BUILD_ENVIRONMENT,
                                        "directory": "the released checkout"}


def provenance(cli: Path, source: Path, directory: Path, build: dict | None) -> dict:
    check_released_source(source)
    if ok([cli, "--version"], directory).strip() != f"zeno-fcis {RELEASE}":
        raise RuntimeError(f"the supplied executable is not zeno-fcis {RELEASE}")
    return {"tag": f"v{RELEASE}", "subject": SUBJECT,
            "source_lock_sha256": digest((source / "Cargo.lock").read_bytes()),
            "rustc": ok(["rustc", "+1.97.1", "--version"], directory).strip(),
            "cargo": ok(["cargo", "+1.97.1", "--version"], directory).strip(),
            "build": build, "released_cli_sha256": digest(cli.read_bytes())}


def released(directory: Path, cli: Path, source: Path, build: dict | None = None) -> dict:
    """Make every 2.6.0 artifact with the released CLI and its own source tree."""
    origin = provenance(cli, source, directory, build)
    commands: list[list[str]] = []

    def relative(item: str | Path) -> str:
        text = str(item)
        for path, name in ((cli, "zeno-fcis"), (source, "V260"), (directory, ".")):
            text = text.replace(str(path), name)
        return text

    def step(command: list[str | Path]) -> str:
        commands.append([relative(item) for item in command])
        return ok(command, directory)

    def app(binary: Path, arguments: list[str], label: str) -> dict:
        commands.append([binary.name, *(relative(item) for item in arguments)])
        return upgrade.report(upgrade.run_app(binary, arguments), label)
    raw = directory / "original-contract"
    for name in INPUTS:
        (raw / name).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source / TEMPLATE / name, raw / name)
    (raw / "tests").mkdir()
    (raw / EXAMPLES).write_text(upgrade.SESSION)
    first = directory / "released-version-1"
    # `--source` names the released checkout explicitly: a published executable
    # is built without a bound source tree, and a build from the tag binds to it.
    step([cli, "new", first, "--contract", raw, "--source", source])
    first_graph = prepare(first, source)
    commands.append(["cargo", "+1.97.1", "build", "--locked", "--offline", "# in ./released-version-1"])
    first = upgrade.build_app(first)
    genesis = directory / "genesis.sqlite"
    pending = directory / "pending.sqlite"
    initialized = app(first, ["init", str(genesis), "--format", "json"], "released genesis")
    audited = app(first, ["--audit", str(genesis)], "released audit")
    upgrade.expect(audited, {"contract_version": 1, "commits": 0, "pending": 0, "upgrades": 0},
                   "released audit")
    decided = app(first, ["--decide", str(pending)], "released decide")
    upgrade.expect(decided, {"decisions": ["Accept"] * 3, "bundles": 3, "pending": 1,
                             "deliveries": 0}, "released decide")
    pending_ids = upgrade.pending_ids(pending)
    if len(pending_ids) != 1:
        raise RuntimeError(f"expected one released pending payout: {pending_ids}")

    adopted = directory / "adopted-contract"
    for name in INPUTS:
        (adopted / name).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source / TEMPLATE / name, adopted / name)
    original = directory / "original.zcve"
    receipt = directory / "receipt.json"
    candidate = source / PROGRAMS / "current-decision-scalars-candidate.zcve"
    step([cli, "contract", "export-program", adopted, "--out", original])
    if original.read_bytes() != (source / PROGRAMS / "current-decision-scalars-original.zcve").read_bytes():
        raise RuntimeError("the released export differs from the released original program")
    step([cli, "transform", "check", "--original", original, "--candidate", candidate,
          "--receipt", receipt])
    step([cli, "contract", "adopt", adopted, "--candidate", candidate, "--receipt", receipt,
          "--usage", "new-version"])
    (adopted / "tests").mkdir()
    shutil.copyfile(source / TEMPLATE / EXAMPLES, adopted / EXAMPLES)
    adopted_files = files(adopted)
    if adopted_files != files(source / FIXTURE):
        raise RuntimeError("the released adoption differs from the released tree's fixture")

    # Control: the released version 2 build upgrades both released stores.
    second = directory / "released-adopted"
    step([cli, "new", second, "--contract", adopted, "--source", source])
    second_graph = prepare(second, source)
    commands.append(["cargo", "+1.97.1", "build", "--locked", "--offline", "# in ./released-adopted"])
    second = upgrade.build_app(second)
    controls = []
    for store, commits, count in ((genesis, 0, 0), (pending, 3, 1)):
        trial = copy_store(store, directory / ("control-" + store.name))
        upgraded = app(second, ["--upgrade", str(trial)], "released upgrade")
        upgrade.expect(upgraded, {"status": "upgraded", "kind": "program-successor",
                                  "contract_version": 2, "commits": commits, "pending": count,
                                  "upgrades": 1}, "released upgrade")
        controls.append(upgraded)
    if upgrade.pending_ids(directory / "control-pending.sqlite") != pending_ids:
        raise RuntimeError("the released upgrade changed the pending delivery ID")
    record = {"release": RELEASE, "subject": SUBJECT, "provenance": origin,
              "released_cli_sha256": origin["released_cli_sha256"], "commands": commands,
              "version_1_lock_sha256": first_graph["lock_sha256"],
              "version_2_lock_sha256": second_graph["lock_sha256"],
              "initialized": initialized, "genesis_audit": audited, "decided": decided,
              "pending_ids": pending_ids, "adopted_files": adopted_files,
              "released_controls": controls}
    (directory / "RELEASED_INPUTS.json").write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    return record


SHELL_DATA = Path("crates/zeno-fcis-shell-sqlite/tests/test-data/released-26")
CLI_DATA = Path("crates/zeno-fcis-cli/tests/fixtures/withdrawal-queue-adopted-2.6.0")


def pinned_cli_sha256(root: Path) -> str:
    """The released CLI's SHA-256 as the retained manifest, written by
    `--write-test-data`, records it."""
    manifest = json.loads((root / SHELL_DATA / "manifest.json").read_text())
    if manifest.get("release") != RELEASE or manifest.get("subject") != SUBJECT:
        raise RuntimeError(f"the retained manifest is not for {RELEASE} at {SUBJECT}")
    pinned = manifest.get("released_cli_sha256")
    if not isinstance(pinned, str) or len(pinned) != 64:
        raise RuntimeError("the retained manifest has no released CLI SHA-256")
    return pinned


def check_pinned_cli(cli: Path, root: Path) -> str:
    """Refuse, by name, any executable other than the one the manifest pins.
    The producer run (`--write-test-data`) is the only way to change the pin."""
    pinned = pinned_cli_sha256(root)
    actual = digest(cli.read_bytes())
    if actual != pinned:
        raise RuntimeError(f"the supplied executable {cli} has SHA-256 {actual}; the retained "
                           f"manifest pins {pinned}. Re-run with --write-test-data to re-pin "
                           f"it deliberately, or supply the pinned executable.")
    return actual


def export_test_data(directory: Path, root: Path) -> dict:
    """Export retained released inputs. They confer no upgrade or replay authority."""
    record = json.loads((directory / "RELEASED_INPUTS.json").read_text())
    shell = root / SHELL_DATA
    shell.mkdir(parents=True, exist_ok=True)
    stores = {}
    for name in ("genesis", "pending"):
        source = (directory / f"{name}.sqlite").resolve()
        with sqlite3.connect(source.as_uri() + "?mode=ro", uri=True) as connection:
            if connection.execute("PRAGMA user_version").fetchone()[0] != 10:
                raise RuntimeError("the released store is not schema version 10")
            sql = "PRAGMA user_version=10;\n" + "\n".join(connection.iterdump()) + "\n"
        (shell / f"{name}.sql").write_text(sql)
        stores[f"{name}.sql"] = digest(sql.encode())
    shell_manifest = {"schema": "zeno-fcis/released-26-test-inputs/1", "release": RELEASE,
                      "subject": SUBJECT, "released_cli_sha256": record["released_cli_sha256"],
                      "provenance": record["provenance"], "commands": record["commands"],
                      "version_1_lock_sha256": record["version_1_lock_sha256"],
                      "files": stores, "pending_ids": record["pending_ids"], "authority": "none"}
    (shell / "manifest.json").write_text(json.dumps(shell_manifest, indent=2, sort_keys=True) + "\n")
    # Only the adopted files that differ from this checkout's fixture are kept;
    # the manifest binds all of them, so a test rebuilds the released bytes.
    adopted = directory / "adopted-contract"
    current = files(root / FIXTURE)
    kept = sorted(name for name, value in record["adopted_files"].items() if current[name] != value)
    cli = root / CLI_DATA
    if cli.exists():
        shutil.rmtree(cli)
    for name in kept:
        (cli / name).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(adopted / name, cli / name)
    cli_manifest = {"schema": "zeno-fcis/released-26-adoption/1", "release": RELEASE,
                    "subject": SUBJECT, "released_cli_sha256": record["released_cli_sha256"],
                    "provenance": record["provenance"], "commands": record["commands"],
                    "base": str(FIXTURE), "kept": kept, "files": record["adopted_files"],
                    "authority": "none"}
    (cli / "released.json").write_text(json.dumps(cli_manifest, indent=2, sort_keys=True) + "\n")
    return {"status": "generated", "stores": sorted(stores), "kept": kept, "authority": "none"}


def refused(binary: Path, arguments: list[str], store: Path, label: str) -> str:
    """Run one store command that must refuse by name and write nothing."""
    before = store_bytes(store)
    completed = upgrade.run_app(binary, [*arguments, str(store)])
    upgrade.expect_refusal(completed, f"({REFUSAL})", label)
    if store_bytes(store) != before:
        raise RuntimeError(f"{label}: the refusal changed the store")
    return completed.stderr.strip()


def current(directory: Path, cli: Path, record: dict) -> dict:
    """Meet the released artifacts with this checkout's CLI and builds."""
    graph = packages(applications.ROOT)
    version = tomllib.loads((applications.ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    first = directory / "current-version-1"
    ok([cli, "new", first, "--contract", directory / "original-contract"], directory)
    upgrade.prepare(first, graph, version)
    first = upgrade.build_app(first)
    stores = {}
    for name in ("genesis", "pending"):
        trial = copy_store(directory / f"{name}.sqlite", directory / f"current-{name}.sqlite")
        stores[name] = {verb or "state": refused(first, [verb] if verb else ["state"], trial,
                                                 f"version 1 {verb or 'state'} of the released {name} store")
                        for verb in (None, "--audit", "--upgrade")}

    adopted = directory / "current-adopted-contract"
    shutil.copytree(directory / "adopted-contract", adopted)
    generation = run([cli, "generate", "contract", adopted, "--check", "--format", "json"], directory)
    message = json.loads(generation.stdout or "{}").get("error", {}).get("message", "")
    if generation.returncode != 1 or "zeno-fcis contract refresh-receipts DIR" not in message:
        raise RuntimeError(f"the released adoption must refuse naming refresh-receipts: {generation}")
    if files(adopted) != record["adopted_files"]:
        raise RuntimeError("the refused generation changed the released adoption")
    refreshed = json.loads(ok([cli, "contract", "refresh-receipts", adopted, "--format", "json"], directory))
    if refreshed.get("refreshed") != [1] or files(adopted) != files(applications.ROOT / FIXTURE):
        raise RuntimeError(f"the refreshed adoption differs from this checkout's fixture: {refreshed}")
    second = directory / "current-adopted"
    ok([cli, "new", second, "--contract", adopted], directory)
    upgrade.prepare(second, graph, version)
    second = upgrade.build_app(second)
    for name in ("genesis", "pending"):
        trial = copy_store(directory / f"{name}.sqlite", directory / f"current-v2-{name}.sqlite")
        stores[name]["version 2 --upgrade"] = refused(
            second, ["--upgrade"], trial, f"version 2 upgrade of the released {name} store")
    # The migration step: a new store under this checkout's version 1 build.
    fresh = upgrade.report(upgrade.run_app(first, ["--decide", str(directory / "current.sqlite")]),
                           "a new store under this build")
    upgrade.expect(fresh, {"decisions": ["Accept"] * 3, "bundles": 3, "pending": 1},
                   "a new store under this build")
    return {"stores": stores, "generation_refusal": message, "refreshed": refreshed["refreshed"],
            "new_store": fresh}


def check(directory: Path, released_cli: Path, released_source: Path, current_cli: Path,
          write_test_data: bool, build: dict | None = None) -> dict:
    pinned = None if write_test_data else check_pinned_cli(released_cli, applications.ROOT)
    record = released(directory, released_cli, released_source, build)
    exported = export_test_data(directory, applications.ROOT) if write_test_data else None
    observed = current(directory, current_cli, record)
    return {"schema": "zeno-fcis/contract-upgrade-26-check/1", "status": "passed",
            "authority": "none", "pinned_cli_sha256": pinned, "released_inputs": record,
            "exported": exported, "current": observed}


def resolve_inputs(args: argparse.Namespace, environment: dict[str, str]) -> tuple[Path | None, Path]:
    """The released executable (None when `--released-target` builds it) and
    the released checkout, from the options or the scenario's variables."""
    cli = args.released_cli
    if cli is None and args.released_target is None:
        value = environment.get(RELEASED_CLI_ENV, "")
        if not value:
            raise SystemExit(f"error: one of --released-cli, --released-target or "
                             f"{RELEASED_CLI_ENV} is required")
        cli = Path(value)
    source = args.released_source
    if source is None:
        value = environment.get(RELEASED_SOURCE_ENV, "")
        if not value:
            raise SystemExit(f"error: --released-source or {RELEASED_SOURCE_ENV} is required")
        source = Path(value)
    return cli, source


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    released_cli = parser.add_mutually_exclusive_group()
    released_cli.add_argument("--released-cli", type=Path,
                              help=f"a zeno-fcis 2.6.0 executable (default: {RELEASED_CLI_ENV})")
    released_cli.add_argument("--released-target", type=Path,
                              help="build the released CLI here with the recorded command")
    parser.add_argument("--released-source", type=Path,
                        help=f"an unmodified checkout of {SUBJECT}, the v{RELEASE} tag "
                             f"(default: {RELEASED_SOURCE_ENV})")
    parser.add_argument("--current-cli", type=Path, default=Path(upgrade.cli()))
    parser.add_argument("--keep", type=Path, help="a new directory that keeps every artifact")
    parser.add_argument("--write-test-data", action="store_true",
                        help="export the released inputs the native tests replay")
    args = parser.parse_args()
    os.environ.setdefault("CARGO_BUILD_JOBS", "2")
    cli, source = resolve_inputs(args, os.environ)
    source = source.resolve()
    build = None
    if args.released_target:
        cli, build = build_released_cli(source, args.released_target.resolve())
    else:
        cli = cli.resolve()
    arguments = (cli, source, args.current_cli.resolve(), args.write_test_data, build)
    if args.keep:
        args.keep.mkdir()
        result = check(args.keep.resolve(), *arguments)
    else:
        with tempfile.TemporaryDirectory(prefix="contract-upgrade-26-") as directory:
            result = check(Path(directory), *arguments)
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        raise SystemExit(main())
    run_cli(_entrypoint)
