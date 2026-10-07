#!/usr/bin/env python3
"""Freeze source, then run the offline native experiment with no shared target.

Not a production transform checker, release gate, or application authorization.
Rust dependencies must already be cached; this command never uses the network.
"""
from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
import tomllib

ROOT = Path(__file__).resolve().parents[3]
BENCH = Path("docs/benchmarks/withdrawal-queue")
CRATES = ("synthesis", "core", "codec", "crypto", "value")
TEMPLATE = Path("crates/zeno-fcis-cli/templates/withdrawal-queue")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def copy_file(path, target, manifest):
    origin = ROOT / path
    if origin.is_symlink():
        raise RuntimeError(f"refuse symlink in research source closure: {path}")
    expected = digest(origin)
    destination = target / path
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(origin, destination)
    if digest(destination) != expected or digest(origin) != expected:
        raise RuntimeError(f"source changed during freeze: {path}")
    manifest[str(path)] = expected


def source_files():
    files = {Path(x) for x in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "README.md")}
    for base in [*(Path("crates") / f"zeno-fcis-{x}" for x in CRATES), TEMPLATE, BENCH]:
        files.update(p.relative_to(ROOT) for p in (ROOT / base).rglob("*")
                     if p.is_file() and not {"target", "__pycache__", ".git"}.intersection(p.parts))
    return sorted(files)


def run(command, cwd, output, name):
    started = time.monotonic()
    with (output / f"{name}.stdout").open("w") as out, (output / f"{name}.stderr").open("w") as err:
        completed = subprocess.run(command, cwd=cwd, stdout=out, stderr=err,
                                   env={**os.environ, "CARGO_BUILD_JOBS": "1"}, timeout=300)
    receipt = {"command": command, "cwd": str(cwd), "exit": completed.returncode,
               "seconds": time.monotonic() - started}
    (output / f"{name}.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(f"{name}: exit {completed.returncode}", flush=True)
    if completed.returncode:
        raise RuntimeError(f"{name} failed; inspect {output / (name + '.stderr')}")
    return receipt


def verify_registry_pins(original, resolved):
    pins = {(p["name"], p["version"], p.get("source")): p.get("checksum")
            for p in tomllib.loads(original.read_text())["package"]}
    packages = tomllib.loads(resolved.read_text())["package"]
    for package in packages:
        if package.get("source"):
            key = (package["name"], package["version"], package["source"])
            if key not in pins or pins[key] != package.get("checksum"):
                raise RuntimeError(f"resolved dependency is not pinned by original Cargo.lock: {key}")
    return [p for p in packages if p.get("source")]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="new directory outside the source tree")
    parser.add_argument("--heavy-lock", type=Path, default=Path("/tmp/zenofcis-heavy-check.lock"))
    args = parser.parse_args()
    output = args.output.resolve()
    if output == ROOT or ROOT in output.parents:
        raise RuntimeError("output must be outside the source checkout")
    output.mkdir(parents=True, exist_ok=False)
    snapshot = output / "source"
    manifest = {}
    for path in source_files():
        copy_file(path, snapshot, manifest)
    (output / "source-manifest.json").write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n")
    # Keep exact crate sources. Narrow only workspace membership for the frozen
    # build, so unrelated project crates are not resolved or compiled.
    cargo = (snapshot / "Cargo.toml").read_text()
    members = "members = [" + ", ".join(json.dumps(f"crates/zeno-fcis-{x}") for x in CRATES) + "]"
    cargo, count = re.subn(r"members\s*=\s*\[.*?\]", lambda _: members, cargo, count=1, flags=re.S)
    if count != 1:
        raise RuntimeError("cannot narrow frozen workspace membership")
    (snapshot / "Cargo.toml").write_text(cargo)
    benchmark = snapshot / BENCH
    registry = verify_registry_pins(snapshot / "Cargo.lock", benchmark / "Cargo.lock")
    cargo_exe = shutil.which("cargo")
    if not cargo_exe:
        raise RuntimeError("cargo is unavailable")
    target = output / "target"
    commands = []
    args.heavy_lock.parent.mkdir(parents=True, exist_ok=True)
    print(f"waiting for heavy-check lock {args.heavy_lock}", flush=True)
    with args.heavy_lock.open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        commands.append(run([cargo_exe, "+1.97.1", "fmt", "--manifest-path", str(benchmark / "Cargo.toml"),
                             "--", "--check", "--config", "skip_children=true"], snapshot, output, "format"))
        for profile in ("debug", "release"):
            command = [cargo_exe, "+1.97.1", "build", "--offline", "--locked", "--manifest-path",
                       str(benchmark / "Cargo.toml"), "--target-dir", str(target), "-j", "1"]
            if profile == "release":
                command.append("--release")
            commands.append(run(command, snapshot, output, f"build-{profile}"))
            registry = verify_registry_pins(snapshot / "Cargo.lock", benchmark / "Cargo.lock")
            destination = output / profile
            command = [str(target / profile / "zeno-withdrawal-research"), str(destination)]
            if profile == "release":
                command.append("--all-application-inputs")
            commands.append(run(command, snapshot, output, f"native-{profile}"))
        commands.append(run([cargo_exe, "+1.97.1", "clippy", "--offline", "--locked", "--no-deps",
                             "--manifest-path", str(benchmark / "Cargo.toml"), "--target-dir", str(target),
                             "--", "-D", "warnings"], snapshot, output, "clippy"))
    changed = [path for path, sha in manifest.items() if not (ROOT / path).is_file() or digest(ROOT / path) != sha]
    if changed:
        raise RuntimeError(f"source changed during run: {changed}")
    frozen_changed = [path for path, sha in manifest.items()
                      if path != "Cargo.toml" and digest(snapshot / path) != sha]
    if frozen_changed:
        raise RuntimeError(f"frozen input changed during run: {frozen_changed}")
    files = {str(p.relative_to(output)): digest(p)
             for profile in ("debug", "release") for p in sorted((output / profile).iterdir())}
    report = {"schema": "zeno-fcis/withdrawal-native-evidence/1", "source_checkout": str(ROOT),
              "head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
              "source_is_dirty": True, "source_manifest_sha256": digest(output / "source-manifest.json"),
              "frozen_workspace_manifest_sha256": digest(snapshot / "Cargo.toml"),
              "registry_dependencies": registry, "commands": commands, "artifacts": files,
              "results": {p: json.loads((output / p / "results.json").read_text()) for p in ("debug", "release")},
              "scope": "offline native scalar benchmark; no application authority, proof or release qualification",
              "success": True}
    (output / "evidence.json").write_text(json.dumps(report, sort_keys=True, indent=2) + "\n")
    print(f"evidence: {output / 'evidence.json'}", flush=True)


if __name__ == "__main__":
    main()
