#!/usr/bin/env python3
"""Compile collection-app defects and require named assertion failures.

Source mutations are restored before proceeding. Synthesized applications are
changed only by rebuilding their producer and emitting a fresh consumer.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

from cargo_source_scope import source_environment
from check_collection_apps import capacity_project

ROOT = Path(__file__).resolve().parents[1]
APP = "crates/zeno-fcis-cli/collection-app/app.rs"
STORE = "crates/zeno-fcis-shell-sqlite/src/collections.rs"
LAYOUT = "crates/zeno-fcis-contract/src/collections/runtime.rs"
WRAP = ["nice", "-n", "19", "ionice", "-c3"]
CONTROLS = [
    ("canonical-deletion", APP,
     "if checked.into_parts().0.is_err() {",
     "if false && checked.into_parts().0.is_err() {",
     "deletion", "canonical_deletion_is_refused_without_publication",
     "Place(10), Cancel(10): raw quantity one passes law 500 but the canonical absent successor has quantity zero; neither preview nor commit may accept it"),

    ("unnamed-write", STORE,
     "if !named.get(map).is_some_and(|keys| keys.contains(key)) {",
     "if false && !named.get(map).is_some_and(|keys| keys.contains(key)) {",
     "token-balances", "shell_unnamed_write_is_refused",
     "key 02..02 is not the decision's named key 01..01; independent commit guard must refuse"),
    ("omitted-post-root", APP,
     '"post_roots":roots(&commit.post_roots)',
     '"post_roots":roots(&commit.post_roots.iter().skip(1).map(|(m,r)|(*m,*r)).collect())',
     "library", "collection_receipts_replay_and_restart",
     "receipt omits the lowest named map's post root; independent replay must reject the genuine emitted journal"),
    ("aliased-key-class", LAYOUT,
     "match seen.iter().position(|k| k == key) {",
     "match seen.iter().position(|_| false) {",
     "token-balances", "self_transfer_preserves_balance_and_supply",
     "the same account in both key fields must accept self-transfer and retain balance 3 and supply 3"),
    ("whole-map-load", APP,
     "let mut loads = 0;",
     "let mut loads = 0;\n        for map in named.keys() {\n            let all = self.store.entries(*map).map_err(unavailable)?;\n            loads += all.len() as u64;\n            for (key,record) in all { loaded.0.insert((*map,key),Some(record)); }\n        }",
     "token-balances", "request_loads_only_named_keys_at_both_sizes",
     "ten seeded entries plus two named loads must fail the exact named-load count of 2"),
    ("capacity-limit", STORE,
     "WHERE map=?1 AND entries+?2 BETWEEN 0 AND capacity",
     "WHERE map=?1",
     "capacity-1", "full_map_capacity_refuses_without_publication",
     "a second distinct zero mint must not create an entry, scalar change, root, retry record or journal row in a capacity-one map"),
    ("declared-capacity-binding", APP,
     "layout.capacities(),",
     "&BTreeMap::from([(110, 2)]),",
     "capacity-1", "full_map_capacity_refuses_without_publication",
     "binding capacity two while the source declares one must fail the full-map regression"),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--work", required=True, type=Path)
    parser.add_argument("--only", nargs="+", choices=[c[0] for c in CONTROLS])
    args = parser.parse_args()
    configured = os.environ.get("GIT_WORK_TREE")
    if configured and Path(configured).resolve() != ROOT:
        parser.error("configured worktree differs from control source")
    args.out.mkdir(parents=True, exist_ok=False)
    args.work.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, CARGO_BUILD_JOBS=os.environ.get("CARGO_BUILD_JOBS", "4"), CARGO_PROFILE_DEV_DEBUG="0",
               CARGO_PROFILE_TEST_DEBUG="0", CARGO_NET_OFFLINE="true", CARGO_INCREMENTAL="0")
    target = Path(env["CARGO_TARGET_DIR"]).resolve()
    cli = target / "debug/zeno-fcis"
    cargo = ["cargo", "+1.97.1"]
    results = []
    commands = []

    def run(label, command):
        size = sum(p.stat().st_size for p in target.rglob("*") if p.is_file())
        if size >= 5_000_000_000:
            raise RuntimeError("collection target reached 5 GB")
        log = args.out / (label + ".log")
        environment = env
        if "--manifest-path" in command:
            app = Path(command[command.index("--manifest-path") + 1]).resolve().parent
            environment = source_environment(app, env)
        with log.open("w") as output:
            process = subprocess.run(WRAP + list(map(str, command)), cwd=ROOT, env=environment,
                                     stdout=output, stderr=subprocess.STDOUT, check=False)
        commands.append({"check": label, "command": list(map(str, command)),
                         "exit_code": process.returncode,
                         "log_sha256": hashlib.sha256(log.read_bytes()).hexdigest()})
        (args.out / "commands.json").write_text(json.dumps(commands, indent=2) + "\n")
        print(f"{label}: exit {process.returncode}", flush=True)
        return process.returncode, log.read_text()

    def build(label):
        code, _ = run(label, cargo + ["build", "--locked", "--offline", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis"])
        if code:
            raise RuntimeError(label)

    def scaffold(label, example):
        app = args.work / label
        project = ROOT / "examples/collections" / example
        if example == "deletion":
            project = ROOT / "test-data/collections/deletion"
        if example == "capacity-1":
            project = args.work / "test-projects" / example
            if not project.exists():
                capacity_project(project, 1)
        code, _ = run(label, [cli, "new", app, "--contract", project, "--source", ROOT])
        if code:
            raise RuntimeError(label)
        return app

    for name, relative, before, after, example, test, witness in CONTROLS:
        if args.only and name not in args.only:
            continue
        path = ROOT / relative
        original = path.read_text()
        if original.count(before) != 1:
            raise RuntimeError(f"nonunique mutation anchor: {name}")
        build(name + "-normal-producer")
        normal = scaffold(name + "-normal-app", example)
        check = cargo + ["test", "--locked", "--offline", "--manifest-path", normal / "Cargo.toml", "--test", "collection_journey", test, "--", "--exact", "--nocapture"]
        baseline, baseline_output = run(name + "-normal", check)
        if baseline or "1 passed" not in baseline_output:
            raise RuntimeError(f"normal test failed: {name}")
        changed = original.replace(before, after)
        row = {"control": name, "path": relative, "test": test, "witness": witness,
               "normal_exit": baseline, "before_sha256": hashlib.sha256(original.encode()).hexdigest(),
               "mutated_sha256": hashlib.sha256(changed.encode()).hexdigest()}
        try:
            path.write_text(changed)
            build(name + "-mutant-producer")
            mutant = scaffold(name + "-mutant-app", example)
            command = cargo + ["test", "--locked", "--offline", "--manifest-path", mutant / "Cargo.toml", "--test", "collection_journey"]
            compiled, _ = run(name + "-build", command + ["--no-run"])
            failed, output = run(name + "-mutant", command + [test, "--", "--exact", "--nocapture"])
            marker = {"canonical-deletion": "canonical deletion was published despite law 500", "unnamed-write": "shell un-named write was accepted", "omitted-post-root": "genuine journal failed independent named-root replay", "capacity-limit": "full map insertion was committed", "declared-capacity-binding": "full map insertion was committed"}.get(name, "assertion")
            row.update(build_exit=compiled, mutant_exit=failed, assertion_marker=marker,
                       killed=compiled == 0 and failed == 101 and marker in output and f"test {test} ... FAILED" in output)
        finally:
            path.write_text(original)
        build(name + "-restored-producer")
        restored_app = scaffold(name + "-restored-app", example)
        restored_check = cargo + ["test", "--locked", "--offline", "--manifest-path", restored_app / "Cargo.toml", "--test", "collection_journey", test, "--", "--exact", "--nocapture"]
        restored, output = run(name + "-restored", restored_check)
        row["restored_exit"] = restored
        row["source_restored"] = path.read_text() == original
        results.append(row)
        (args.out / "results.json").write_text(json.dumps(results, indent=2) + "\n")
        if not row.get("killed") or restored or "1 passed" not in output or not row["source_restored"]:
            raise RuntimeError(f"control failed build/assertion/restoration requirements: {name}")
    print(f"collection app controls: passed ({len(results)} compiled mutants, named assertion failures, restored positives)")


if __name__ == "__main__":
    main()
