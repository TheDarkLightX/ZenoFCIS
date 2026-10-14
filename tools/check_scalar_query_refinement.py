#!/usr/bin/env python3
"""Check actual scalar runtime/query correspondence with the pinned verifier.

The positive proof is unfiltered. Two deliberately targeted negative checks
exercise real getter/adapter bodies; they are not whole-authority fault checks.
This evidence neither checks SMT certificates nor grants application authority.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import signal
import shutil
import subprocess
import tempfile

import check_verus as verifier
from v2_proof_sources import execution_sources
from verus_coverage import observed_inventory, require_coverage

ROOT = Path(__file__).resolve().parents[1]
HARNESS = Path("verification/verus/scalar_node_refinement.rs")
PROFILE = Path("verification/verus/scalar-query.json")
NAMESPACE = "scalar_node_refinement::"
GETTER = Path("crates/zeno-fcis-synthesis/src/finite/ir/program.rs")
ADAPTER = Path("crates/zeno-fcis-cli/src/symbolic/program/library.rs")
OWNERS = (NAMESPACE + "library::impl&%0::of",) + tuple(
    NAMESPACE + "owned_ir::program::impl&%0::" + name
    for name in ("inputs", "outputs", "nodes", "roots")
)


def source_paths() -> tuple[Path, ...]:
    symbolic = Path("crates/zeno-fcis-cli/src/symbolic")
    paths = [
        *execution_sources(ROOT), HARNESS,
        Path("verification/verus/scalar_owned_ir.rs"), PROFILE,
        verifier.PIN, GETTER, ADAPTER, symbolic / "program.rs",
        symbolic / "smt.rs", symbolic / "equivalence.rs",
        symbolic / "equivalence/queries.rs",
        symbolic / "equivalence/queries_spec.rs",
        symbolic / "unsupported.rs",
        Path("crates/zeno-fcis-cli/src/symbolic.rs"),
        Path("crates/zeno-fcis-synthesis/src/finite/ir.rs"),
        Path("crates/zeno-fcis-synthesis/src/finite/evaluation_tests.rs"),
        Path("crates/zeno-fcis-synthesis/src/finite/mod.rs"),
        Path("crates/zeno-fcis-synthesis/src/finite_runtime.rs"),
        Path("verification/verus/authority_v2_sources.json"),
        Path("verification/verus/authority_v2_test_sources.json"),
        Path("tools/check_scalar_query_refinement.py"),
        Path("tools/test_check_scalar_query_refinement.py"),
        Path("tools/check_verus.py"), Path("tools/verus_coverage.py"),
        Path("tools/v2_proof_sources.py"),
        Path("Cargo.toml"), Path("Cargo.lock"), Path("rust-toolchain.toml"),
    ]
    # Cargo compiles native owners outside the direct Verus harness, including
    # the contract compiler used by the eleven-app comparison. Bind their source
    # and manifests as well as the independent app inputs that test reads.
    paths.extend(p.relative_to(ROOT) for p in ROOT.glob("crates/*/src/**/*.rs"))
    paths.extend(p.relative_to(ROOT) for p in ROOT.glob("crates/*/Cargo.toml"))
    cli = Path("crates/zeno-fcis-cli")
    for kind, names in (
        ("templates", ("account-lockout", "agent-treasury-guard", "compliance-gateway",
                       "durable-counter", "inventory-reservation", "order-fulfillment",
                       "prepared-counter", "withdrawal-queue")),
        ("tests/fixtures", ("escrow", "spend-approval")),
    ):
        for name in names:
            paths.extend((cli / kind / name / "project.zeno",
                          cli / kind / name / "v2/policy.json"))
    return tuple(sorted(set(paths)))


def snapshot() -> dict[str, str]:
    result = {}
    for relative in source_paths():
        path = ROOT / relative
        if not path.is_file() or any(p.is_symlink() for p in (path, *path.parents)):
            raise RuntimeError(f"missing or symbolic-link proof source: {relative}")
        result[str(relative)] = verifier.digest(path)
    return result


def positive_accepted(process, report: dict, pin: dict, profile: dict) -> bool:
    report_pin = {**pin, "expected_verified": profile["expected_verified"],
                  "target_functions": profile["target_functions"]}
    return (process.returncode == 0
            and verifier.proof_succeeded(report, pin)
            and verifier.accepted(report, report_pin)
            and not verifier.ERROR_HEADER.search(process.stderr)
            and not verifier.NON_SEMANTIC.search(process.stderr))


def target_refused(process, report: dict, pin: dict, relative: Path,
                   specimen: Path, region: tuple[int, int]) -> bool:
    """Strict targeted postcondition failure, separate from whole-proof refusal."""
    if not isinstance(report, dict):
        return False
    result, tool = report.get("verification-results", {}), report.get("verus", {})
    if not isinstance(result, dict) or not isinstance(tool, dict):
        return False
    if (process.returncode != 1 or tool.get("version") != pin["version"]
            or tool.get("commit") != pin["commit"]
            or str(tool.get("toolchain", "")).split(" ")[0] != pin["rust_toolchain"]
            or ("success" in result and result["success"] is not False)
            or result.get("encountered-error") is not True
            or result.get("encountered-vir-error") is not False
            or result.get("is-verifying-entire-crate") is not False
            or type(result.get("errors")) is not int or result["errors"] != 1
            or type(result.get("verified")) is not int or result["verified"] != 4
            or verifier.NON_SEMANTIC.search(process.stderr)):
        return False
    headers = list(verifier.ERROR_HEADER.finditer(process.stderr))
    if (len(headers) != 2 or headers[0][1] != "postcondition not satisfied"
            or re.fullmatch(r"aborting due to 1 previous error(?:; [1-9][0-9]* warnings? emitted)?",
                            headers[1][1]) is None):
        return False
    block = process.stderr[headers[0].end():headers[1].start()]
    primary = re.search(r"^\s*-->\s+(.+):(\d+):(\d+)\s*$", block, re.M)
    if primary is None:
        return False
    path = Path(primary[1])
    actual = path if path.is_absolute() else specimen / path
    return (os.path.normpath(str(actual)) == os.path.normpath(str(specimen / relative))
            and region[0] <= int(primary[2]) <= region[1])


def require_body_only_change(vir: str, profile: dict, target: str) -> None:
    functions, declarations = observed_inventory(
        vir, NAMESPACE, tuple(profile["body_covered_functions"])
    )
    expected = profile["functions"]
    if set(functions) != set(expected) or declarations != profile["abstract_declarations"]:
        raise ValueError("targeted fault changed the inventory or descriptor declarations")
    changed = [name for name in expected if functions[name] != expected[name]]
    if changed != [target]:
        raise ValueError(f"fault changed unexpected translated functions: {changed}")
    before, after = expected[target], functions[target]
    if (before.get("body_sha256") == after.get("body_sha256")
            or {k: v for k, v in before.items() if k != "body_sha256"}
            != {k: v for k, v in after.items() if k != "body_sha256"}):
        raise ValueError("fault changed a signature or contract rather than only its body")


def check(cache: Path, install: bool, logs: Path) -> dict:
    pin = json.loads((ROOT / verifier.PIN).read_text())
    profile = json.loads((ROOT / PROFILE).read_text())
    before = snapshot()
    tools = verifier.prepare_tools(cache, pin, install)
    environment = {k: v for k, v in os.environ.items()
                   if not k.startswith(("VERUS_", "VARGO_"))
                   and k not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")}
    environment.update(RUSTUP_TOOLCHAIN=pin["rust_toolchain"],
                       VERUS_Z3_PATH=str(tools / "z3"),
                       CARGO_BUILD_JOBS="1", CARGO_INCREMENTAL="0",
                       RUST_TEST_THREADS="1", CARGO_TERM_COLOR="never")
    command = [str(tools / "verus"), "--crate-type=lib", "--edition=2024",
               "--no-cheating", "--no-external-by-default", "--num-threads", "1",
               "-V", "spinoff-all", "--output-json", "--rlimit", "100",
               "--triggers-mode", "silent", "--log", "vir",
               "--log", "vir-option=no_span+no_type+no_fn_details"]
    def run(label: str, args: list[str], cwd=ROOT, env=environment):
        folder = logs / label
        folder.mkdir()
        (folder / "command.json").write_text(json.dumps(
            {"command": args, "cwd": str(cwd)}, indent=2) + "\n")
        process = subprocess.Popen(args, cwd=cwd, env=env, text=True,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   start_new_session=True)
        timed_out = False
        try:
            stdout, stderr = process.communicate(timeout=600)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(process.pid, signal.SIGTERM)
            try:
                stdout, stderr = process.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                stdout, stderr = process.communicate(timeout=10)
        proc = subprocess.CompletedProcess(args, process.returncode, stdout, stderr)
        (folder / "stdout").write_text(proc.stdout)
        (folder / "stderr").write_text(proc.stderr)
        (folder / "process.json").write_text(json.dumps({
            "exit": proc.returncode, "timeout": timed_out,
            "stdout_sha256": verifier.digest(folder / "stdout"),
            "stderr_sha256": verifier.digest(folder / "stderr"),
        }, indent=2) + "\n")
        if timed_out:
            raise RuntimeError(f"checker timeout, incomplete evidence: {label}")
        return proc, folder
    (logs / "source-before.json").write_text(json.dumps(
        before, sort_keys=True, indent=2) + "\n")
    positive, folder = run("positive", [
        *command, "--log-dir", str(logs / "positive/vir"), str(HARNESS)
    ])
    report = json.loads(positive.stdout)
    if not positive_accepted(positive, report, pin, profile):
        raise RuntimeError("unfiltered scalar proof failed exact pinned admission")
    coverage = require_coverage((folder / "vir/crate.vir").read_text(), profile)
    if not set(OWNERS).issubset(coverage):
        raise ValueError("proof omitted an actual getter or library adapter")
    controls = []
    for name, path, anchor, replacement, function, target in (
        ("getter-inputs-as-outputs", GETTER, "&self.inputs", "&self.outputs",
         "inputs", OWNERS[1]),
        ("adapter-inputs-as-outputs", ADAPTER, "inputs: program.inputs(),",
         "inputs: program.outputs(),", "of", OWNERS[0]),
    ):
        specimen = logs / (name + "-source")
        for relative in before:
            source, destination = ROOT / relative, specimen / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
        source = (specimen / path).read_text()
        if source.count(anchor) != 1:
            raise ValueError(f"fault anchor changed: {name}")
        changed = source.replace(anchor, replacement)
        (specimen / path).write_text(changed)
        proc, folder = run(name, [
            *command, "--log-dir", str(logs / name / "vir"), str(HARNESS),
            "--verify-module", "owned_ir::program", "--verify-module", "library",
            "--multiple-errors", "8"
        ], specimen)
        parsed = json.loads(proc.stdout)
        require_body_only_change((folder / "vir/crate.vir").read_text(), profile, target)
        if not target_refused(proc, parsed, pin, path, specimen,
                              verifier.function_lines(changed, function)):
            raise RuntimeError(f"fault survived or failed for another reason: {name}")
        controls.append({"name": name, "targeted": True, "killed": True,
                         "whole_negative_qualification": False,
                         "source_sha256": verifier.digest(specimen / path)})
    native = []
    cargo = ["cargo", f"+{pin['runtime_rust']}", "test", "--locked", "--offline"]
    native_env = {**environment, "RUSTUP_TOOLCHAIN": pin["runtime_rust"]}
    for name, arguments, expected in (
        ("symbolic", ["-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "symbolic::",
                      "--", "--skip", "contract::symbolic::", "--test-threads=1"], 35),
        ("contract-runtime", ["-p", "zeno-fcis-cli", "--bin", "zeno-fcis",
            "contract::symbolic::tests::the_contract_encoding_agrees_with_the_library_on_sampled_tuples",
            "--", "--exact", "--test-threads=1"], 1),
        ("owned-program", ["-p", "zeno-fcis-synthesis", "--lib",
                          "finite::ir::evaluation_tests::", "--", "--test-threads=1"], 7),
    ):
        proc, folder = run(name, cargo + arguments, env=native_env)
        verifier.require_success(proc)
        counts = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", proc.stdout)
        if counts != [(str(expected), "0", "0")]:
            raise RuntimeError(f"native corpus count or refusal changed: {name}: {counts}")
        native.append({"name": name, "passed": expected, "ignored": 0,
                       "stdout_sha256": verifier.digest(folder / "stdout")})
    verifier.verify_tool_files(tools, pin)
    after = snapshot()
    (logs / "source-after.json").write_text(json.dumps(after, sort_keys=True, indent=2) + "\n")
    if before != after:
        raise RuntimeError("source changed while checking scalar correspondence")
    return {"schema": "zeno-fcis/scalar-query-refinement-evidence/1",
            "status": "passed", "source_sha256": before,
            "toolchain": pin, "command": positive.args, "verus_report": report,
            "translated_function_coverage": coverage, "targeted_controls": controls,
            "native_tests": native,
            "scope": "actual admitted scalar eager runtime, ordered outputs/refusals, query construction and printing; nonbinding Step limit",
            "trusted_base": ["reviewed specifications", "Verus translation and erasure",
                             "pinned vstd and Z3", "Rust compilers and platform"],
            "unproved": ["SMT proof certificates", "contract-to-scalar compilation",
                         "shell or effect receivers", "human intent", "stable release"]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--cache", type=Path,
                        default=Path.home() / ".cache/zeno-fcis/verus")
    parser.add_argument("--install", action="store_true")
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    logs = Path(tempfile.mkdtemp(prefix="scalar-query-", dir=args.out.parent))
    try:
        receipt = check(args.cache.resolve(), args.install, logs)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError) as error:
        receipt = {"schema": "zeno-fcis/scalar-query-refinement-evidence/1",
                   "status": "failed", "error": str(error)}
    receipt["logs"] = str(logs)
    args.out.write_text(json.dumps(receipt, sort_keys=True, indent=2) + "\n")
    print(f"Scalar query refinement: {receipt['status']}")
    return 0 if receipt["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

