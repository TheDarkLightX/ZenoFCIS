#!/usr/bin/env python3
"""Build and test the demo site against this checkout.

In order:

1. `cargo +1.97.1 build -p zeno-fcis-cli`, then `zeno-fcis new
   site/apps/account-lockout --template account-lockout`, so that the page
   runs the template exactly as the CLI ships it. An application whose files
   are unchanged is kept as it is.
2. site/Cargo.lock: every external package must carry the identity the
   workspace lock reviewed, and every local package must be this checkout's.
   `--relock` regenerates the lock from the workspace lock first.
3. In site/: `cargo fmt --check`, clippy with `-D warnings` for the host and
   for wasm32-unknown-unknown, and the host tests.
4. The module: a release build for wasm32-unknown-unknown, twice, with the
   application and the demo crate rebuilt from scratch in between (the library
   crates come from the cache), and identical bytes required. The result is
   copied to site/public/account-lockout.wasm, with the application's README
   beside it for the page to link.
5. `node site/tests/replay.mjs`.

Requirements: Rust 1.97.1 with the wasm32-unknown-unknown target, Node 22, and
python3. CARGO_TARGET_DIR is honoured; the wasm32 artifacts go under its
wasm32-unknown-unknown/ subdirectory. The module's paths are remapped, so it
names /zeno-fcis, /target, and /registry instead of local directories.
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
SITE = ROOT / "site"
TEMPLATE = "account-lockout"
APP = SITE / "apps" / TEMPLATE
DEMO = SITE / "demos" / TEMPLATE
DEMO_CRATE = "zeno-fcis-site-account-lockout"
MODULE = SITE / "public" / f"{TEMPLATE}.wasm"
# The template's README, as shipped, for the page to link.
README = SITE / "public" / f"{TEMPLATE}.README.md"
TARGET = "wasm32-unknown-unknown"

sys.path.insert(0, str(ROOT / "tools"))
import check_generated_application as gate  # noqa: E402


def run(command: list[str], cwd: Path, environment: dict[str, str], *, capture: bool = False) -> str:
    print("+", " ".join(command), flush=True)
    result = subprocess.run(command, cwd=cwd, check=True, text=True, env=environment,
                            stdout=subprocess.PIPE if capture else None)
    return result.stdout if capture else ""


def target_dir(environment: dict[str, str]) -> Path:
    return (ROOT / Path(environment.get("CARGO_TARGET_DIR", ROOT / "target"))).resolve()


def build_cli(environment: dict[str, str]) -> Path:
    run(["cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"], ROOT, environment)
    return target_dir(environment) / "debug" / "zeno-fcis"


def generate_application(executable: Path, environment: dict[str, str]) -> None:
    """Writes the template as the CLI ships it. Unchanged files keep their timestamps."""
    with tempfile.TemporaryDirectory(prefix="zeno-fcis-site-") as directory:
        fresh = Path(directory) / TEMPLATE
        run([str(executable), "new", str(fresh), "--template", TEMPLATE], ROOT, environment)
        if APP.is_dir() and gate.generated_file_manifest(APP) == gate.generated_file_manifest(fresh):
            print(f"{APP.relative_to(ROOT)}: unchanged")
            return
        if APP.exists():
            shutil.rmtree(APP)
        shutil.copytree(fresh, APP)
    print(f"{APP.relative_to(ROOT)}: written")


def local_manifests() -> dict[tuple[str, str], Path]:
    """Every local package the site may resolve: this checkout's crates, the application, the demo."""
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    allowed = {(tomllib.loads(manifest.read_text())["package"]["name"], version): manifest
               for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml"))}
    site_version = tomllib.loads((SITE / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    application = tomllib.loads((APP / "Cargo.toml").read_text())["package"]
    allowed[(application["name"], application["version"])] = APP / "Cargo.toml"
    allowed[(DEMO_CRATE, site_version)] = DEMO / "Cargo.toml"
    return allowed


def check_lock(environment: dict[str, str], relock: bool) -> None:
    allowed = local_manifests()
    if relock:
        graph = gate.resolve_reviewed_graph(SITE, allowed, environment)
        print(f"site/Cargo.lock: regenerated, {len(graph['packages'])} packages")
        return
    metadata = json.loads(run(["cargo", "+1.97.1", "metadata", "--locked", "--offline",
                               "--format-version", "1"], SITE, environment, capture=True))
    reference = tomllib.loads((ROOT / "Cargo.lock").read_text())
    locked = tomllib.loads((SITE / "Cargo.lock").read_text())
    gate.validate_resolved_graph(reference, locked, metadata, allowed)
    print(f"site/Cargo.lock: {len(locked['package'])} packages, every external identity reviewed")


def check_site(environment: dict[str, str]) -> None:
    for command in (
        ["cargo", "+1.97.1", "fmt", "--all", "--", "--check"],
        ["cargo", "+1.97.1", "clippy", "--all-targets", "--locked", "--offline", "--", "-D", "warnings"],
        ["cargo", "+1.97.1", "clippy", "--lib", "--locked", "--offline", "--target", TARGET,
         "--", "-D", "warnings"],
        ["cargo", "+1.97.1", "test", "--locked", "--offline"],
    ):
        run(command, SITE, environment)


def module_environment(environment: dict[str, str]) -> dict[str, str]:
    """Remaps the checkout, the target directory, and the registry, so the module names no local directory.

    With `--target`, Cargo applies these flags to the module only, never to the
    build scripts it compiles for the host.
    """
    cargo_home = Path(environment.get("CARGO_HOME", Path.home() / ".cargo"))
    remaps = [f"--remap-path-prefix={ROOT}=/zeno-fcis",
              f"--remap-path-prefix={target_dir(environment)}=/target",
              f"--remap-path-prefix={cargo_home / 'registry' / 'src'}=/registry"]
    if any("\x1f" in flag or "\0" in flag for flag in remaps):
        raise RuntimeError("a path cannot be encoded as one compiler argument")
    return {**environment, "CARGO_ENCODED_RUSTFLAGS": "\x1f".join(remaps)}


def build_module(environment: dict[str, str]) -> bytes:
    run(["cargo", "+1.97.1", "build", "--release", "--locked", "--offline", "--target", TARGET,
         "-p", DEMO_CRATE], SITE, module_environment(environment))
    return (target_dir(environment) / TARGET / "release" / f"{DEMO_CRATE.replace('-', '_')}.wasm").read_bytes()


def reproducible_module(environment: dict[str, str]) -> bytes:
    first = build_module(environment)
    run(["cargo", "+1.97.1", "clean", "--offline", "--release", "--target", TARGET,
         "-p", DEMO_CRATE, "-p", TEMPLATE], SITE, environment)
    second = build_module(environment)
    if first != second:
        raise RuntimeError("the module differs between two builds")
    return first


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--relock", action="store_true",
                        help="regenerate site/Cargo.lock from the workspace lock before checking it")
    args = parser.parse_args()
    environment = dict(os.environ)
    environment.setdefault("CARGO_INCREMENTAL", "0")
    generate_application(build_cli(environment), environment)
    check_lock(environment, args.relock)
    check_site(environment)
    module = reproducible_module(environment)
    MODULE.write_bytes(module)
    shutil.copyfile(APP / "README.md", README)
    print(f"{MODULE.relative_to(ROOT)}: {len(module)} bytes, sha256 {hashlib.sha256(module).hexdigest()}, "
          "identical across two builds")
    run(["node", str(SITE / "tests" / "replay.mjs")], ROOT, environment)
    print("site: built and tested")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"site: FAIL: {error}", file=sys.stderr)
        sys.exit(1)
