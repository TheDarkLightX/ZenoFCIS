#!/usr/bin/env python3
"""Install this host's release archive exactly as documented and use it like a user would.

Usage: python3 tools/check_installed_cli.py --release DIR [--source TREE] --work-dir DIR --report FILE

DIR holds `binaries/zeno-fcis-<version>-<host>.tar.gz` for this host: a build-host-binary
folder or a release set. TREE is the source tree generated apps bind to (default: this
checkout). Every step records its argument list, exit status and output hashes; a missing
tool or an unexpected result fails the journey, and nothing is skipped:

1. The fenced install block marked `<!-- installed-journey: <host> -->` in
   docs/INSTALLATION.md runs with `bash -euo pipefail`, with only the release URL prefix
   replaced by a local folder holding the archive and a DOWNLOADS.sha256 made as the
   release makes it. The block names this checkout's version, or takes it from VERSION.
   This runs curl, the platform's checksum command, tar, PATH and `zeno-fcis describe`
   as users run them.
2. The installed executable equals the archive member, holds no checkout or home path,
   and states the documented floor: on Linux the ELF interpreter and glibc 2.39; on
   macOS arm64 only, macOS 11.0, and `codesign --verify --strict` with an ad-hoc signature.
3. `new` without `--source` exits 2, names `--source` and creates no target directory.
4. With `--source TREE`: the escrow fixture and the durable-counter template are created,
   their bindings checked, and their README build commands run as written (`cargo test`,
   then `cargo run`), which compiles the bundled SQLite with the platform's C compiler.
5. A store journey on the spend-approval fixture app: init, a refused second init,
   refused submissions that write nothing, three accepted submissions, a decide that
   writes nothing, the execution, pending, deliver to a file exactly once, history, state.
6. Serial checks: export-program, optimize with a receipt and its replay; transform check
   with a receipt, its replay, and the same receipt path again refused with its bytes kept.
7. Platform routes. macOS: `--offline-parallel` and `loop open` print the exact
   inconclusive result, exit 2 and write nothing; the durable counter's README
   `synth verify` passes with a separately bound finite-conformance receipt.
   Linux, inside the contained runtime: the `--offline-parallel`
   receipt equals the serial one and replays, and `synth verify` passes.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shlex
import shutil
import subprocess
import sys
import tarfile
import time
import tomllib
from pathlib import Path

import check_app_journey as journey
import check_contract_upgrade as upgrades
import check_generated_application as applications
import check_release_custody as custody
import rc_package

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = custody.JOURNEY_SCHEMA
URL_PREFIX = re.compile(r"https://github\.com/TheDarkLightX/ZenoFCIS/releases/download/v(\$VERSION|[0-9][^/\s\"']*)/")
HOSTS = {("Linux", "x86_64"): "x86_64-unknown-linux-gnu", ("Darwin", "arm64"): "aarch64-apple-darwin"}
MAC = "aarch64-apple-darwin"
# Exercise the current release profile, as the shared-checker and parallel gates do.
ARTIFACTS = ROOT / "crates/zeno-fcis-cli/tests/fixtures/i128-programs"
FIXTURES = Path("crates/zeno-fcis-cli/tests/fixtures")


class JourneyError(RuntimeError):
    """A failed journey step."""


def macos_refusal(schema: str, backend: str, profile: str, reason: str) -> dict:
    """Expected typed refusal when the journey explicitly selects a backend on macOS."""
    containment = {
        "schema": "zeno-fcis/containment-result/1", "backend": backend, "required_profile": profile,
        "selection_source": "flag", "evidence": "inline-unqualified-refusal",
        "startup": {"admitted": False, "guarantees": []},
        "final": {"termination": "stopped", "reason": reason, "application_work_started": False,
                  "cleanup": "unknown", "absence_verified": False},
    }
    result = {"schema": schema, "status": "inconclusive", "authority": "none", "reason": reason,
              "accounting_complete": False, "backend": backend, "required_profile": profile,
              "containment": containment}
    return result


def host_triple(system: str, machine: str) -> str:
    try:
        return HOSTS[(system, machine)]
    except KeyError:
        raise JourneyError(f"no qualified release host for {system} {machine}") from None


def install_block(markdown: str, host: str) -> str:
    """The fenced bash block right after `<!-- installed-journey: HOST -->`; exactly one."""
    marker = f"<!-- installed-journey: {host} -->"
    if markdown.count(marker) != 1:
        raise JourneyError(f"docs/INSTALLATION.md must mark exactly one install block for {host}")
    after = markdown.split(marker, 1)[1].lstrip("\n")
    if not after.startswith("```bash\n"):
        raise JourneyError(f"the {host} marker is not followed by a bash block")
    return after[len("```bash\n"):].split("\n```", 1)[0] + "\n"


def prepare_block(block: str, version: str, downloads: str) -> tuple[str, dict[str, str]]:
    """The block with its release URL prefix replaced by `downloads`, and the environment it needs.

    Every release URL names `version`, or every one names `$VERSION`, which the block then
    takes from the environment and never assigns."""
    named = set(URL_PREFIX.findall(block))
    if not named:
        raise JourneyError("the install block downloads nothing from the release")
    if named == {"$VERSION"}:
        if re.search(r"(?m)^\s*(export\s+)?VERSION=", block):
            raise JourneyError("a block that uses $VERSION must take it from the environment")
        environment = {"VERSION": version}
    elif named == {version}:
        environment = {}
    else:
        raise JourneyError(f"the install block names {sorted(named)}, not this checkout's version {version}")
    return URL_PREFIX.sub(downloads.rstrip("/") + "/", block), environment


def downloads_text(release: Path, host: str, version: str) -> str:
    """The flat list the release signs: the set's list when it has an index, else this host's archive."""
    if (release / "SHA256SUMS").is_file():
        return rc_package.downloads_lines(release)
    archive = release / "binaries" / f"zeno-fcis-{version}-{host}.tar.gz"
    return f"{rc_package.sha256(archive)}  {archive.name}\n"


def sh_block(readme: str, holding: str) -> list[list[str]]:
    """The commands of the README's one `sh` block that holds the line `holding`."""
    blocks = [block.split("```", 1)[0] for block in readme.split("```sh\n")[1:]]
    found = [block for block in blocks if holding in block.splitlines()]
    if len(found) != 1:
        raise JourneyError(f"the README has no single sh block with `{holding}`")
    return [shlex.split(line) for line in found[0].splitlines() if line.strip()]


def workspace_version(root: Path = ROOT) -> str:
    return tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["version"]


def files_under(directory: Path) -> list[str]:
    return sorted(path.relative_to(directory).as_posix() for path in directory.rglob("*") if not path.is_dir())


class Journey:
    def __init__(self, release: Path, source: Path, work: Path):
        self.release, self.source, self.work = release, source, work
        self.version = workspace_version()
        self.host = host_triple(platform.system(), platform.machine())
        self.archive = release / "binaries" / f"zeno-fcis-{self.version}-{self.host}.tar.gz"
        self.steps: list[dict] = []
        # The generated applications' rust-toolchain.toml selects the toolchain, as in a new shell.
        self.environment = {key: value for key, value in os.environ.items() if key != "RUSTUP_TOOLCHAIN"}
        self.environment["CARGO_TARGET_DIR"] = str(work / "app-target")
        self.counter: Path | None = None
        self.report: dict = {"schema": SCHEMA, "status": "unfinished", "host": self.host, "version": self.version,
                             "steps": self.steps, "nonclaims": [
                                 "covers this archive on this runner image only",
                                 "not notarization or Developer ID signing",
                                 "not a reproducible-build proof"]}

    def run(self, name: str, arguments: list, *, cwd: Path, expected: int | None = 0,
            timeout: int = 3600, environment: dict | None = None) -> subprocess.CompletedProcess:
        command = [str(value) for value in arguments]
        started = time.monotonic()
        completed = subprocess.run(command, cwd=cwd, env=environment or self.environment, capture_output=True,
                                   timeout=timeout, check=False)
        self.steps.append({"name": name, "argv": command, "exit": completed.returncode,
                           "seconds": round(time.monotonic() - started, 3),
                           "stdout_sha256": hashlib.sha256(completed.stdout).hexdigest(),
                           "stderr_sha256": hashlib.sha256(completed.stderr).hexdigest()})
        if expected is not None and completed.returncode != expected:
            raise JourneyError(f"{name}: exit {completed.returncode}, expected {expected}: "
                               f"{completed.stderr.decode(errors='replace')[-2000:]}")
        return completed

    def report_of(self, name: str, arguments: list, *, cwd: Path, expected: int = 0) -> dict:
        completed = self.run(name, arguments, cwd=cwd, expected=expected)
        try:
            return json.loads(completed.stdout)
        except ValueError:
            raise JourneyError(f"{name}: no JSON report") from None

    def expect(self, actual: dict, expected: dict, label: str) -> None:
        try:
            upgrades.expect(actual, expected, label)
        except RuntimeError as error:
            raise JourneyError(str(error)) from None

    def host_facts(self) -> dict:
        facts = {"uname": platform.uname()._asdict(), "python": sys.version}
        if self.host == MAC:
            for name, command in (("sw_vers", ["sw_vers"]), ("xcode", ["xcodebuild", "-version"]),
                                  ("sdk", ["xcrun", "--sdk", "macosx", "--show-sdk-version"])):
                facts[name] = self.run(f"host-{name}", command, cwd=self.work).stdout.decode()
        else:
            facts["os_release"] = Path("/etc/os-release").read_text()
        facts["rustc"] = self.run("host-rustc", ["rustc", "+1.97.1", "-vV"], cwd=self.work).stdout.decode()
        return facts

    def install(self) -> Path:
        downloads = self.work / "downloads"
        downloads.mkdir()
        shutil.copyfile(self.archive, downloads / self.archive.name)
        (downloads / "DOWNLOADS.sha256").write_text(downloads_text(self.release, self.host, self.version))
        block = install_block((ROOT / "docs/INSTALLATION.md").read_text(), self.host)
        script, variables = prepare_block(block, self.version, downloads.as_uri())
        directory = self.work / "install"
        directory.mkdir()
        (self.work / "install-block.sh").write_text(script)
        installed = self.run("documented-install", ["bash", "-euo", "pipefail", "-c", script], cwd=directory,
                             environment={**self.environment, **variables})
        executable = directory / f"zeno-fcis-{self.version}-{self.host}" / "zeno-fcis"
        if not executable.is_file():
            raise JourneyError("the documented install left no executable at the archive prefix")
        described = self.run("describe", [executable, "describe"], cwd=directory)
        if not installed.stdout.endswith(described.stdout):
            raise JourneyError("the documented install did not end by running the installed zeno-fcis describe")
        try:
            version = json.loads(described.stdout)["cli_version"]
        except (ValueError, KeyError):
            raise JourneyError("zeno-fcis describe printed no cli_version") from None
        if version != self.version:
            raise JourneyError(f"the installed CLI is version {version}, not {self.version}")
        return executable

    def executable_facts(self, executable: Path) -> dict:
        with tarfile.open(self.archive, mode="r:gz") as archive:
            member = archive.extractfile(f"zeno-fcis-{self.version}-{self.host}/zeno-fcis")
            if member is None or member.read() != executable.read_bytes():
                raise JourneyError("the installed executable differs from the archive member")
        data = executable.read_bytes()
        problems = custody.executable_problems(data, self.host)
        for marker in sorted({str(ROOT), str(self.source)}):
            if rc_package.holds_marker(executable, marker):
                problems.append(f"the executable holds the checkout or source path {marker}")
        facts: dict = {"sha256": hashlib.sha256(data).hexdigest()}
        if self.host == MAC:
            facts["macho"] = custody.macho_facts(data)
            self.run("codesign-verify", ["codesign", "--verify", "--strict", executable], cwd=self.work)
            shown = self.run("codesign-display", ["codesign", "-dv", executable], cwd=self.work)
            if "Signature=adhoc" not in (shown.stdout + shown.stderr).decode(errors="replace").splitlines():
                problems.append("the executable does not carry an ad-hoc signature")
        else:
            facts["elf"] = custody.elf_facts(data)
        if problems:
            raise JourneyError("; ".join(problems))
        return facts

    def without_source(self, cli: Path) -> dict:
        directory = self.work / "without-source"
        directory.mkdir()
        refused = self.run("new-without-source", [cli, "new", "escrow", "--contract",
                                                   self.source / FIXTURES / "escrow"], cwd=directory, expected=2)
        message = (refused.stdout + refused.stderr).decode(errors="replace")
        written = files_under(directory)
        left = sorted(path.name for path in directory.iterdir())
        if "--source" not in message or written or left:
            raise JourneyError(f"new without --source did not name --source, or left {left} / {written}")
        return {"exit": refused.returncode, "message": message.strip(),
                "left": left}

    def packages(self) -> dict[str, Path]:
        return {tomllib.loads(manifest.read_text())["package"]["name"]: manifest.parent
                for manifest in sorted((self.source / "crates").glob("*/Cargo.toml"))}

    def bound(self, name: str, app: Path) -> dict:
        try:
            return applications.check_generated_binding(app, self.packages(), self.version)
        except RuntimeError as error:
            raise JourneyError(f"{name}: {error}") from None

    def readme_apps(self, cli: Path) -> dict:
        root = self.work / "apps"
        root.mkdir()
        environment = {**self.environment, "PATH": f"{cli.parent}{os.pathsep}{self.environment.get('PATH', '')}"}
        self.run("new-escrow", [cli, "new", "escrow", "--contract", self.source / FIXTURES / "escrow",
                                "--source", self.source], cwd=root)
        escrow = root / "escrow"
        escrow_binding = self.bound("escrow", escrow)
        try:
            commands = journey.readme_commands(escrow)
        except RuntimeError as error:
            raise JourneyError(f"escrow README: {error}") from None
        if [command[:2] for command in commands] != [["cargo", "test"], ["cargo", "run"]]:
            raise JourneyError(f"the escrow README's steps are not a cargo test and a cargo run: {commands}")
        outputs = [self.run(f"escrow-readme-{index}", journey.as_written(command, root / "escrow.sqlite"),
                            cwd=escrow, environment=environment).stdout.decode(errors="replace")
                   for index, command in enumerate(commands)]
        if "test every_example_is_the_authority_decision ... ok" not in outputs[0]:
            raise JourneyError("the escrow app's cargo test did not check its decision examples")
        summary = json.loads(outputs[1].strip().splitlines()[-1])
        self.expect(summary, journey.ESCROW_SUMMARY, "escrow session")
        self.run("new-durable-counter", [cli, "new", "counter", "--template", "durable-counter", "--source",
                                         self.source], cwd=root)
        counter = root / "counter"
        counter_binding = self.bound("durable counter", counter)
        counter_commands = sh_block((counter / "README.md").read_text(), "cargo +1.97.1 test --offline")
        for index, command in enumerate(counter_commands):
            self.run(f"counter-readme-{index}", command, cwd=counter, environment=environment)
        self.counter = counter
        return {"escrow": {"commands": commands, "summary": summary, "packages": escrow_binding["packages"]},
                "durable-counter": {"commands": counter_commands, "packages": counter_binding["packages"]}}

    def store_journey(self, cli: Path) -> dict:
        root = self.work / "store"
        root.mkdir()
        self.run("new-spend-approval", [cli, "new", "spend", "--contract", self.source / FIXTURES / "spend-approval",
                                        "--source", self.source], cwd=root)
        app = root / "spend"
        self.bound("spend approval", app)
        self.run("spend-build", ["cargo", "+1.97.1", "build", "--offline"], cwd=app)
        package = tomllib.loads((app / "Cargo.toml").read_text())["package"]["name"]
        binary = Path(self.environment["CARGO_TARGET_DIR"]) / "debug" / package
        store, payments = root / "operations.sqlite", root / "payments.jsonl"
        journal = Path(f"{store}.submissions")

        def operate(label: str, arguments: list, expected: int = 0) -> dict:
            return self.report_of(label, [binary, *arguments, "--format", "json"], cwd=root, expected=expected)

        def refused(label: str, arguments: list, expected: int, word: str) -> None:
            before = (store.read_bytes(), journal.read_bytes())
            completed = self.run(label, [binary, *arguments, "--format", "json"], cwd=root, expected=expected)
            if word not in (completed.stdout + completed.stderr).decode(errors="replace"):
                raise JourneyError(f"{label}: the refusal does not name {word!r}")
            if (store.read_bytes(), journal.read_bytes()) != before:
                raise JourneyError(f"{label}: the refusal wrote to the store or its journal")

        self.expect(operate("init", ["init", store]), {"status": "initialized", "commits": 0, "pending": 0}, "init")
        if not journal.is_file():
            raise JourneyError("init created no submission journal")
        refused("init-again", ["init", store], 1, "already exists")
        refused("reject", ["submit", store, "action=Create", "tier_requested=3", "role=Cfo", "board=false",
                           "within_limit=true"], 2, "Reject")
        refused("unknown-field", ["submit", store, "action=Create", "tier_requested=3", "role=Clerk",
                                  "board=false", "within_limit=true", "amount=5"], 1, "unknown field `amount`")
        refused("outside-domain", ["submit", store, "action=Create", "tier_requested=4", "role=Clerk",
                                   "board=false", "within_limit=true"], 1, "outside the domain")
        submitted = []
        for commit, (label, fields, state) in enumerate(journey.OPERATION, start=1):
            report = operate(label.replace(" ", "-"), ["submit", store, *fields])
            self.expect(report, {"status": "committed", "class": "Accept", "commit": commit, "state": state,
                                 "deliveries": []}, label)
            submitted.append(report)
        before = store.read_bytes()
        preview = operate("decide", ["decide", store, *journey.EXECUTE])
        self.expect(preview, {"status": "decided", "class": "Accept", "commit": None, "state": journey.EXECUTED,
                              "deliveries": [{"channel": 300, "payload": {"payment_tier": 3}}]}, "decide")
        if store.read_bytes() != before:
            raise JourneyError("decide wrote to the store")
        executed = operate("execute", ["submit", store, *journey.EXECUTE])
        self.expect(executed, {key: value for key, value in preview.items() if key != "status"}
                    | {"status": "committed", "commit": 4}, "execute")
        waiting = operate("pending", ["pending", store])
        self.expect(waiting, {"commits": 4, "pending": 1}, "pending")
        delivered = operate("deliver", ["deliver", store, "--to", payments])
        self.expect(delivered, {"status": "delivered", "destination": "file",
                                "deliveries": [waiting["next"]["delivery_id"]], "pending": 0}, "deliver")
        self.expect(operate("deliver-again", ["deliver", store, "--to", payments]), {"deliveries": [], "pending": 0},
                    "deliver again")
        if [json.loads(line) for line in payments.read_text().splitlines()] != [waiting["next"]]:
            raise JourneyError("the payment file does not hold the payment exactly once")
        history = operate("history", ["history", store])
        self.expect(history, {"status": "history", "commits": 4, "pending": 0}, "history")
        recorded = [{key: entry[key] for key in ("commit", "class", "state", "deliveries")}
                    for entry in history["entries"]]
        if recorded != [{key: report[key] for key in ("commit", "class", "state", "deliveries")}
                        for report in [*submitted, executed]]:
            raise JourneyError("the history differs from the submissions")
        self.expect(operate("state", ["state", store]), {"status": "state", "contract_version": 1, "commits": 4,
                                                         "pending": 0, "upgrades": 0, "state": journey.EXECUTED},
                    "state")
        return {"app": package, "commits": 4, "delivered": delivered["deliveries"]}

    def serial_checks(self, cli: Path) -> dict:
        root = self.work / "serial"
        root.mkdir()
        contract = root / "contract"
        shutil.copytree(self.source / FIXTURES / "spend-approval", contract)
        program, candidate, optimized = root / "program.zcve", root / "candidate.zcve", root / "optimize.json"
        exported = self.report_of("export-program", [cli, "contract", "export-program", contract, "--out", program,
                                                     "--format", "json"], cwd=root)
        self.expect(exported, {"status": "exported", "version": 1}, "export-program")
        if exported["program"]["sha256"] != rc_package.sha256(program):
            raise JourneyError("export-program reported another program than it wrote")
        improved = self.report_of("optimize", [cli, "optimize", "--program", program, "--candidate-out", candidate,
                                               "--receipt", optimized], cwd=root)
        self.expect(improved, {"status": "improved"}, "optimize")
        self.expect(self.report_of("optimize-replay", [cli, "transform", "replay", "--receipt", optimized,
                                                       "--original", program, "--candidate", candidate], cwd=root),
                    {"status": "replayed"}, "optimize replay")
        original, kernel = ARTIFACTS / "boolean-kernel-original.zcve", ARTIFACTS / "boolean-kernel-candidate.zcve"
        receipt = root / "receipt.json"
        check = [cli, "transform", "check", "--original", original, "--candidate", kernel]
        self.expect(self.report_of("transform-check", [*check, "--receipt", receipt], cwd=root),
                    {"status": "equivalent"}, "transform check")
        replay = [cli, "transform", "replay", "--original", original, "--candidate", kernel]
        self.expect(self.report_of("transform-replay", [*replay, "--receipt", receipt], cwd=root),
                    {"status": "replayed"}, "transform replay")
        kept = receipt.read_bytes()
        again = self.report_of("transform-check-existing-receipt", [*check, "--receipt", receipt], cwd=root,
                               expected=1)
        self.expect(again, {"status": "receipt-exists"}, "transform check onto an existing receipt")
        if receipt.read_bytes() != kept:
            raise JourneyError("a refused check changed the existing receipt")
        return {"program_sha256": exported["program"]["sha256"], "receipt_sha256": hashlib.sha256(kept).hexdigest(),
                "original": original, "check": check, "replay": replay, "receipt": receipt}

    def platform_routes(self, cli: Path, serial: dict) -> dict:
        root = self.work / "platform"
        root.mkdir()
        parallel = root / "parallel.json"
        command = [*serial["check"], "--offline-parallel", "--receipt", parallel, "--containment-backend", "systemd"]
        counter = self.counter
        if counter is None:
            raise JourneyError("the durable-counter app was not created")
        conformance = root / "rust-conformance.json"
        self.expect(self.report_of("synth-run", [cli, "synth", "run", "synthesis.json", "--out", "synthesized",
                                                 "--check"], cwd=counter), {"status": "current"}, "synth run")
        verify = [cli, "synth", "verify", "synthesis.json", "--out", "synthesized", "--receipt", conformance]
        if self.host == MAC:
            result = self.report_of("offline-parallel", command, cwd=root, expected=2)
            if result != macos_refusal("zeno-fcis/transform-result/1", "systemd", "group-resources/1",
                                       "containment-unavailable:systemd:platform-unqualified"):
                raise JourneyError(f"offline-parallel on macOS: {result}")
            session = root / "session"
            loop = self.report_of("loop-open", [cli, "loop", "open", "--original", serial["original"], "--session",
                                                session, "--containment-backend", "auto"], cwd=root, expected=2)
            acquisition = loop.pop("acquisition", None)
            if loop != macos_refusal("zeno-fcis/transform-loop/1", "posix", "finite-data/1",
                                     "containment-unavailable:posix:platform-unqualified") \
                    or not isinstance(acquisition, dict) \
                    or acquisition.get("observed_intake", "missing") is not None \
                    or acquisition.get("accounting_complete") is not False:
                raise JourneyError(f"loop open on macOS: {loop}, acquisition {acquisition}")
            if files_under(root) or session.exists():
                raise JourneyError(f"an unavailable contained route wrote {files_under(root)}")
            self.expect(self.report_of("synth-verify", verify, cwd=counter),
                        {"status": "passed", "authority": "none", "runtime_platform": "macos"}, "synth verify")
            if not conformance.is_file():
                raise JourneyError("synth verify wrote no receipt")
            return {"offline_parallel": result, "loop_open": {**loop, "acquisition": acquisition},
                    "synth_verify": "passed"}
        self.expect(self.report_of("offline-parallel", command, cwd=root), {"status": "equivalent", "backend": "systemd"},
                    "offline-parallel")
        if parallel.read_bytes() != serial["receipt"].read_bytes():
            raise JourneyError("the contained receipt differs from the serial one")
        self.expect(self.report_of("offline-parallel-replay", [*serial["replay"], "--receipt", parallel], cwd=root),
                    {"status": "replayed"}, "offline-parallel replay")
        self.expect(self.report_of("synth-verify", verify, cwd=counter), {"status": "passed"}, "synth verify")
        if not conformance.is_file():
            raise JourneyError("synth verify wrote no receipt")
        return {"offline_parallel": "equivalent; the receipt equals the serial receipt and replays",
                "containment_backend": "systemd", "synth_verify": "passed"}

    def main(self) -> None:
        if not self.archive.is_file():
            raise JourneyError(f"no archive {self.archive.name} in {self.release}")
        self.report["archive"] = {"name": self.archive.name, "sha256": rc_package.sha256(self.archive)}
        self.report["host_facts"] = self.host_facts()
        cli = self.install()
        self.report["executable"] = self.executable_facts(cli)
        self.report["new_without_source"] = self.without_source(cli)
        self.report["readme_apps"] = self.readme_apps(cli)
        self.report["store"] = self.store_journey(cli)
        serial = self.serial_checks(cli)
        self.report["serial"] = {key: serial[key] for key in ("program_sha256", "receipt_sha256")}
        self.report["platform_routes"] = self.platform_routes(cli, serial)
        self.report["status"] = "passed"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--release", type=Path, required=True)
    parser.add_argument("--source", type=Path, default=ROOT)
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    options = parser.parse_args(argv)
    work = options.work_dir.resolve()
    if work.exists():
        print(f"installed-cli: FAIL: {work} must be new", file=sys.stderr)
        return 1
    work.mkdir(parents=True)
    try:
        run = Journey(options.release.resolve(), options.source.resolve(), work)
    except JourneyError as error:
        print(f"installed-cli: FAIL: {error}", file=sys.stderr)
        return 1
    try:
        run.main()
    except (JourneyError, OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError,
            tarfile.TarError, custody.CustodyError, rc_package.RcError) as error:
        run.report.update(status="failed", error=str(error))
        print(f"installed-cli: FAIL: {error}", file=sys.stderr)
        return 1
    finally:
        options.report.parent.mkdir(parents=True, exist_ok=True)
        options.report.write_text(json.dumps(run.report, indent=2, sort_keys=True, default=str) + "\n")
    print(f"installed-cli: PASS ({run.host}, {len(run.steps)} steps); report: {options.report}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
