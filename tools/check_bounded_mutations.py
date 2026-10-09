#!/usr/bin/env python3
"""Kill a fixed, reviewable set of bounded-state faults using public acceptance tests.

Compiles each mutation in a temporary source copy. A compilation failure is not
counted as a killed mutation. The unmodified baseline must pass first.
"""
from pathlib import Path
import os
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
CRATE = Path("crates/zeno-fcis-collections/src")
MUTATIONS = (
    ("item equality refused", "bounded.rs", "if item > l.max_item_bytes", "if item >= l.max_item_bytes"),
    ("capacity equality refused", "bounded.rs", "if count > l.max_entries as usize", "if count >= l.max_entries as usize"),
    ("snapshot equality refused", "bounded.rs", "if total > l.max_snapshot_bytes", "if total >= l.max_snapshot_bytes"),
    ("map replacement retains old charge", "bounded.rs", ".checked_sub(old_size)", ".checked_sub(0 * old_size)"),
    ("FIFO inserts at front", "bounded.rs", "items.push(value);", "items.insert(0, value);"),
    ("ID conflict treated as retry", "pipe.rs", "old == &message", "old.id == message.id"),
    ("ack compares only ID", "pipe.rs", "if head != message", "if head.id != message.id"),
    ("profile check bypassed", "pipe.rs", "if message.profile != self.profile", "if false && message.profile != self.profile"),
)
COMMAND = ["cargo", "+1.97.1", "test", "-p", "zeno-fcis-collections", "--all-features", "--locked", "--offline", "--test", "bounded_contract"]


def run(root, env):
    return subprocess.run(COMMAND, cwd=root, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=300)


def main():
    env = dict(os.environ)
    env["CARGO_TARGET_DIR"] = str(ROOT / "target" / "bounded-mutations")
    baseline = run(ROOT, env)
    if baseline.returncode:
        raise SystemExit("baseline failed:\n" + baseline.stdout)
    with tempfile.TemporaryDirectory(prefix="bounded-mutations-") as directory:
        copy = Path(directory) / "source"
        shutil.copytree(ROOT, copy, ignore=shutil.ignore_patterns(".git", "target", "node_modules", "__pycache__"))
        for name, filename, before, after in MUTATIONS:
            path = copy / CRATE / filename
            original = path.read_text()
            if before not in original:
                raise SystemExit(f"mutation anchor missing: {name}")
            path.write_text(original.replace(before, after))
            result = run(copy, env)
            path.write_text(original)
            if result.returncode == 0:
                raise SystemExit(f"SURVIVED: {name}")
            if "test result: FAILED." not in result.stdout:
                raise SystemExit(f"mutation did not reach failing tests: {name}\n{result.stdout}")
            print(f"KILLED: {name}", flush=True)
    print(f"PASS: baseline and {len(MUTATIONS)} executed source mutations")


if __name__ == "__main__":
    main()
