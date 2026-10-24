"""Link direct-source native harnesses to freshly built codec/value/collections oracles.

Callers serialize preparation with the fleet lock, before starting controls.
Workers only read private, hash-bound copies of that preparation. These are
native comparison dependencies, never formal proof evidence or a proof cache.
"""
from __future__ import annotations

import json
import os
from dataclasses import dataclass
from pathlib import Path
import shutil

import check_verus as verifier


def native_test_environment(root: Path, environment: dict) -> dict:
    """Bind directly compiled synthesis tests to their own source/specimen."""
    return {**environment, "CARGO_MANIFEST_DIR": str(root.resolve() / "crates/zeno-fcis-synthesis")}


@dataclass(frozen=True)
class NativeDependencies:
    root: Path
    sources: tuple[tuple[str, str], ...]
    artifacts: tuple[tuple[Path, str], ...]
    arguments: tuple[str, ...]
    receipt: Path
    receipt_sha256: str


def source_hashes(root: Path) -> dict[str, str]:
    files = [root / "Cargo.toml", root / "Cargo.lock", root / "rust-toolchain.toml", root / ".cargo/config.toml"]
    for package in ("zeno-fcis-codec", "zeno-fcis-value", "zeno-fcis-collections", "zeno-fcis-crypto"):
        base = root / "crates" / package
        files.append(base / "Cargo.toml")
        files.extend(sorted((base / "src").rglob("*.rs")))
    return {str(p.relative_to(root)): verifier.digest(p) for p in files}


def prepare_native_dependencies(root: Path, directory: Path, environment: dict) -> NativeDependencies:
    root = root.resolve()
    target = Path(environment.get("CARGO_TARGET_DIR", os.environ.get("CARGO_TARGET_DIR", str(root / "target"))))
    if not target.is_dir():
        raise ValueError("native dependencies require an existing shared target")
    retained = directory / "native-dependencies"
    retained.mkdir(parents=True, exist_ok=False)
    before = source_hashes(root)
    # Preserve bytes while forcing both actual local library compilations.
    for path in before:
        source = root / path
        if source.suffix == ".rs":
            source.touch()
    env = {k: v for k, v in environment.items() if not k.startswith(("VERUS_", "VARGO_"))
           and k not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")}
    env.update(CARGO_TARGET_DIR=str(target), CARGO_BUILD_JOBS="1", CARGO_INCREMENTAL="0",
               CARGO_NET_OFFLINE="true", RUST_TEST_THREADS="1")
    command = ["cargo", "+1.97.1", "build", "--locked", "--offline", "-vv", "-p",
               "zeno-fcis-codec", "-p", "zeno-fcis-collections", "-p", "zeno-fcis-crypto",
               "--message-format=json"]
    import subprocess
    disk = subprocess.run(["df", "-B1G", "/"], capture_output=True, text=True, check=True)
    (retained / "disk.log").write_text(disk.stdout)
    if os.statvfs("/").f_bavail * os.statvfs("/").f_frsize < 6 * 1024**3:
        raise ValueError("disk below 6 GiB; native dependency build stopped")
    env.update(CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0")
    run = verifier.run(command, root, env)
    (retained / "cargo.stdout").write_text(run.stdout)
    (retained / "cargo.stderr").write_text(run.stderr)
    verifier.require_success(run)
    libraries = {}
    # Copy every reported link dependency, including transitive/proc-macro
    # artifacts. Workers must not resolve against a mutable shared Cargo target.
    private = retained / "libraries"
    private.mkdir()
    copied = {}
    expected = {"zeno_fcis_codec": "zeno-fcis-codec", "zeno_fcis_value": "zeno-fcis-value",
                "zeno_fcis_collections": "zeno-fcis-collections", "zeno_fcis_crypto": "zeno-fcis-crypto"}
    for line in run.stdout.splitlines():
        row = json.loads(line)
        if row.get("reason") == "compiler-artifact":
            artifacts = [Path(name) for name in row.get("filenames", [])]
            reported_libraries = [path for path in artifacts if path.suffix == ".rlib"]
            for metadata in [path for path in artifacts if path.suffix == ".rmeta"]:
                # For a selected package Cargo reports its top-level .rlib and
                # hashed .rmeta. Rustc also needs the matching hashed .rlib for
                # transitive linking. Bind that sibling to the emitted bytes;
                # never search a shared target for a plausible stale artifact.
                library = metadata.with_suffix(".rlib")
                if library not in artifacts and reported_libraries:
                    if (library.is_symlink() or not library.is_file()
                            or not any(verifier.digest(library) == verifier.digest(path)
                                       for path in reported_libraries)):
                        raise ValueError("hashed native library differs from Cargo output")
                    artifacts.append(library)
            for artifact in artifacts:
                if artifact.suffix not in (".rlib", ".rmeta", ".so"):
                    continue
                destination = private / artifact.name
                expected_hash = verifier.digest(artifact)
                if destination in copied and copied[destination] != expected_hash:
                    raise ValueError("conflicting native dependency artifact: " + artifact.name)
                if destination not in copied:
                    shutil.copyfile(artifact, destination)
                    destination.chmod(0o444)
                if verifier.digest(destination) != expected_hash:
                    raise ValueError("native dependency artifact changed while copying")
                copied[destination] = expected_hash
        name = row.get("target", {}).get("name")
        if row.get("reason") != "compiler-artifact" or name not in expected:
            continue
        manifest = root / "crates" / expected[name] / "Cargo.toml"
        if row.get("fresh") is not False or Path(row["manifest_path"]).resolve() != manifest.resolve():
            raise ValueError("native oracle artifact lacks fresh current-source provenance: " + name)
        artifact = next(Path(p) for p in row["filenames"] if p.endswith(".rlib"))
        bound = private / artifact.name
        libraries[name] = {"path": str(bound), "sha256": copied[bound], "build_path": str(artifact),
                           "manifest_path": str(manifest), "fresh": False}
    if set(libraries) != set(expected):
        raise ValueError("actual codec, value, collections and crypto native oracle artifacts are missing")
    after = source_hashes(root)
    if before != after:
        raise ValueError("native comparison dependency source changed during compilation")
    (retained / "result.json").write_text(json.dumps({"status": "passed", "command": command,
        "cwd": str(root), "exit_code": run.returncode, "source_sha256": before,
        "source_stable": True, "libraries": libraries,
        "artifacts": {str(path): value for path, value in copied.items()},
        "formal_evidence": False}, indent=2) + "\n")
    args = ["-L", "dependency=" + str(private)]
    for name, artifact in sorted(libraries.items()):
        args.extend(("--extern", name + "=" + artifact["path"]))
    receipt = retained / "result.json"
    return NativeDependencies(root, tuple(before.items()), tuple(copied.items()), tuple(args),
                              receipt, verifier.digest(receipt))


def native_dependency_args(root: Path, directory: Path, environment: dict,
                           *, prepared: NativeDependencies | None = None) -> list[str]:
    if prepared is None:
        prepared = prepare_native_dependencies(root, directory, environment)
    else:
        retained = directory / "native-dependencies"
        retained.mkdir(parents=True, exist_ok=False)
        (retained / "binding.json").write_text(json.dumps({
            "preparation": str(prepared.receipt), "sha256": prepared.receipt_sha256,
            "arguments": prepared.arguments, "formal_evidence": False}, indent=2) + "\n")
    if (root.resolve() != prepared.root or source_hashes(root) != dict(prepared.sources)
            or verifier.digest(prepared.receipt) != prepared.receipt_sha256
            or any(verifier.digest(path) != expected for path, expected in prepared.artifacts)):
        raise ValueError("native comparison dependency binding changed")
    return list(prepared.arguments)
