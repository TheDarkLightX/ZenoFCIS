#!/usr/bin/env python3
"""Build and exercise the actual CLI-generated application as an isolated consumer.

Only the new temporary consumer lock is resolved. External dependency versions
must match the repository lock. No Lean installation or invocation is involved.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def run(command: list[str], cwd: Path, *, capture: bool = False) -> str:
    result = subprocess.run(command, cwd=cwd, check=True, text=True,
                            stdout=subprocess.PIPE if capture else None)
    return result.stdout if capture else ""


def check(directory: Path) -> None:
    app = directory / "counter"
    run(["cargo", "+1.97.1", "fetch", "--locked"], ROOT)
    packaged = set(run(["cargo", "+1.97.1", "package", "-p", "zeno-fcis-cli", "--list",
                        "--allow-dirty", "--locked", "--offline"], ROOT, capture=True).splitlines())
    template = ROOT / "crates/zeno-fcis-cli/templates/durable-counter"
    for source in template.rglob("*"):
        if source.is_file() and str(source.relative_to(ROOT / "crates/zeno-fcis-cli")) not in packaged:
            raise RuntimeError(f"CLI package omits template resource: {source}")
    run(["cargo", "+1.97.1", "run", "-p", "zeno-fcis-cli", "--locked", "--offline", "--",
         "new", str(app), "--template", "durable-counter"], ROOT)
    manifest = app / "Cargo.toml"
    packages = {}
    for package in sorted((ROOT / "crates").glob("*/Cargo.toml")):
        document = tomllib.loads(package.read_text())
        packages[document["package"]["name"]] = (package.parent, document)

    def dependencies(document: dict) -> set[str]:
        names = set()
        for key, value in document.items():
            if key in ("dependencies", "dev-dependencies", "build-dependencies"):
                names.update(set(value) & packages.keys())
            elif isinstance(value, dict):
                names.update(dependencies(value))
        return names

    pending = dependencies(tomllib.loads(manifest.read_text()))
    selected = set()
    while pending:
        name = pending.pop()
        if name not in selected:
            selected.add(name)
            # Build/runtime edges suffice; package tests are not part of the consumer.
            document = {key: value for key, value in packages[name][1].items() if key != "dev-dependencies"}
            pending.update(dependencies(document) - selected)
    with manifest.open("a") as output:
        output.write("\n[patch.crates-io]\n")
        for name in sorted(selected):
            output.write(f'{name} = {{ path = {json.dumps(str(packages[name][0]))} }}\n')
    # Seed the new consumer from the reviewed lock, preserving transitive versions.
    shutil.copyfile(ROOT / "Cargo.lock", app / "Cargo.lock")
    run(["cargo", "+1.97.1", "metadata", "--offline", "--format-version", "1"], app, capture=True)
    locked = tomllib.loads((ROOT / "Cargo.lock").read_text())
    actual = tomllib.loads((app / "Cargo.lock").read_text())
    identity = lambda item: (item["name"], item["version"], item.get("source"), item.get("checksum"))
    allowed = {identity(item) for item in locked["package"] if "source" in item}
    unexpected = [identity(item) for item in actual["package"] if "source" in item and identity(item) not in allowed]
    if unexpected:
        raise RuntimeError(f"consumer changed the locked external graph: {unexpected}")
    run(["cargo", "+1.97.1", "fmt", "--all", "--", "--check"], app)
    run(["cargo", "+1.97.1", "clippy", "--all-targets", "--locked", "--offline", "--", "-D", "warnings"], app)
    run(["cargo", "+1.97.1", "test", "--locked", "--offline"], app)
    run(["cargo", "+1.97.1", "run", "--locked", "--offline", "--", str(directory / "counter.sqlite")], app)
    print("generated application: isolated consumer, locked dependencies, lifecycle and decision table passed")


def main() -> None:
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
        check(directory)
    else:
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-generated-application-") as directory:
            check(Path(directory))


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"generated application: FAIL: {error}", file=sys.stderr)
        sys.exit(1)
