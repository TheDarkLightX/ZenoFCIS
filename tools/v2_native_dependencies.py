"""Link direct-source native harnesses to freshly built codec/value oracles.

Callers serialize compilation with the fleet lock. These artifacts are native
comparison dependencies; building them does not supply formal proof evidence.
"""
from __future__ import annotations

import json
import os
from pathlib import Path

import check_verus as verifier


def native_dependency_args(root: Path, directory: Path, environment: dict) -> list[str]:
    target = Path(environment.get("CARGO_TARGET_DIR", os.environ.get("CARGO_TARGET_DIR", str(root / "target"))))
    if not target.is_dir():
        raise ValueError("native dependencies require an existing shared target")
    retained = directory / "native-dependencies"
    retained.mkdir(parents=True, exist_ok=False)
    files = [root / "Cargo.toml", root / "Cargo.lock", root / "rust-toolchain.toml", root / ".cargo/config.toml"]
    for package in ("zeno-fcis-codec", "zeno-fcis-value"):
        base = root / "crates" / package
        files.append(base / "Cargo.toml")
        files.extend(sorted((base / "src").rglob("*.rs")))
    before = {str(p.relative_to(root)): verifier.digest(p) for p in files}
    # Preserve bytes while forcing both actual local library compilations.
    for source in files:
        if source.suffix == ".rs":
            source.touch()
    env = {k: v for k, v in environment.items() if not k.startswith(("VERUS_", "VARGO_"))
           and k not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")}
    env.update(CARGO_TARGET_DIR=str(target), CARGO_BUILD_JOBS="1", CARGO_INCREMENTAL="0",
               CARGO_NET_OFFLINE="true", RUST_TEST_THREADS="1")
    command = ["cargo", "+1.97.1", "build", "--locked", "--offline", "-vv", "-p",
               "zeno-fcis-codec", "--message-format=json"]
    run = verifier.run(command, root, env)
    (retained / "cargo.stdout").write_text(run.stdout)
    (retained / "cargo.stderr").write_text(run.stderr)
    verifier.require_success(run)
    libraries = {}
    expected = {"zeno_fcis_codec": "zeno-fcis-codec", "zeno_fcis_value": "zeno-fcis-value"}
    for line in run.stdout.splitlines():
        row = json.loads(line)
        name = row.get("target", {}).get("name")
        if row.get("reason") != "compiler-artifact" or name not in expected:
            continue
        manifest = root / "crates" / expected[name] / "Cargo.toml"
        if row.get("fresh") is not False or Path(row["manifest_path"]).resolve() != manifest.resolve():
            raise ValueError("native oracle artifact lacks fresh current-source provenance: " + name)
        artifact = next(Path(p) for p in row["filenames"] if p.endswith(".rlib"))
        libraries[name] = {"path": str(artifact), "sha256": verifier.digest(artifact),
                           "manifest_path": str(manifest), "fresh": False}
    if set(libraries) != set(expected):
        raise ValueError("actual codec and value native oracle artifacts are missing")
    after = {str(p.relative_to(root)): verifier.digest(p) for p in files}
    if before != after:
        raise ValueError("native comparison dependency source changed during compilation")
    (retained / "result.json").write_text(json.dumps({"status": "passed", "command": command,
        "cwd": str(root), "exit_code": run.returncode, "source_sha256": before,
        "source_stable": True, "libraries": libraries, "formal_evidence": False}, indent=2) + "\n")
    args = ["-L", "dependency=" + str(target / "debug/deps")]
    for name, artifact in sorted(libraries.items()):
        args.extend(("--extern", name + "=" + artifact["path"]))
    return args
