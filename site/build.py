#!/usr/bin/env python3
"""Build and test the demo site against this checkout.

In order:

1. generate: `cargo +1.97.1 fetch --locked` in the workspace, as the gate
   does, so that every later cargo command can run offline against the
   reviewed lock (the site's lock names the same external packages); then
   `cargo +1.97.1 build -p zeno-fcis-cli`, then `zeno-fcis new
   site/apps/<template> --template <template>` for every template, so that
   the page runs each template exactly as the CLI ships it. An application
   whose files are unchanged is kept as it is.
2. lock: site/Cargo.lock. Every external package must carry the identity the
   workspace lock reviewed, and every local package must be this checkout's.
   `--relock` regenerates the lock from the workspace lock first.
3. check: in site/, `cargo fmt --check`, clippy with `-D warnings` for the
   host and for wasm32-unknown-unknown, and the host tests.
4. module: each module, a release build for wasm32-unknown-unknown, twice,
   with the application and the demo crate rebuilt from scratch in between
   (the library crates come from the cache), and identical bytes required.
   The result is copied to site/public/<template>.wasm, and the sizes are
   printed.
5. replay: compiled ABI bounds/private-memory checks, then
   `node site/tests/replay.mjs <template>...`.
6. scripts: `node --check` on every script under site/public/ and
   site/tests/: the replay loads only the module loader, so this is what
   catches a syntax error in the page's own script before a browser does.
7. browser: `site/tests/deploy_check.py`, the exact artifact served from a
   subpath, in headless Chrome. `--no-browser` skips it on a machine without
   Chrome.

`--only TEMPLATE` (repeatable) limits the module, replay, and browser steps
to some templates; `--stage STAGE` (repeatable) runs only the named steps.
Together they let a long build be split into shorter runs.

Requirements: Rust 1.97.1 with the wasm32-unknown-unknown target, Node 22,
python3, and Chrome for step 7. CARGO_TARGET_DIR is honoured; the wasm32
artifacts go under its wasm32-unknown-unknown/ subdirectory. Each module's
paths are remapped, so it names /zeno-fcis, /target, and /registry instead of
local directories.
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

from wasm import private_module

ROOT = Path(__file__).resolve().parents[1]
SITE = ROOT / "site"
PUBLIC = SITE / "public"
COMMON_CRATE = "zeno-fcis-site-common"
TEMPLATES = ("account-lockout", "order-fulfillment", "inventory-reservation", "compliance-gateway",
             "withdrawal-queue", "agent-treasury-guard")
STAGES = ("generate", "lock", "check", "module", "replay", "scripts", "browser")
TARGET = "wasm32-unknown-unknown"

sys.path.insert(0, str(ROOT / "tools"))
import check_generated_application as gate  # noqa: E402


def app(template: str) -> Path:
    return SITE / "apps" / template


def demo_crate(template: str) -> str:
    return f"zeno-fcis-site-{template}"


def module(template: str) -> Path:
    return PUBLIC / f"{template}.wasm"


def run(command: list[str], cwd: Path, environment: dict[str, str], *, capture: bool = False) -> str:
    print("+", " ".join(command), flush=True)
    result = subprocess.run(command, cwd=cwd, check=True, text=True, env=environment,
                            stdout=subprocess.PIPE if capture else None)
    return result.stdout if capture else ""


def target_dir(environment: dict[str, str]) -> Path:
    return (ROOT / Path(environment.get("CARGO_TARGET_DIR", ROOT / "target"))).resolve()


def build_cli(environment: dict[str, str]) -> Path:
    run(["cargo", "+1.97.1", "fetch", "--locked"], ROOT, environment)
    run(["cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"], ROOT, environment)
    return target_dir(environment) / "debug" / "zeno-fcis"


def generate_application(executable: Path, template: str, environment: dict[str, str]) -> None:
    """Writes the template as the CLI ships it. Unchanged files keep their timestamps."""
    destination = app(template)
    with tempfile.TemporaryDirectory(prefix="zeno-fcis-site-") as directory:
        fresh = Path(directory) / template
        run([str(executable), "new", str(fresh), "--template", template], ROOT, environment)
        source = ROOT / "crates" / "zeno-fcis-cli" / "templates" / template
        expected = gate.generated_file_manifest(source)
        for entry in expected:
            if entry["path"] == "Cargo.toml.in":
                entry["path"] = "Cargo.toml"
        if sorted(expected, key=lambda entry: entry["path"]) != gate.generated_file_manifest(fresh):
            raise RuntimeError(f"{template}: generated files differ from this checkout's template source")
        if destination.is_dir() and gate.generated_file_manifest(destination) == gate.generated_file_manifest(fresh):
            print(f"{destination.relative_to(ROOT)}: unchanged")
            return
        if destination.exists():
            shutil.rmtree(destination)
        shutil.copytree(fresh, destination)
    print(f"{destination.relative_to(ROOT)}: written")


def local_manifests() -> dict[tuple[str, str], Path]:
    """Every local package the site may resolve: this checkout's crates, the applications, the demos."""
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    allowed = {(tomllib.loads(manifest.read_text())["package"]["name"], version): manifest
               for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml"))}
    site_version = tomllib.loads((SITE / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    allowed[(COMMON_CRATE, site_version)] = SITE / "common" / "Cargo.toml"
    for template in TEMPLATES:
        application = tomllib.loads((app(template) / "Cargo.toml").read_text())["package"]
        allowed[(application["name"], application["version"])] = app(template) / "Cargo.toml"
        allowed[(demo_crate(template), site_version)] = SITE / "demos" / template / "Cargo.toml"
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
        ["python3", str(SITE / "tests" / "test_wasm.py")],
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


def build_module(template: str, environment: dict[str, str]) -> bytes:
    crate = demo_crate(template)
    run(["cargo", "+1.97.1", "build", "--release", "--locked", "--offline", "--target", TARGET,
         "-p", crate], SITE, module_environment(environment))
    raw = (target_dir(environment) / TARGET / "release" / f"{crate.replace('-', '_')}.wasm").read_bytes()
    return private_module(raw)


def reproducible_module(template: str, environment: dict[str, str]) -> bytes:
    first = build_module(template, environment)
    run(["cargo", "+1.97.1", "clean", "--offline", "--release", "--target", TARGET,
         "-p", demo_crate(template), "-p", template], SITE, environment)
    second = build_module(template, environment)
    if first != second:
        raise RuntimeError(f"{template}: the module differs between two builds")
    return first


def report_sizes() -> None:
    modules = sorted(PUBLIC.glob("*.wasm"))
    total = 0
    for path in modules:
        size = path.stat().st_size
        total += size
        print(f"{path.relative_to(ROOT)}: {size:,} bytes")
    print(f"site/public: {len(modules)} module(s), {total:,} bytes in all; a panel downloads its module alone")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--relock", action="store_true",
                        help="regenerate site/Cargo.lock from the workspace lock before checking it")
    parser.add_argument("--only", action="append", choices=TEMPLATES, metavar="TEMPLATE",
                        help="limit the module, replay, and browser steps to this template (repeatable)")
    parser.add_argument("--stage", action="append", choices=STAGES, metavar="STAGE",
                        help="run only this step (repeatable): " + ", ".join(STAGES))
    parser.add_argument("--no-browser", action="store_true",
                        help="skip the headless-Chrome check of the served artifact")
    parser.add_argument("--chrome", help="the Chrome binary for that check (default: the first on PATH)")
    args = parser.parse_args()
    stages = set(args.stage or STAGES)
    templates = tuple(args.only or TEMPLATES)
    environment = dict(os.environ)
    environment.setdefault("CARGO_INCREMENTAL", "0")
    # The CLI and site are separate Cargo workspaces. Give both one absolute
    # target directory, including when no caller supplied CARGO_TARGET_DIR.
    environment["CARGO_TARGET_DIR"] = str(target_dir(environment))
    if "generate" in stages:
        executable = build_cli(environment)
        for template in TEMPLATES:
            generate_application(executable, template, environment)
    if "lock" in stages:
        check_lock(environment, args.relock)
    if "check" in stages:
        check_site(environment)
    if "module" in stages:
        for template in templates:
            built = reproducible_module(template, environment)
            module(template).write_bytes(built)
            print(f"{module(template).relative_to(ROOT)}: sha256 {hashlib.sha256(built).hexdigest()}, "
                  "identical across two builds")
        report_sizes()
    if "replay" in stages:
        run(["node", str(SITE / "tests" / "abi.mjs"), *templates], ROOT, environment)
        run(["node", str(SITE / "tests" / "replay.mjs"), *templates], ROOT, environment)
    if "scripts" in stages:
        scripts = [*PUBLIC.glob("**/*.js"), *(SITE / "tests").glob("**/*.js"), *(SITE / "tests").glob("*.mjs")]
        for script in sorted(scripts):
            run(["node", "--check", str(script)], ROOT, environment)
    if "browser" in stages:
        if args.no_browser:
            print("site: the browser check was skipped")
        else:
            chrome = ["--chrome", args.chrome] if args.chrome else []
            run(["python3", str(SITE / "tests" / "deploy_check.py"), *chrome, *templates], ROOT, environment)
    print(f"site: {', '.join(stage for stage in STAGES if stage in stages)}: done")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"site: FAIL: {error}", file=sys.stderr)
        sys.exit(1)
