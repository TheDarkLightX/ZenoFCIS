#!/usr/bin/env python3
"""Exercise extended scalar contracts through generation, an app and SQLite.

The appended escrow cases cannot match the nonnegative amount domain. The
retained independent-of-this-test examples therefore keep their exact labels.
This is regression evidence, not exhaustive verification of the escrow domain.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def run(command: list[str], *, cwd: Path = ROOT, expected: int = 0) -> subprocess.CompletedProcess:
    result = subprocess.run(command, cwd=cwd, text=True, capture_output=True, timeout=900)
    if result.returncode != expected:
        raise RuntimeError(f"{command[:4]} exited {result.returncode}: {result.stdout[-2000:]} {result.stderr[-2000:]}")
    return result


def report(command: list[str], *, expected: int = 0) -> dict:
    return json.loads(run(command, expected=expected).stdout)


def fixture(destination: Path, extra_cases: int, *, extra_outputs: int = 0) -> None:
    source = ROOT / "crates/zeno-fcis-cli/tests/fixtures/escrow"
    for name in ["project.zeno", "v2/policy.json", "tests/decision-examples.txt"]:
        target = destination / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source / name, target)
    path = destination / "v2/policy.json"
    policy = json.loads(path.read_text())
    # Negative amounts cannot match this fixture's declared Money domain.
    padding = [{"rule": f"unreachable negative amount {i}",
                "when": f"amount == -{i} && now == -{i}",
                "class": "Reject", "reason": 203, "post": {}, "outbox": []}
               for i in range(1, extra_cases + 1)]
    # Independently computed, bounded outputs exercise the enlarged output
    # capacity too. The impossible guards preserve all retained examples;
    # held-held+i is defined across the entire admitted Money domain.
    state_names = ["status", "held", "funded", "to_seller", "to_buyer",
                   "funded_at", "shipped_at"]
    for i, case in enumerate(padding[:extra_outputs]):
        post = {str(110 + field): name for field, name in enumerate(state_names)}
        post["111"] = f"held - held + {i + 1}"
        case.update({"class": "Accept", "reason": None, "post": post})
    policy["cases"] = padding + policy["cases"]
    path.write_text(json.dumps(policy, indent=2) + "\n")


def check(cli: Path, work: Path, target: Path) -> dict:
    contract = work / "escrow-wide"
    original = json.loads((ROOT / "crates/zeno-fcis-cli/tests/fixtures/escrow/v2/policy.json").read_text())
    fixture(contract, 67 - len(original["cases"]), extra_outputs=20)
    generated = report([str(cli), "generate", "contract", str(contract), "--format", "json"])
    nodes = generated["summary"]["program_nodes"]
    outputs = generated["summary"]["outputs"]
    if not (256 < nodes <= 4096 and 16 < outputs <= 64):
        raise RuntimeError(f"67-case escrow does not exercise extended capacity: {nodes}")
    program = work / "wide.zcve"
    report([str(cli), "contract", "export-program", str(contract), "--out", str(program), "--format", "json"])
    app = work / "app"
    run([str(cli), "new", str(app), "--contract", str(contract), "--source", str(ROOT)])
    manifest = app / "Cargo.toml"
    cargo = ["cargo", "+1.97.1"]
    tests = run([*cargo, "test", "--offline", "--manifest-path", str(manifest),
                 "--target-dir", str(target)])
    if "0 failed" not in tests.stdout or "every_example_is_the_authority_decision" not in tests.stdout:
        raise RuntimeError("generated app did not execute its independent example regressions")
    run([*cargo, "build", "--locked", "--offline", "--manifest-path", str(manifest),
         "--target-dir", str(target)])
    import tomllib
    package = tomllib.loads(manifest.read_text())["package"]["name"]
    binary = target / "debug" / package
    store = work / "escrow.sqlite"
    genesis = report([str(binary), "init", str(store), "--format", "json"])
    fields = ["action=Fund", "amount=5000", "refund=0", "caller=Buyer", "now=1000000"]
    before = store.read_bytes()
    preview = report([str(binary), "decide", str(store), *fields, "--format", "json"])
    if store.read_bytes() != before:
        raise RuntimeError("decision preview wrote to the store")
    committed = report([str(binary), "submit", str(store), *fields, "--format", "json"])
    current = report([str(binary), "state", str(store), "--format", "json"])
    history = report([str(binary), "history", str(store), "--format", "json"])
    expected_state = {"status": "Funded", "held": 5000, "funded": 5000,
                      "to_seller": 0, "to_buyer": 0, "funded_at": 1000000,
                      "shipped_at": 0}
    if genesis != {"status": "initialized", "contract_version": 1,
                   "commits": 0, "pending": 0, "upgrades": 0}:
        raise RuntimeError("genesis did not initialize the expected empty store")
    for value, status, commit in [(preview, "decided", None), (committed, "committed", 1)]:
        if (value.get("status"), value.get("class"), value.get("reason"),
            value.get("commit"), value.get("state"), value.get("deliveries")) != (
                status, "Accept", None, commit, expected_state, []):
            raise RuntimeError("fund decision differs from the independent escrow example")
    expected_entry = {"contract_version": 1,
                      "command": {"action": "Fund", "amount": 5000, "refund": 0},
                      "context": {"caller": "Buyer", "now": 1000000},
                      "class": "Accept", "reason": None, "commit": 1,
                      "state": expected_state, "deliveries": []}
    if current.get("state") != expected_state or current.get("commits") != 1:
        raise RuntimeError("reopening changed the committed state or commit count")
    if history.get("entries") != [expected_entry] or history.get("commits") != 1:
        raise RuntimeError("history did not replay the exact committed decision")
    # This retained fixture compiles to exactly 4,097 nodes with 661 padding
    # cases; pin the CLI boundary independently of the library capacity test.
    oversized = work / "too-large"
    fixture(oversized, 661)
    refused = report([str(cli), "generate", "contract", str(oversized), "--format", "json"], expected=1)
    if (refused.get("error", {}).get("message") !=
            "6 outputs and 4097 program nodes exceed 64 and 4096"
            or (oversized / "src").exists()
            or (oversized / "v2/policy.zcve").exists()):
        raise RuntimeError("oversized CLI contract did not refuse without generation")
    return {"status": "passed", "cases": 67, "program_nodes": nodes, "outputs": outputs,
            "genesis": genesis, "preview": preview, "committed": committed,
            "state": current, "replayed_history": history,
            "oversized_refusal": refused,
            "tests_sha256": hashlib.sha256(tests.stdout.encode()).hexdigest(),
            "scope": "extended scalar CLI route and exact retained escrow examples"}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path)
    parser.add_argument("--work-dir", type=Path)
    args = parser.parse_args()
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
    cli = args.cli
    if cli is None:
        run(["cargo", "+1.97.1", "build", "--locked", "--offline", "-p", "zeno-fcis-cli"])
        cli = target / "debug/zeno-fcis"
    if args.work_dir:
        args.work_dir.mkdir(exist_ok=False, parents=True)
        result = check(cli.resolve(), args.work_dir.resolve(), target)
        (args.work_dir / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    else:
        with tempfile.TemporaryDirectory(prefix="g3-app-") as temporary:
            result = check(cli.resolve(), Path(temporary), target)
    print(json.dumps(result))


if __name__ == "__main__":
    main()
