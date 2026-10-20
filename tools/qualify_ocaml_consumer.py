#!/usr/bin/env python3
"""Reproduce the OCaml scalar consumer qualification from this source alone.

Usage: python3 tools/qualify_ocaml_consumer.py --cli PATH [--work-dir DIR] [--report FILE]

PATH is the `zeno-fcis` CLI built from this checkout. Every stage fails closed:

1. Preconditions: Linux x86_64, `/usr/bin/ocamlopt -version` is 4.14.1, every file
   in `release/ocaml-4.14.1-ubuntu-24.04-amd64.sha256` has its pinned SHA-256,
   `/usr/bin/python3` exists (the transport-fault wrapper's interpreter), the work
   directory is new, and the generated apps get their own target directory,
   `$CARGO_TARGET_DIR/ocaml-consumer`, so same-named apps of other checks never
   share it.
2. The bounded-counter C3 contract from `tools/instantiate_core.py` and a copy of
   `integrations/ocaml/tests/endpoint`: `generate contract`, `generate contract
   --check`, a full-domain `contract review` (16 and 32 inputs, 16 examples, no
   disagreement, refusal or finding), `new --source` this checkout, then the
   generated binding and the reviewed dependency graph.
3. Both apps build with `cargo +1.97.1 build --locked --offline`.
4. The OCaml SDK, `counter` and the SDK's module tests compile with `ocamlopt`;
   the module tests print exactly their pass line.
5. Each app's `scalar_` tests and `every_example_is_the_authority_decision` pass;
   the C3 run writes all 18 native-codec negative frames.
6. `tools/check_ocaml_consumer.py` passes with 16 C3 and 16 endpoint tuples, 24
   native negatives, 11 client negative groups and an UNKNOWN commit recovered.
7. Two checked-i128 overflow controls on copies of the endpoint policy add
   a zero correction formed at the i128 endpoints. On nonoverflowing inputs
   they keep `high + 1` and `low - 1`; on the other half they overflow eagerly.
   Their reviews refuse exactly 16 of 32 inputs with an arithmetic refusal on
   law-consistent states. No app or store is created for them.
8. Clippy with `-D warnings` on both apps.

The report records the source identity, the pin file's hash, `rustc -vV`, the
CLI, app, consumer and correspondence-report hashes and every command with its
exit status and output hashes. It claims neither release qualification nor an
independent review. Without --work-dir the driver works in a temporary folder
and removes it at the end, pass or fail; the report is then kept only when
--report names a file outside it.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import time
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PINS = ROOT / "release" / "ocaml-4.14.1-ubuntu-24.04-amd64.sha256"
OCAMLOPT = Path("/usr/bin/ocamlopt")
OCAML_VERSION = b"4.14.1\n"
MODULE_TESTS_LINE = "OCaml opaque ownership and strict report groups: 2 passed"
NEGATIVE_WORD_CASES = {"unknown-word", "duplicate-word", "missing-word", "bool-domain",
                       "integer-outside-i64", "frame-and-words"}
# The i64 endpoint operations fit the 2.5 evaluator. Force real i128
# overflow on one endpoint, and a zero correction on the other endpoint.
I128_MIN, I128_MAX = -(1 << 127), (1 << 127) - 1
I64_MIN, I64_MAX = -(1 << 63), (1 << 63) - 1
OVERFLOW_CONTROLS = (
    ("max-plus-one", "action == 150", "111",
     f"high + 1 + (({I128_MAX} + (high - {I64_MAX} + 1)) - {I128_MAX})"),
    ("min-minus-one", "true", "110",
     f"low - 1 + (({I128_MIN} + (low - ({I64_MIN}) - 1)) - ({I128_MIN}))"),
)
APPS = {
    "c3": {"inputs": 16, "examples": 16},
    "endpoints": {"inputs": 32, "examples": 16},
}


class QualificationError(RuntimeError):
    """A failed stage; the report records it."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise QualificationError(message)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_pins(path: Path = PINS) -> list[tuple[str, Path]]:
    """`<sha256>  <absolute path>` lines, exactly as `sha256sum --check --strict` reads them."""
    pins = []
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        digest, separator, name = line.partition("  ")
        require(separator == "  " and len(digest) == 64 and all(c in "0123456789abcdef" for c in digest)
                and name.startswith("/") and "\0" not in name, f"{path.name}:{number}: malformed pin")
        pins.append((digest, Path(name)))
    require(bool(pins), f"{path.name}: no pins")
    require(len({name for _, name in pins}) == len(pins), f"{path.name}: a file is pinned twice")
    return pins


def pin_problems(pins: list[tuple[str, Path]]) -> list[str]:
    problems = []
    for digest, path in pins:
        if not path.is_file():
            problems.append(f"missing pinned file {path}")
        elif sha256(path) != digest:
            problems.append(f"pinned file differs: {path}")
    return problems


def review_problems(review: dict, inputs: int, examples: int) -> list[str]:
    """The full-domain review facts every qualified contract must show."""
    summary = review.get("summary", {})
    found = {
        "construction": review.get("inputs", {}).get("construction"),
        "inputs": review.get("inputs", {}).get("count"),
        "examples": summary.get("examples", {}).get("compared"),
        "disagreements": summary.get("examples", {}).get("disagreements"),
        "refusals": summary.get("refusals", {}).get("count"),
        "findings": summary.get("findings"),
    }
    expected = {"construction": "full-domain", "inputs": inputs, "examples": examples,
                "disagreements": 0, "refusals": 0, "findings": 0}
    return [f"{key}: {found[key]!r}, expected {value!r}" for key, value in expected.items() if found[key] != value]


def overflow_problems(review: dict) -> list[str]:
    """A checked-overflow control must be refused on exactly half of its 32 inputs."""
    refusals = review.get("summary", {}).get("refusals", {})
    found = {
        "construction": review.get("inputs", {}).get("construction"),
        "inputs": review.get("inputs", {}).get("count"),
        "arithmetic": refusals.get("by_class", {}).get("arithmetic"),
        "law_consistent_arithmetic": refusals.get("on_law_consistent_states", {}).get("by_class", {}).get("arithmetic"),
        "refusals": refusals.get("count"),
    }
    expected = {"construction": "full-domain", "inputs": 32, "arithmetic": 16, "law_consistent_arithmetic": 16,
                "refusals": 16}
    return [f"{key}: {found[key]!r}, expected {value!r}" for key, value in expected.items() if found[key] != value]


def overflow_policy(policy: dict, guard: str, field: str, expression: str) -> dict:
    """The endpoint policy with one post-state expression replaced; exactly one case matches."""
    changed = json.loads(json.dumps(policy))
    cases = [case for case in changed["cases"] if case.get("when") == guard]
    require(len(cases) == 1, f"endpoint policy has {len(cases)} cases guarded by {guard!r}")
    require(field in cases[0]["post"], f"endpoint case {guard!r} does not set field {field}")
    cases[0]["post"][field] = expression
    return changed


def codec_negative_frames(expected_path: Path = ROOT / "integrations/ocaml/tests/expected.json") -> list[str]:
    expected = json.loads(expected_path.read_text(encoding="utf-8"))
    names = [row["case"] for row in expected["native_negatives"]]
    frames = sorted(f"{name}.frame" for name in names if name not in NEGATIVE_WORD_CASES)
    require(len(names) == 24 and len(frames) == 18, "the frozen corpus no longer names 24 negatives and 18 frames")
    return frames


def consumer_report_problems(report: dict) -> list[str]:
    expected = {"status": "passed", "c3_tuples": 16, "endpoint_tuples": 16, "native_negatives": 24,
                "client_negative_groups": 11}
    problems = [f"{key}: {report.get(key)!r}, expected {value!r}" for key, value in expected.items()
                if report.get(key) != value]
    if not report.get("unknown_submission_recovered"):
        problems.append("the UNKNOWN commit recovery is not recorded")
    return problems


class Qualification:
    def __init__(self, cli: Path, work: Path, report: Path | None):
        self.cli, self.work, self.report_path = cli, work, report
        self.logs = work / "logs"
        target_root = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
        self.target = target_root / "ocaml-consumer"
        self.environment = {**os.environ, "CARGO_TARGET_DIR": str(self.target), "PYTHONDONTWRITEBYTECODE": "1"}
        self.records: list[dict] = []
        self.report: dict = {
            "schema": "zeno-fcis/ocaml-consumer-qualification/1", "status": "unfinished",
            "profile": "zeno-fcis/native-scalar-consumer/1", "records": self.records,
            "release_qualified": False, "independent_review": False,
            "catalogue_certificate": "unclaimed",
        }

    def run(self, name: str, arguments: list, *, cwd: Path = ROOT, expected: int = 0,
            environment: dict | None = None, markers: tuple[str, ...] = (), timeout: int = 3600) -> bytes:
        command = [str(value) for value in arguments]
        started = time.monotonic()
        result = subprocess.run(command, cwd=cwd, env={**self.environment, **(environment or {})},
                                capture_output=True, timeout=timeout, check=False)
        (self.logs / f"{name}.stdout").write_bytes(result.stdout)
        (self.logs / f"{name}.stderr").write_bytes(result.stderr)
        self.records.append({"name": name, "argv": command, "cwd": str(cwd), "exit": result.returncode,
                             "seconds": round(time.monotonic() - started, 3),
                             "stdout_sha256": hashlib.sha256(result.stdout).hexdigest(),
                             "stderr_sha256": hashlib.sha256(result.stderr).hexdigest()})
        require(result.returncode == expected,
                f"{name}: exit {result.returncode}, expected {expected}; {result.stderr[-2048:]!r}")
        text = result.stdout.decode("utf-8", errors="replace")
        for marker in markers:
            require(marker in text, f"{name}: output lacks {marker!r}")
        return result.stdout

    def preconditions(self) -> None:
        require(platform.system() == "Linux" and platform.machine() in ("x86_64", "AMD64"),
                "the OCaml profile is qualified only on Linux x86_64")
        require(self.cli.is_file(), f"no CLI at {self.cli}")
        require(OCAMLOPT.is_file(), "no /usr/bin/ocamlopt; install the pinned packages")
        problems = pin_problems(read_pins())
        require(not problems, "; ".join(problems))
        require(self.run("ocamlopt-version", [OCAMLOPT, "-version"]) == OCAML_VERSION,
                "ocamlopt is not exactly 4.14.1")
        require(Path("/usr/bin/python3").is_file(), "no /usr/bin/python3 for the transport-fault wrapper")
        self.report["preconditions"] = {
            "platform": f"{platform.system()} {platform.machine()}",
            "pins_sha256": sha256(PINS), "pinned_files": len(read_pins()),
            "cli_sha256": sha256(self.cli), "target_dir": str(self.target),
            "rustc": self.run("rustc-version", ["rustc", "+1.97.1", "-vV"]).decode(),
        }

    def source_identity(self) -> dict:
        def git(*arguments: str) -> str:
            result = subprocess.run(["git", "--no-optional-locks", *arguments], cwd=ROOT,
                                    capture_output=True, text=True, check=False)
            return result.stdout.strip() if result.returncode == 0 else f"unavailable: {result.stderr.strip()}"
        manifest = subprocess.run([sys.executable, "-B", "tools/release_manifest.py"], cwd=ROOT,
                                  capture_output=True, check=False)
        return {"commit": git("rev-parse", "HEAD"), "tree": git("rev-parse", "HEAD^{tree}"),
                "tracked_changes": bool(git("status", "--porcelain", "--untracked-files=no")),
                "source_manifest_sha256": hashlib.sha256(manifest.stdout).hexdigest() if manifest.returncode == 0
                else None}

    def application(self, name: str, packages: dict, version: str) -> dict:
        sys.path.insert(0, str(ROOT / "tools"))
        import check_generated_application as applications
        import instantiate_core
        contract = self.work / f"{name}-contract"
        app = self.work / f"ocaml-{name}"
        if name == "c3":
            manifest = instantiate_core.instantiate("bounded-counter", {"C": 3}, contract, include_proof=False)
            require(manifest["finite_family_certificate"] is None and manifest["input_tuples"] == 16,
                    "the C3 instance is not the uncertified 16-tuple contract")
        else:
            shutil.copytree(ROOT / "integrations/ocaml/tests/endpoint", contract)
        self.run(f"{name}-generate", [self.cli, "generate", "contract", contract, "--format", "json"])
        self.run(f"{name}-drift", [self.cli, "generate", "contract", contract, "--check", "--format", "json"])
        review_path = self.work / f"{name}-review.json"
        self.run(f"{name}-review", [self.cli, "contract", "review", contract, "--out", review_path,
                                    "--format", "json"])
        problems = review_problems(json.loads(review_path.read_text()), **APPS[name])
        require(not problems, f"{name} review: " + "; ".join(problems))
        self.run(f"{name}-scaffold", [self.cli, "new", app, "--contract", contract, "--source", ROOT])
        binding = applications.check_generated_binding(app, packages, version)
        consumer = tomllib.loads((app / "Cargo.toml").read_text())["package"]
        allowed = {(key, version): path / "Cargo.toml" for key, path in packages.items()}
        allowed[(consumer["name"], consumer["version"])] = app / "Cargo.toml"
        graph = applications.resolve_reviewed_graph(app, allowed, self.environment)
        return {"app": app, "contract": contract, "package": consumer["name"], "binding": binding, "graph": graph,
                "review_sha256": sha256(review_path), "native": self.target / "debug" / consumer["name"]}

    def ocaml(self) -> Path:
        directory = self.work / "ocaml"
        directory.mkdir()
        for name in ("zeno_scalar.ml", "zeno_scalar.mli", "counter.ml"):
            shutil.copy2(ROOT / "integrations/ocaml" / name, directory / name)
        shutil.copy2(ROOT / "integrations/ocaml/tests/module.ml", directory / "module_tests.ml")
        for name in ("zeno_scalar.mli", "zeno_scalar.ml"):
            self.run(f"ocaml-compile-{name}", [OCAMLOPT, "-c", name], cwd=directory)
        for source, executable in (("counter.ml", "counter"), ("module_tests.ml", "module-tests")):
            self.run(f"ocaml-link-{executable}", [OCAMLOPT, "unix.cmxa", "zeno_scalar.cmx", source, "-o",
                                                  executable], cwd=directory)
        output = self.run("ocaml-module-tests", [directory / "module-tests"], cwd=directory)
        require(output.decode().splitlines() == [MODULE_TESTS_LINE], "the OCaml module tests did not all pass")
        return directory / "counter"

    def native_tests(self, name: str, app: dict) -> Path:
        manifest = app["app"] / "Cargo.toml"
        negatives = self.work / f"{name}-negative-codec"
        self.run(f"{name}-scalar-tests", ["cargo", "+1.97.1", "test", "--manifest-path", manifest, "--locked",
                                          "--offline", "scalar_", "--", "--nocapture", "--test-threads=1"],
                 environment={"ZENO_OCAML_NEGATIVE_DIR": str(negatives)},
                 markers=("scalar_frames_preserve_native_envelopes_and_refuse_aliases",
                          "scalar_frame_cli_matches_native_decisions_and_refusals"))
        self.run(f"{name}-authority-tests", ["cargo", "+1.97.1", "test", "--manifest-path", manifest, "--locked",
                                             "--offline", "every_example_is_the_authority_decision", "--",
                                             "--nocapture", "--test-threads=1"],
                 markers=("decision examples checked: 16",))
        return negatives

    def wide_profile_refusal(self, packages: dict, version: str) -> dict:
        """A generated app with an integer beyond i64 must not advertise the profile."""
        import check_generated_application as applications
        contract = self.work / "wide-contract"
        app = self.work / "wide-consumer"
        shutil.copytree(ROOT / "integrations/ocaml/tests/endpoint", contract)
        declaration = contract / "project.zeno"
        text = declaration.read_text()
        old = "type 106 int High in 9223372036854775806..=9223372036854775807;"
        require(text.count(old) == 1, "the endpoint High declaration changed")
        declaration.write_text(text.replace(old, old.replace("5807;", "5808;")))
        self.run("wide-scaffold", [self.cli, "new", app, "--contract", contract, "--source", ROOT])
        applications.check_generated_binding(app, packages, version)
        consumer = tomllib.loads((app / "Cargo.toml").read_text())["package"]
        allowed = {(key, version): path / "Cargo.toml" for key, path in packages.items()}
        allowed[(consumer["name"], consumer["version"])] = app / "Cargo.toml"
        applications.resolve_reviewed_graph(app, allowed, self.environment)
        self.run("wide-build", ["cargo", "+1.97.1", "build", "--manifest-path", app / "Cargo.toml",
                                "--locked", "--offline"])
        native = self.target / "debug" / consumer["name"]
        report = json.loads(self.run("wide-version", [native, "version", "--format", "json"]))
        require(report["host_profile"] is None, "wide app advertises the i64 consumer profile")
        refused = json.loads(self.run("wide-frame", [native, "frame", "action=Up",
                                                    "probe=9223372036854775808", "authorized=true",
                                                    "--format", "json"], expected=1))
        require(refused["error"]["code"] == "refused"
                and "outside zeno-fcis/native-scalar-consumer/1" in refused["error"]["message"],
                "wide scalar frame did not refuse the unsupported profile")
        return {"high_max": "9223372036854775808", "host_profile": None, "frame_exit": 1}

    def overflow_controls(self, contract: Path) -> list[dict]:
        root = self.work / "overflow-controls"
        root.mkdir()
        records = []
        for name, guard, field, expression in OVERFLOW_CONTROLS:
            directory = root / name
            shutil.copytree(contract, directory)
            policy_path = directory / "v2/policy.json"
            policy = overflow_policy(json.loads(policy_path.read_text()), guard, field, expression)
            policy_path.write_text(json.dumps(policy, indent=2) + "\n")
            self.run(f"overflow-{name}-generate", [self.cli, "generate", "contract", directory, "--format", "json"])
            review_path = directory / "review.json"
            self.run(f"overflow-{name}-review", [self.cli, "contract", "review", directory, "--out", review_path,
                                                 "--format", "json"], expected=1)
            problems = overflow_problems(json.loads(review_path.read_text()))
            require(not problems, f"overflow control {name}: " + "; ".join(problems))
            stores = sorted(str(path) for path in directory.rglob("*.sqlite"))
            require(not stores, f"overflow control {name} created a store: {stores}")
            records.append({"control": name, "arithmetic_refusals": 16, "inputs": 32, "store_created": False})
        return records

    def main(self) -> None:
        self.logs.mkdir(parents=True)
        self.preconditions()
        self.report["source"] = self.source_identity()
        version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
        packages = {}
        for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml")):
            package = tomllib.loads(manifest.read_text())["package"]
            packages[package["name"]] = manifest.parent
        apps = {name: self.application(name, packages, version) for name in APPS}
        for name, app in apps.items():
            self.run(f"{name}-build", ["cargo", "+1.97.1", "build", "--manifest-path", app["app"] / "Cargo.toml",
                                       "--locked", "--offline"])
            require(app["native"].is_file(), f"{name}: no native executable at {app['native']}")
            app["native_sha256"] = sha256(app["native"])
        consumer = self.ocaml()
        negatives = {name: self.native_tests(name, app) for name, app in apps.items()}
        missing = [frame for frame in codec_negative_frames() if not (negatives["c3"] / frame).is_file()]
        require(not missing, f"the C3 tests wrote no negative frame for {missing}")
        correspondence = self.work / "correspondence.json"
        self.run("consumer-check", [sys.executable, "-B", ROOT / "tools/check_ocaml_consumer.py",
                                    "--c3", apps["c3"]["native"], "--endpoints", apps["endpoints"]["native"],
                                    "--consumer", consumer, "--negatives", negatives["c3"],
                                    "--work-dir", self.work / "correspondence", "--report", correspondence,
                                    "--budget-seconds", "600"])
        problems = consumer_report_problems(json.loads(correspondence.read_text()))
        require(not problems, "consumer check: " + "; ".join(problems))
        self.report["overflow_controls"] = self.overflow_controls(apps["endpoints"]["contract"])
        self.report["wide_profile_refusal"] = self.wide_profile_refusal(packages, version)
        for name, app in apps.items():
            self.run(f"{name}-clippy", ["cargo", "+1.97.1", "clippy", "--manifest-path", app["app"] / "Cargo.toml",
                                        "--all-targets", "--locked", "--offline", "--", "-D", "warnings"])
        self.report.update(
            status="passed",
            apps={name: {"package": app["package"], "native_sha256": app["native_sha256"],
                         "review_sha256": app["review_sha256"], "binding": app["binding"]}
                  for name, app in apps.items()},
            consumer_sha256=sha256(consumer), correspondence_report_sha256=sha256(correspondence),
            counts={"c3_tuples": 16, "endpoint_tuples": 16, "native_negatives": 24, "client_negative_groups": 11,
                    "codec_negative_frames": 18, "overflow_controls": 2},
        )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--work-dir", type=Path)
    parser.add_argument("--report", type=Path)
    options = parser.parse_args(argv)
    temporary = options.work_dir is None
    work = (Path(tempfile.mkdtemp(prefix="zeno-fcis-ocaml-")) if temporary else options.work_dir).resolve()
    if not temporary:
        if work.exists():
            print(f"ocaml-qualification: FAIL: {work} must be new", file=sys.stderr)
            return 1
        work.mkdir(parents=True)
    if options.report is not None:
        report_path: Path | None = options.report.resolve()
    else:
        report_path = None if temporary else work / "QUALIFICATION.json"
    qualification = Qualification(options.cli.resolve(), work, report_path)
    try:
        qualification.main()
    except (QualificationError, OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        qualification.report.update(status="failed", error=str(error))
        print(f"ocaml-qualification: FAIL: {error}", file=sys.stderr)
        return 1
    finally:
        if report_path is not None:
            report_path.parent.mkdir(parents=True, exist_ok=True)
            report_path.write_text(json.dumps(qualification.report, indent=2, sort_keys=True, default=str) + "\n")
        if temporary:
            try:
                shutil.rmtree(work)
            except OSError as error:
                print(f"ocaml-qualification: could not remove {work}: {error}", file=sys.stderr)
    print(f"ocaml-qualification: PASS (C3 and endpoint apps, 24 native negatives, 2 overflow controls); "
          f"report: {report_path or 'not kept (pass --report FILE)'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
