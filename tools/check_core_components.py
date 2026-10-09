#!/usr/bin/env python3
"""Check every supported seed instance through existing F1/F2/Authority routes.

--static checks data and independent examples only. The execution mode requires
an externally granted test slot and a built CLI; it never builds that CLI.
A successful report is bounded execution evidence, not a family kernel proof.
"""
from __future__ import annotations

import argparse
import importlib.util
import itertools
import json
import os
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

import instantiate_core as core

REFERENCE = core.ROOT / "tools/test_data/core_components/reference.py"
REFERENCE_SHA256 = "126b6fb20905c13ce4133f15602b1d966a131826469af9cfabd93b360cad68b9"


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def expected_cases():
    require(core.digest(REFERENCE.read_bytes()) == REFERENCE_SHA256,
            "independent reference changed; obtain a new independent review")
    spec = importlib.util.spec_from_file_location("core_independent_reference", REFERENCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return list(module.cases())


def example_line(row):
    expected = row["expected"]
    numbers = lambda values: " ".join(str(int(value)) for value in values)
    reason = "-" if expected["reason"] is None else str(expected["reason"])
    require(expected["deliveries"] == [], "the seed profile has no deliveries")
    return f'{numbers(row["input"])} | {expected["class"].lower()} {reason} {numbers(expected["post"])} | -'


def static_check():
    rows = expected_cases()
    instances = []
    for family in core.FAMILIES:
        spec = core.definition(family)
        for parameters in spec["instances"]:
            files = core.source(family, parameters)
            manifest = core.read_json(files["core-instance.json"].decode())
            expected = [row for row in rows if row["family"] == family.replace("-", "_")
                        and row["parameters"] == parameters]
            written = [line for line in files["tests/decision-examples.txt"].decode().splitlines()
                       if line and not line.startswith("#")]
            require(written == [example_line(row) for row in expected],
                    f"{family} {parameters}: example corpus differs from independent reference")
            actual_inputs = [tuple(map(int, row["input"])) for row in expected]
            require(actual_inputs == list(itertools.product(*manifest["input_domains"])),
                    f"{family} {parameters}: independent inputs do not cover the exact full product")
            require(len(written) == manifest["input_tuples"], "input count differs")
            state_width = spec["state_width"]
            command_offset = state_width
            actions = manifest["input_domains"][command_offset]
            for action in actions:
                require(any(row["input"][command_offset] == action and
                            row["expected"]["class"] == "Accept" for row in expected),
                        f"{family}: command {action} lacks a positive witness")
            require(any(row["expected"]["class"] == "Reject" for row in expected),
                    f"{family}: no business refusal witness")
            for row in expected:
                if row["expected"]["class"] == "Reject":
                    require(row["expected"]["post"] == row["input"][:state_width],
                            "reference reject changes state")
            instances.append(manifest)
    require(len(instances) == 21 and sum(x["input_tuples"] for x in instances) == 6158,
            "supported family range drifted")
    return instances


def run(command, cwd, log, environment, expected_exit=0):
    with log.open("wb") as stream:
        result = subprocess.run(list(map(str, command)), cwd=cwd, env=environment,
                                stdout=stream, stderr=subprocess.STDOUT, check=False)
    require(result.returncode == expected_exit,
            f"command exited {result.returncode}, expected {expected_exit}; see {log}")


def review(cli, contract, packet, log, environment, mutant=False):
    # F2 returns 1 for findings; an absent/malformed packet is never a pass.
    run([cli, "contract", "review", contract, "--out", packet, "--format", "json"],
        core.ROOT, log, environment, expected_exit=1 if mutant else 0)
    result = core.read_json(packet.read_text())
    summary = result["summary"]
    require(result["inputs"]["construction"] == "full-domain", "F2 used boundary sampling")
    if mutant:
        require(summary["law_refusal_findings"] > 0, "mutant did not cause a lawful-prestate law refusal")
        require(summary["refusals"]["by_class"].get("law", 0) > 0, "mutant lacked a law refusal")
    else:
        require(summary["refusals"]["count"] == 0, "technical refusal on a raw input")
        require(summary["findings"] == 0 and summary["examples"]["disagreements"] == 0,
                "F2 found a semantic mismatch")
    return result


def execute(cli, work, target, instances):
    import check_generated_application as applications
    environment = {**os.environ, "CARGO_BUILD_JOBS": "1", "CARGO_INCREMENTAL": "0",
                   "CARGO_PROFILE_DEV_DEBUG": "0", "CARGO_PROFILE_TEST_DEBUG": "0",
                   "CARGO_TARGET_DIR": str(target), "RUST_TEST_THREADS": "1"}
    packages = {tomllib.loads(path.read_text())["package"]["name"]: path.parent
                for path in sorted((core.ROOT / "crates").glob("*/Cargo.toml"))}
    version = tomllib.loads((core.ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    reports = []
    for manifest in instances:
        family, parameters = manifest["family"], manifest["parameters"]
        name = family + "-" + core.instance_id(parameters)
        directory = work / name
        directory.mkdir()
        contract, app = directory / "contract", directory / "app"
        core.instantiate(family, parameters, contract)
        print(f"checking {name}: {manifest['input_tuples']} raw tuples", flush=True)
        run([cli, "generate", "contract", contract, "--format", "json"], core.ROOT,
            directory / "generate.log", environment)
        run([cli, "generate", "contract", contract, "--check"], core.ROOT,
            directory / "regenerate.log", environment)
        checked = review(cli, contract, directory / "review.json", directory / "review.log", environment)
        require(checked["inputs"]["count"] == manifest["input_tuples"] and
                checked["inputs"]["domain_size"] == str(manifest["input_tuples"]) and
                checked["summary"]["examples"]["compared"] == manifest["input_tuples"],
                f"{name}: F2 domain/examples differ from complete independent product")
        run([cli, "new", app, "--contract", contract, "--source", core.ROOT], core.ROOT,
            directory / "scaffold.log", environment)
        binding = applications.check_generated_binding(app, packages, version)
        consumer = tomllib.loads((app / "Cargo.toml").read_text())["package"]
        allowed = {(key, version): path / "Cargo.toml" for key, path in packages.items()}
        allowed[(consumer["name"], consumer["version"])] = app / "Cargo.toml"
        graph = applications.resolve_reviewed_graph(app, allowed, environment)
        run(["cargo", "+1.97.1", "test", "--locked", "--offline", "--", "--show-output"],
            app, directory / "app-tests.log", environment)
        test_log = (directory / "app-tests.log").read_text()
        require(applications.decision_example_counts(test_log) == [str(manifest["input_tuples"])],
                f"{name}: generated tests did not check every independent example")
        required_tests = ("every_example_is_the_authority_decision", "genesis_publishes_only_the_contract_genesis",
                          "examples_run_as_one_persistent_session", "every_written_form_of_an_example_reads_alike")
        for test in required_tests:
            require(f"test {test} ... ok" in test_log, f"{name}: generated test {test} did not pass")
        reports.append({"instance": manifest, "review_sha256": core.digest((directory / "review.json").read_bytes()),
                        "generated_sha256": {path: core.digest((contract / path).read_bytes()) for path in
                                              ("src/v2_contract.rs", "v2/schema.zcve", "v2/policy.zcve")},
                        "program": core.read_json((directory / "generate.log").read_text())["summary"],
                        "app_tests_sha256": core.digest((directory / "app-tests.log").read_bytes()),
                        "binding": binding, "consumer_lock_sha256": graph["lock_sha256"]})
    mutants = []
    for family in core.FAMILIES:
        directory = work / (family + "-mutant")
        core.instantiate(family, core.definition(family)["instances"][0], directory)
        policy_path = directory / "v2/policy.json"
        policy = core.read_json(policy_path.read_text())
        if family == "reservation-pool":
            next(case for case in policy["cases"] if case["when"] == "action == 150")["post"]["111"] = "reserved"
        elif family == "rate-limiter":
            policy["cases"][-1]["post"]["111"] = "used"
        else:
            next(case for case in policy["cases"] if case["when"] == "action == 161")["post"]["111"] = "v0"
        policy_path.write_text(json.dumps(policy, indent=2) + "\n")
        checked = review(cli, directory, directory / "review.json", directory / "review.log", environment, mutant=True)
        mutants.append({"family": family, "law_refusal_findings": checked["summary"]["law_refusal_findings"],
                        "review_sha256": core.digest((directory / "review.json").read_bytes())})
    return {"instances": reports, "law_violating_mutants": mutants}


def finite_proof(cli, work):
    """Reissue and replay current-build certificates without replacing stored ones."""
    import prove_core_families as proof

    require(cli.is_file(), "CLI does not exist; build it from this source under the assigned slot")
    require(core.ROOT not in work.parents and work != core.ROOT, "keep qualification outside the source tree")
    require(Path('/dev/shm') not in work.parents and work != Path('/dev/shm'), "use a disk-backed directory")
    native = REFERENCE.with_name('native-report.json')
    require(core.digest(native.read_bytes()) == proof.NATIVE_REPORT_SHA256, "inherited native report changed")
    initial = proof.snapshot_inputs(cli)
    files = dict(initial['runtime']['files'])
    for family in core.FAMILIES:
        files.update(initial['families'][family])
    for name in ('tools/test_core_family_proofs.py', 'tools/test_core_components.py',
                 'tools/test_data/core_components/native-report.json'):
        files[name] = core.digest((core.ROOT / name).read_bytes())
    work.mkdir(exist_ok=False)
    stage = work / 'source'
    environment = {**os.environ, 'PYTHONDONTWRITEBYTECODE': '1'}
    try:
        # Only declared inputs are copied. The old certificates stay untouched;
        # each issue/replay below binds these exact bytes and the current CLI.
        for name, expected in sorted(files.items()):
            source = core.ROOT / name
            require(not source.is_symlink(), f'symlinked qualification input: {name}')
            destination = stage / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, destination)
            require(core.digest(destination.read_bytes()) == expected, f'input changed while copying: {name}')
        proof.require_unchanged_inputs(cli, initial, root=stage)
        for mode in ('issue', 'replay', 'qualify'):
            command = [sys.executable, stage / 'tools/prove_core_families.py', mode,
                       '--cli', cli, '--work-dir', work / mode]
            if mode == 'issue':
                command += ['--native-report', stage / 'tools/test_data/core_components/native-report.json']
            run(command, stage, work / f'{mode}.log', environment)
            proof.require_unchanged_inputs(cli, initial, root=stage)
        for script in ('test_core_family_proofs.py', 'test_core_components.py'):
            run([sys.executable, stage / 'tools' / script], stage,
                work / f'{script}.log', environment)
        run([sys.executable, stage / 'tools/check_core_components.py', '--static'],
            stage, work / 'static.log', environment)
        replayed = core.read_json((work / 'replay/report.json').read_text())
        require(replayed['status'] == 'passed' and replayed['mode'] == 'replay', 'actual replay did not pass')
    finally:
        proof.require_unchanged_inputs(cli, initial)
        require(all(core.digest((core.ROOT / name).read_bytes()) == expected
                    for name, expected in files.items()), 'qualification input changed during checking')
    require(all(core.digest((stage / name).read_bytes()) == expected
                for name, expected in files.items()), 'isolated qualification source changed during checking')
    proof.require_unchanged_inputs(cli, initial, root=stage)
    require(proof.load(stage / 'core-components/proofs/runtime-source.json') == initial['runtime'],
            'issued certificates bind a different runtime source inventory')
    for family in core.FAMILIES:
        certificate = proof.load(stage / f'core-components/proofs/{family}.json')
        proof.preflight(certificate, family, cli, initial['runtime'])
        require(replayed['certificates'][family] == core.digest(proof.encoded(certificate)),
                'replay report refers to a different certificate')
    report = {'schema': 'zeno-fcis/core-family-current-build-check/1',
              'status': 'passed', 'authority': 'none', 'owner_adoption': False,
              'instances': 21, 'raw_transition_inputs': 6158,
              'cli_sha256': initial['cli_sha256'],
              'runtime_source_sha256': core.digest(proof.encoded(initial['runtime'])),
              'certificates': replayed['certificates'],
              'genesis': 'inherited exact-artifact native evidence; native tests were not rerun',
              'evidence_directory': str(work)}
    (work / 'report.json').write_text(json.dumps(report, indent=2, sort_keys=True) + '\n')
    print(json.dumps(report, sort_keys=True))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--static", action="store_true")
    parser.add_argument("--finite-proof", action="store_true",
                        help="issue/replay current-build certificates in an isolated source copy")
    parser.add_argument("--cli", type=Path)
    parser.add_argument("--work-dir", type=Path)
    parser.add_argument("--target-dir", type=Path)
    args = parser.parse_args()
    if args.finite_proof:
        require(args.cli and not args.static and not args.target_dir,
                "--finite-proof requires --cli and does not run native builds")
        work = (args.work_dir.resolve() if args.work_dir else
                Path(tempfile.mkdtemp(prefix='zeno-fcis-core-families-')) / 'qualification')
        finite_proof(args.cli.resolve(), work)
        return
    instances = static_check()
    report = {"schema": "zeno-fcis/core-seed-check/1", "authority": "none", "owner_adoption": False,
              "family_theorem": None, "evidence": "static-data-only", "instances": len(instances),
              "raw_tuples": sum(item["input_tuples"] for item in instances),
              "independent_reference_sha256": core.digest(REFERENCE.read_bytes())}
    if not args.static:
        require(all((args.cli, args.work_dir, args.target_dir)), "execution requires --cli, --work-dir and --target-dir")
        cli, work, target = args.cli.resolve(), args.work_dir.resolve(), args.target_dir.resolve()
        require(cli.is_file(), "CLI does not exist; build it under the assigned resource slot")
        require(not any(path == Path('/dev/shm') or Path('/dev/shm') in path.parents for path in (work, target)),
                "use disk-backed test directories")
        work.mkdir(parents=False, exist_ok=False)
        target.mkdir(parents=True, exist_ok=True)
        report.update(execute(cli, work, target, instances))
        report.update(evidence="complete-finite-execution", cli_sha256=core.digest(cli.read_bytes()))
        (work / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({key: value for key, value in report.items() if key != "instances"} |
                     {"instances": len(instances)}, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"core seed check failed: {error}", file=sys.stderr)
        sys.exit(1)
