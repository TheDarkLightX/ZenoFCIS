#!/usr/bin/env python3
"""Replay the specification-parser proof kernels on their actual runtime bodies.

The receipt records scoped Verus evidence, native tests and semantic negative
controls. It does not certify the complete AST lowerer or parser completeness.
Requires the repository's pinned verifier and Rust toolchains; no Python package
or new runtime Cargo dependency is needed.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
import tomllib

from check_verus import prepare_tools, proof_succeeded, semantic_refusal

ROOT = Path(__file__).resolve().parents[1]
PROFILE = ROOT / "verification/verus/spec_parser_profile.json"
PIN = ROOT / "verification/verus/toolchain.json"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def execute(command, cwd, environment, log, timeout=90):
    started = time.monotonic()
    proc = subprocess.run(command, cwd=cwd, env=environment, text=True,
                          capture_output=True, timeout=timeout, check=False)
    write_json(log, {"argv": command, "cwd": str(cwd), "exit_code": proc.returncode,
                     "seconds": time.monotonic() - started,
                     "stdout": proc.stdout, "stderr": proc.stderr})
    return proc


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def accepted_kernel(proc, report, pin, kernel):
    """Refuse partial runs, stale tools, and missing proof obligations."""
    return (proc.returncode == 0 and proof_succeeded(report, pin)
            and report["verification-results"]["verified"] == kernel["expected_verified"]
            and set(kernel["target_functions"]).issubset(report["func-details"]))


def accepted_native(proc, expected):
    summaries = re.findall(
        r"(?m)^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;",
        proc.stdout,
    )
    return proc.returncode == 0 and summaries == [(str(expected), "0", "0", "0", "0")]


def snapshot_tree(directory):
    """Bind a complete regular-file inventory, including newly added files."""
    directory = Path(directory)
    require(directory.is_dir() and not directory.is_symlink(),
            f"source directory is missing or a symlink: {directory}")
    result = {}
    for parent, directories, files in os.walk(directory):
        for name in directories:
            require(not (Path(parent) / name).is_symlink(),
                    f"source directory symlink is not supported: {Path(parent) / name}")
        for name in files:
            path = Path(parent) / name
            require(path.is_file() and not path.is_symlink(),
                    f"source must be a regular file: {path}")
            result[path.relative_to(directory).as_posix()] = sha(path)
    return dict(sorted(result.items()))


def binding_snapshot(profile, root=ROOT):
    paths = set(profile["bindings"])
    for kernel in profile["kernels"]:
        paths.update(kernel["files"])
    for directory in profile.get("binding_directories", []):
        relative = Path(directory)
        require(relative.parts and not relative.is_absolute() and ".." not in relative.parts,
                f"binding directory must stay in the repository: {directory}")
        paths.update(str(relative / path) for path in snapshot_tree(root / relative))
    result = {}
    for path in sorted(paths):
        relative = Path(path)
        require(not relative.is_absolute() and ".." not in relative.parts,
                f"binding path must stay in the repository: {path}")
        source = root / relative
        require(source.is_file() and not source.is_symlink(), f"missing regular binding: {path}")
        result[relative.as_posix()] = sha(source)
    return result


def dependency_closure(metadata, package_id):
    """Normal/build dependencies only; metadata is filtered for the build host."""
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    pending, selected = [package_id], set()
    while pending:
        current = pending.pop()
        if current in selected:
            continue
        require(current in nodes, "dependency absent from Cargo's resolved graph")
        selected.add(current)
        for dependency in nodes[current]["deps"]:
            if any(kind["kind"] in (None, "build") for kind in dependency["dep_kinds"]):
                pending.append(dependency["pkg"])
    return selected


def cargo_configuration_snapshot(root, environment):
    """Bind configuration presence and hashes, without reading credential files."""
    directories = [root, *root.parents]
    paths = {parent / ".cargo" / name for parent in directories
             for name in ("config", "config.toml")}
    cargo_home = Path(environment.get("CARGO_HOME", str(Path.home() / ".cargo"))).resolve()
    paths.update(cargo_home / name for name in ("config", "config.toml"))
    return {str(path): sha(path) if path.is_file() else None for path in sorted(paths)}


def build_native_spec(pin, environment, out, install):
    """Build real opaque leaf types from these sources using Verus's Rust ABI.

    Nothing is borrowed from a prior Cargo target directory. Cargo metadata
    supplies a dependency superset before compilation; the build's artifact
    messages identify the exact packages whose sources and output files enter
    this receipt. No external functions or semantic properties are assumed.
    """
    prefix = out / "native-spec"
    prefix.mkdir(parents=True, exist_ok=True)
    native_environment = {
        key: value for key, value in environment.items()
        if not key.startswith(("CARGO_BUILD_", "CARGO_TARGET_", "CARGO_PROFILE_"))
        and key not in ("RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
                        "RUSTC_BOOTSTRAP", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS",
                        "CARGO_TARGET_DIR", "CARGO_INCREMENTAL")
    }
    native_environment.update(RUSTC_WRAPPER="", RUSTC_WORKSPACE_WRAPPER="",
                              RUSTFLAGS="", CARGO_ENCODED_RUSTFLAGS="",
                              CARGO_INCREMENTAL="0", RUSTUP_TOOLCHAIN=pin["rust_toolchain"])
    tools = {}
    for tool in ("rustc", "cargo"):
        resolved = execute(["rustup", "which", "--toolchain", pin["rust_toolchain"], tool],
                           ROOT, native_environment, prefix / (tool + "-path.json"))
        require(resolved.returncode == 0, f"cannot resolve pinned {tool}")
        path = Path(resolved.stdout.strip())
        require(path.is_absolute() and path.is_file(), f"missing pinned {tool} binary")
        tools[tool] = {"path": str(path), "sha256": sha(path)}
    native_environment["RUSTC"] = tools["rustc"]["path"]
    version = execute([tools["rustc"]["path"], "--version", "--verbose"], ROOT,
                      native_environment, prefix / "rustc-version.json")
    require(version.returncode == 0, "native proof Rust version failed")
    release = re.search(r"(?m)^release: (\S+)$", version.stdout)
    host = re.search(r"(?m)^host: (\S+)$", version.stdout)
    require(release and host and f"{release[1]}-{host[1]}" == pin["rust_toolchain"],
            "native dependency compiler does not match proof Rust pin")
    tools["rustc"]["version"] = version.stdout
    configuration = cargo_configuration_snapshot(ROOT, native_environment)
    offline = [] if install else ["--offline"]
    metadata_command = [tools["cargo"]["path"], "metadata", "--format-version", "1",
                        "--locked", "--filter-platform", host[1], *offline]
    metadata_result = execute(metadata_command, ROOT, native_environment,
                              prefix / "metadata.json", timeout=300)
    require(metadata_result.returncode == 0, "native dependency metadata failed")
    metadata = json.loads(metadata_result.stdout)
    packages = {package["id"]: package for package in metadata["packages"]}
    spec_manifest = ROOT / "crates/zeno-fcis-spec/Cargo.toml"
    roots = [identifier for identifier, package in packages.items()
             if Path(package["manifest_path"]).resolve() == spec_manifest.resolve()]
    require(len(roots) == 1, "Cargo did not resolve the actual specification crate")
    candidate_ids = dependency_closure(metadata, roots[0])
    source_before = {identifier: snapshot_tree(Path(packages[identifier]["manifest_path"]).parent)
                     for identifier in sorted(candidate_ids)}
    target = prefix / "target"
    require(not target.is_symlink(), "native proof target must not be a symlink")
    if target.exists():
        shutil.rmtree(target)
    command = [tools["cargo"]["path"], "build", "--locked", *offline,
               "-p", "zeno-fcis-spec", "--lib", "--target", host[1],
               "--target-dir", str(target), "--message-format=json-render-diagnostics"]
    build = execute(command, ROOT, native_environment, prefix / "build.json", timeout=300)
    require(build.returncode == 0, "native proof dependency build failed")
    messages = [json.loads(line) for line in build.stdout.splitlines() if line.strip()]
    finished = [message for message in messages if message.get("reason") == "build-finished"]
    require(len(finished) == 1 and finished[0].get("success") is True,
            "Cargo did not report one complete native build")
    artifacts = [message for message in messages if message.get("reason") == "compiler-artifact"]
    require(artifacts and all(message.get("fresh") is False for message in artifacts),
            "native proof build unexpectedly reused prior artifacts")
    actual_ids = {message["package_id"] for message in artifacts}
    require(roots[0] in actual_ids and actual_ids <= candidate_ids,
            "native artifact set does not match the pre-bound dependency graph")
    for identifier in sorted(actual_ids):
        source = Path(packages[identifier]["manifest_path"]).parent
        require(source_before[identifier] == snapshot_tree(source),
                f"native dependency source changed during compilation: {identifier}")
    spec_artifacts = [Path(filename) for message in artifacts
                      if message["package_id"] == roots[0]
                      and message["target"]["name"] == "zeno_fcis_spec"
                      for filename in message["filenames"] if filename.endswith(".rlib")]
    require(len(spec_artifacts) == 1, "expected exactly one native specification rlib")
    library = spec_artifacts[0]
    require(library.is_relative_to(target) and library.is_file(),
            "native specification artifact is outside the fresh target")
    dependency_directory = target / host[1] / "debug/deps"
    require(dependency_directory.is_dir(), "native dependency search path is missing")
    require(configuration == cargo_configuration_snapshot(ROOT, native_environment),
            "Cargo configuration changed during native compilation")
    record = {
        "toolchain": pin["rust_toolchain"], "tools": tools, "target": host[1],
        "build_argv": command, "metadata_argv": metadata_command,
        "cargo_configuration_sha256": configuration,
        "environment_overrides": {key: native_environment[key] for key in (
            "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS", "CARGO_INCREMENTAL", "RUSTUP_TOOLCHAIN")},
        "packages": {identifier: {
            "name": packages[identifier]["name"], "version": packages[identifier]["version"],
            "source": packages[identifier]["source"],
            "manifest_path": packages[identifier]["manifest_path"],
            "source_sha256": source_before[identifier],
            "features": sorted({feature for message in artifacts
                                if message["package_id"] == identifier for feature in message["features"]}),
        } for identifier in sorted(actual_ids)},
        "target_directory": str(target), "artifact_sha256": snapshot_tree(target),
        "spec_rlib": str(library), "dependency_directory": str(dependency_directory),
        "scope": "Real opaque leaf types only; no external operations or semantic axioms.",
    }
    return record, ["--extern", "zeno_fcis_spec=" + str(library),
                    "-L", "dependency=" + str(dependency_directory)]


def native_spec_unchanged(record, environment):
    if record is None:
        return True
    if record["cargo_configuration_sha256"] != cargo_configuration_snapshot(ROOT, environment):
        return False
    for package in record["packages"].values():
        if package["source_sha256"] != snapshot_tree(Path(package["manifest_path"]).parent):
            return False
    return (all(tool["sha256"] == sha(Path(tool["path"])) for tool in record["tools"].values())
            and record["artifact_sha256"] == snapshot_tree(Path(record["target_directory"])))


def mutated_source(original, mutant):
    """Mutate a uniquely identified function, including indented methods."""
    matches = list(re.finditer(r"\bfn\s+" + re.escape(mutant["function"]) + r"\s*\(", original))
    require(len(matches) == 1, "mutation function must occur exactly once")
    start = matches[0].start()
    # The mutation must stay after this signature and before the next function.
    following = re.search(
        r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?\s+)?"
        r"(?:(?:const|async|unsafe|open|closed|spec|proof|exec)\s+)*fn\s+",
        original[matches[0].end():],
    )
    end = matches[0].end() + following.start() if following else len(original)
    body = original[start:end]
    require(body.count(mutant["old"]) == 1, "mutation anchor is not unique in its function")
    return original[:start] + body.replace(mutant["old"], mutant["new"]) + original[end:]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=ROOT / "target/spec-parser-proof")
    parser.add_argument("--install", action="store_true")
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    # A failed replay must never leave a prior success receipt at this path.
    (out / "receipt.json").unlink(missing_ok=True)
    pin_bytes = PIN.read_bytes()
    profile_bytes = PROFILE.read_bytes()
    rust_path = ROOT / "rust-toolchain.toml"
    rust_bytes = rust_path.read_bytes()
    pin = json.loads(pin_bytes)
    profile = json.loads(profile_bytes)
    cache = Path(os.environ.get("ZENO_FCIS_VERUS_CACHE", str(Path.home() / ".cache/zeno-fcis/verus")))
    verifier = prepare_tools(cache, pin, args.install)
    require(PIN.read_bytes() == pin_bytes and PROFILE.read_bytes() == profile_bytes
            and rust_path.read_bytes() == rust_bytes,
            "proof configuration changed during tool preparation")
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(("VERUS_", "VARGO_"))
           and k not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")}
    env.update(RUSTUP_TOOLCHAIN=pin["rust_toolchain"], VERUS_Z3_PATH=str(verifier / "z3"))
    rust = tomllib.loads(rust_bytes.decode())["toolchain"]["channel"]
    bound = binding_snapshot(profile)
    require(PIN.read_bytes() == pin_bytes and PROFILE.read_bytes() == profile_bytes
            and rust_path.read_bytes() == rust_bytes,
            "proof configuration changed before source binding")
    results = []
    grammar = execute(["python3", "tools/generate_spec_grammar.py", "--check"],
                      ROOT, env, out / "grammar.json")
    require(grammar.returncode == 0, "generated grammar is stale")
    native_dependency, dependency_arguments = None, []
    if any(kernel.get("native_spec_dependency") is True for kernel in profile["kernels"]):
        native_dependency, dependency_arguments = build_native_spec(pin, env, out, args.install)
        require(bound == binding_snapshot(profile), "bound source changed during native build")
    for kernel in profile["kernels"]:
        name = kernel["name"]
        prefix = out / name
        command = [str(verifier / "verus"), "--crate-type=lib", "--edition=2024",
                   "--no-cheating", "--no-external-by-default", "--output-json",
                   "--triggers-mode", "silent", kernel["harness"]]
        arguments = dependency_arguments if kernel.get("native_spec_dependency") is True else []
        command += arguments
        positive = execute(command, ROOT, env, prefix / "positive.json")
        report = json.loads(positive.stdout)
        require(accepted_kernel(positive, report, pin, kernel),
                f"{name}: pinned proof or obligation inventory failed")
        binary = prefix / "native-tests"
        compiler = ([native_dependency["tools"]["rustc"]["path"]] if arguments
                    else ["rustc", "+" + rust])
        compile_test = execute([*compiler, "--edition=2024", "--test",
                                "--check-cfg", "cfg(verus_keep_ghost)",
                                "--check-cfg", "cfg(test)", kernel["harness"],
                                "-o", str(binary), *arguments], ROOT, env, prefix / "native-compile.json")
        require(compile_test.returncode == 0, f"{name}: native compilation failed")
        native = execute([str(binary)], ROOT, env, prefix / "native.json")
        require(accepted_native(native, kernel["expected_native_tests"]),
                f"{name}: native tests failed or inventory changed")
        controls = []
        for mutant in kernel["mutations"]:
            subject = Path(mutant["subject"])
            original = (ROOT / subject).read_text()
            changed = mutated_source(original, mutant)
            with tempfile.TemporaryDirectory(prefix="spec-parser-control-") as temporary:
                specimen = Path(temporary)
                for path in kernel["files"]:
                    dest = specimen / path
                    dest.parent.mkdir(parents=True, exist_ok=True)
                    if Path(path) == subject:
                        dest.write_text(changed)
                    else:
                        shutil.copyfile(ROOT / path, dest)
                outcome = execute(command, specimen, env, prefix / (mutant["name"] + ".json"))
                parsed = json.loads(outcome.stdout)
                require(semantic_refusal(outcome, parsed, pin, subject, specimen,
                                         target_functions=mutant["target_functions"]),
                        f'{name}/{mutant["name"]}: not an intended semantic proof failure')
                controls.append({"name": mutant["name"], "status": "semantic-refusal",
                                 "mutated_source_sha256": hashlib.sha256(changed.encode()).hexdigest(),
                                 "verification_results": parsed["verification-results"]})
        results.append({"kernel": name, "verification_results": report["verification-results"],
                        "verus": report["verus"], "native_tests": kernel["expected_native_tests"],
                        "native_rust_toolchain": pin["rust_toolchain"] if arguments else rust,
                        "negative_controls": controls})
        print(f'{name}: {kernel["expected_verified"]} verified; '
              f'{kernel["expected_native_tests"]} native tests; {len(controls)} semantic controls', flush=True)
    require(bound == binding_snapshot(profile),
            "bound source changed during this run")
    require(PIN.read_bytes() == pin_bytes and PROFILE.read_bytes() == profile_bytes
            and rust_path.read_bytes() == rust_bytes,
            "proof configuration changed during this run")
    require(native_spec_unchanged(native_dependency, env),
            "native dependency sources, configuration, tools, or artifacts changed during proofs")
    logs = {str(path.relative_to(out)): sha(path) for path in sorted(out.rglob("*.json"))
            if path != out / "receipt.json"}
    write_json(out / "receipt.json", {
        "schema": "zeno-fcis/spec-parser-proof/1", "status": "passed",
        "source_sha256": bound, "kernels": results, "log_sha256": logs,
        "native_spec_dependency": native_dependency,
        "scope": profile["scope"], "unproved": profile["unproved"],
    })
    print(out / "receipt.json")


if __name__ == "__main__":
    main()
