#!/usr/bin/env python3
"""Build and exercise the actual CLI-generated application as an isolated consumer.

Only the new temporary consumer lock is resolved. External dependency versions
must match the repository lock. No Lean installation or invocation is involved.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def run(command: list[str], cwd: Path, *, capture: bool = False,
        environment: dict[str, str] | None = None) -> str:
    result = subprocess.run(command, cwd=cwd, check=True, text=True,
                            stdout=subprocess.PIPE if capture else None, env=environment)
    return result.stdout if capture else ""


def validate_resolved_graph(reference: dict, locked: dict, metadata: dict,
                            local_manifests: dict[tuple[str, str], Path]) -> None:
    """Admit the reviewed external graph and only explicitly selected local sources."""
    def identity(package: dict) -> tuple:
        return tuple(package.get(field) for field in ("name", "version", "source", "checksum"))

    external = {identity(package) for package in reference["package"] if package.get("source")}
    for package in locked["package"]:
        if package.get("source"):
            if identity(package) not in external:
                raise RuntimeError(f"unreviewed external dependency: {identity(package)}")
        elif (package["name"], package["version"]) not in local_manifests:
            raise RuntimeError(f"unapproved local package: {package['name']} {package['version']}")
    admitted = {identity(package)[:3] for package in locked["package"]}
    for package in metadata["packages"]:
        if identity(package)[:3] not in admitted:
            raise RuntimeError(f"metadata disagrees with admitted lock: {package['name']}")
        if package.get("source") is None:
            expected = local_manifests[(package["name"], package["version"])]
            if Path(package["manifest_path"]).resolve() != expected.resolve():
                raise RuntimeError(f"unapproved local manifest: {package['manifest_path']}")


def resolve_reviewed_graph(directory: Path, local_manifests: dict[tuple[str, str], Path],
                           environment: dict[str, str]) -> dict:
    # Only this newly created workspace's lock may change. Cargo retains the
    # reviewed external versions while reconciling the different local roots.
    shutil.copyfile(ROOT / "Cargo.lock", directory / "Cargo.lock")
    metadata = json.loads(run(["cargo", "+1.97.1", "metadata", "--offline",
                               "--format-version", "1"], directory, capture=True,
                              environment=environment))
    reference = tomllib.loads((ROOT / "Cargo.lock").read_text())
    locked = tomllib.loads((directory / "Cargo.lock").read_text())
    validate_resolved_graph(reference, locked, metadata, local_manifests)
    packages = [{key: package[key] for key in ("name", "version", "source", "checksum") if key in package}
                for package in locked["package"]]
    return {"lock_sha256": hashlib.sha256((directory / "Cargo.lock").read_bytes()).hexdigest(),
            "packages": sorted(packages, key=lambda package: (package["name"], package["version"], package.get("source", "")))}


def generated_file_manifest(app: Path) -> list[dict]:
    return [{"path": path.relative_to(app).as_posix(), "bytes": path.stat().st_size,
             "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
            for path in sorted(app.rglob("*")) if path.is_file()]


def bind_generated_dependencies(manifest: Path, package_roots: dict[str, Path], version: str) -> None:
    packages = {name: (root, tomllib.loads((root / "Cargo.toml").read_text()))
                for name, root in package_roots.items()}
    emitted = tomllib.loads(manifest.read_text())
    if emitted.get("patch") or emitted.get("replace"):
        raise RuntimeError("generated manifest already contains a resolver override")

    def package_name(name: str, declaration: str | dict) -> str:
        return declaration.get("package", name) if isinstance(declaration, dict) else name

    def dependencies(document: dict) -> set[str]:
        names = set()
        for key, value in document.items():
            if key in ("dependencies", "dev-dependencies", "build-dependencies"):
                names.update({package_name(name, declaration) for name, declaration in value.items()} & packages.keys())
            elif isinstance(value, dict):
                names.update(dependencies(value))
        return names

    def check_pins(document: dict) -> None:
        for key, value in document.items():
            if key in ("dependencies", "dev-dependencies", "build-dependencies"):
                for name, declaration in value.items():
                    name = package_name(name, declaration)
                    if name.startswith("zeno-fcis"):
                        requirement = declaration if isinstance(declaration, str) else declaration.get("version")
                        if name not in packages or requirement != f"={version}":
                            raise RuntimeError(f"generated internal dependency pin is invalid: {name}")
                        if isinstance(declaration, dict) and any(field in declaration for field in ("path", "git", "registry")):
                            raise RuntimeError(f"generated dependency contains a source override: {name}")
            elif isinstance(value, dict):
                check_pins(value)

    check_pins(emitted)
    pending = dependencies(emitted)
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


def exercise_application(app: Path, directory: Path, package_roots: dict[str, Path],
                         version: str, environment: dict[str, str]) -> dict:
    generated = generated_file_manifest(app)
    manifest = app / "Cargo.toml"
    consumer = tomllib.loads(manifest.read_text())["package"]
    bind_generated_dependencies(manifest, package_roots, version)
    allowed = {(name, version): path / "Cargo.toml" for name, path in package_roots.items()}
    allowed[(consumer["name"], consumer["version"])] = manifest
    graph = resolve_reviewed_graph(app, allowed, environment)
    commands = [
        ["cargo", "+1.97.1", "fmt", "--all", "--", "--check"],
        ["cargo", "+1.97.1", "clippy", "--all-targets", "--locked", "--offline", "--", "-D", "warnings"],
        ["cargo", "+1.97.1", "test", "--locked", "--offline"],
    ]
    for command in commands:
        run(command, app, environment=environment)
    demonstration = run(["cargo", "+1.97.1", "run", "--locked", "--offline", "--",
                         str(directory / "counter.sqlite")], app, capture=True, environment=environment)
    print(demonstration, end="")
    return {"generated_files": generated, "resolved_graph": graph,
            "commands": commands + [["cargo", "+1.97.1", "run", "--locked", "--offline", "--", "<new-database>"]],
            "demonstration": demonstration.strip(), "status": "passed"}


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
    packages = {tomllib.loads(manifest.read_text())["package"]["name"]: manifest.parent
                for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml"))}
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    exercise_application(app, directory, packages, version, dict(os.environ))
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
