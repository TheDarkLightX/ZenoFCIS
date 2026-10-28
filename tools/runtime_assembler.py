#!/usr/bin/env python3
"""Closed experimental task-status assembly using the existing checked app factory.

Receipts record checks and identities, never publication authority. This is not
an OS sandbox, a migration tool, or a complete A1–A12 qualification.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
FORMAT = "zeno-fcis/app-assembly/1"
PROFILE = "task-status-sqlite-local-test/1"
CAPABILITIES = ["authenticated-loopback", "single-sqlite-store", "local-deduplicating-receiver"]
MAX_JSON = 65536
MAX_FILE = 128 * 1024 * 1024
CONTRACT_FILES = ["project.zeno", "v2/policy.json", "v2/schema.zcve", "v2/policy.zcve", "src/v2_contract.rs", "ui.json", "tests/decision-examples.txt", "tests/decision-scenarios.txt"]

CATALOG = {
    PROFILE: {
        "contract": "examples/apps/task-status", "files": CONTRACT_FILES,
        "roles": [("operator", "170"), ("adapter", "171"), ("observer", "172")],
        "evidence": "task-status-runtime", "support": "task_journey.rs",
        "journey": "test-data/app-journeys/task-status.json",
        "tests": ["parties_journey", "assembly_real_process_loss_retains_delivery_and_original_request",
                  "assembly_timeout_is_unknown_and_correlated_observations_use_authority",
                  "assembly_old_pending_delivery_cannot_complete_new_attempt"],
    },
    "inventory-reservation-sqlite-local-simulation/1": {
        "contract": "crates/zeno-fcis-cli/templates/inventory-reservation",
        "files": [name for name in CONTRACT_FILES if name != "tests/decision-scenarios.txt"],
        "roles": [("operator", "1"), ("observer", "0")],
        "evidence": "inventory-local-runtime", "support": "inventory_journey.rs",
        "journey": "tools/runtime-assembly/inventory-parties.json",
        "tests": ["parties_journey", "assembly_inventory_authority_and_local_intent",
                  "assembly_inventory_completed_request_restart_keeps_one_local_intent"],
    },
}


def profile(name):
    if not isinstance(name, str) or name not in CATALOG:
        refuse("AssemblyUnsupportedProfile")
    return CATALOG[name]


def required_evidence(selected):
    return ["native-contract-review", "zenoui-laws", "generated-app-journey", selected["evidence"]]


def refuse(code):
    raise ValueError(code)


def canonical(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True) + "\n").encode()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def plain(path):
    path = Path(path).absolute()
    if any(p.is_symlink() for p in [path, *path.parents]):
        refuse("AssemblySymlink")
    return path.resolve()


def read(path, maximum=MAX_FILE):
    path = plain(path)
    with path.open("rb") as stream:
        data = stream.read(maximum + 1)
    if len(data) > maximum:
        refuse("AssemblyByteLimit")
    return data


def executable_digest(path):
    hasher = hashlib.sha256()
    total = 0
    with plain(path).open("rb") as stream:
        while block := stream.read(1024 * 1024):
            total += len(block)
            if total > 1024 * 1024 * 1024:
                refuse("AssemblyExecutableByteLimit")
            hasher.update(block)
    return hasher.hexdigest()


def pairs(items):
    result = {}
    for key, value in items:
        if key in result:
            refuse("AssemblyDuplicateField")
        result[key] = value
    return result


def decode(path):
    return json.loads(read(path, MAX_JSON), object_pairs_hook=pairs,
                      parse_constant=lambda _: refuse("AssemblyNonfiniteNumber"))


def keys(value, expected):
    if not isinstance(value, dict) or set(value) != set(expected):
        refuse("AssemblyFields")


def files(root, names):
    return {name: digest(read(root / name)) for name in names}


def source_identity(root=ROOT):
    # Conservative build-input inventory. No graph/proof or OS isolation claim.
    names = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml"]
    for directory in [".cargo", "crates", "site", "tools", "test-data", "verification/verus"]:
        for path in sorted((root / directory).rglob("*")):
            relative = path.relative_to(root)
            if any(part in {"target", "node_modules", "__pycache__", ".zeno-fcis"} for part in relative.parts):
                continue
            if relative.parts[:2] == ("site", "apps"):
                continue
            if path.is_symlink():
                refuse("AssemblySourceSymlink")
            if path.is_file() and path.suffix != ".pyc":
                names.append(relative.as_posix())
    return digest(canonical(files(root, sorted(set(names)))))


def validate_config(config, selected=None):
    selected = selected or profile(PROFILE)
    keys(config, ["address", "data_env", "parties"])
    if config["address"] != "127.0.0.1:8080" or config["data_env"] != "ZENO_ASSEMBLY_DATA":
        refuse("AssemblyConfigurationProfile")
    if not isinstance(config["parties"], list) or len(config["parties"]) != len(selected["roles"]):
        refuse("AssemblyParties")
    principals = set()
    for party, (name, role) in zip(config["parties"], selected["roles"]):
        keys(party, ["name", "principal", "role", "credential_env"])
        if party["name"] != name or party["role"] != role or party["credential_env"] != "ZENO_ASSEMBLY_" + name.upper() + "_SECRET":
            refuse("AssemblyPartyBinding")
        principal = party["principal"]
        if not isinstance(principal, str) or len(principal) != 64 or any(c not in "0123456789abcdef" for c in principal) or principal == "0" * 64 or principal in principals:
            refuse("AssemblyPrincipal")
        principals.add(principal)


def validate(document):
    keys(document, ["format", "profile", "capabilities", "source_sha256", "cli_sha256", "contract", "configuration", "evidence"])
    if document["format"] != FORMAT:
        refuse("AssemblyVersion")
    selected = profile(document["profile"])
    if document["capabilities"] != CAPABILITIES:
        refuse("AssemblyCapabilities")
    if document["evidence"] != required_evidence(selected):
        refuse("AssemblyEvidenceRequirements")
    for field in ["source_sha256", "cli_sha256"]:
        value = document[field]
        if not isinstance(value, str) or len(value) != 64 or any(c not in "0123456789abcdef" for c in value):
            refuse("AssemblyDigest")
    keys(document["contract"], selected["files"])
    if document["contract"] != files(ROOT / selected["contract"], selected["files"]):
        refuse("AssemblyContractIdentity")
    validate_config(document["configuration"], selected)


def prepare(cli, name=PROFILE):
    selected = profile(name)
    return {"format": FORMAT, "profile": name, "capabilities": CAPABILITIES,
            "source_sha256": source_identity(), "cli_sha256": executable_digest(cli),
            "contract": files(ROOT / selected["contract"], selected["files"]),
            "configuration": {"address": "127.0.0.1:8080", "data_env": "ZENO_ASSEMBLY_DATA", "parties": [
                {"name": name, "principal": format(index + 161, "02x") * 32, "role": role, "credential_env": "ZENO_ASSEMBLY_" + name.upper() + "_SECRET"}
                for index, (name, role) in enumerate(selected["roles"])]},
            "evidence": required_evidence(selected)}


def check_identity(document, cli):
    if document["source_sha256"] != source_identity():
        refuse("AssemblySourceIdentity")
    if document["cli_sha256"] != executable_digest(cli):
        refuse("AssemblyCliIdentity")


def run_checked(command, log, cwd, environment):
    with log.open("wb") as stream:
        result = subprocess.run(list(map(str, command)), cwd=cwd, env=environment,
                                stdin=subprocess.DEVNULL, stdout=stream, stderr=subprocess.STDOUT)
    if result.returncode:
        # Retain diagnostics outside staging, never an installable destination.
        stage = next((parent for parent in log.parents if parent.name.startswith(".assembly-")), None)
        diagnostic = stage.parent / (stage.name + "-failed-" + log.name) if stage else log.with_suffix(".failed.log")
        shutil.copyfile(log, diagnostic)
        print("assembly diagnostic: " + str(diagnostic), file=sys.stderr)
        if stage:
            retained = Path(tempfile.mkdtemp(prefix=stage.name + "-failed-evidence-", dir=stage.parent))
            shutil.copytree(log.parent, retained, dirs_exist_ok=True)
            print("assembly stage logs: " + str(retained), file=sys.stderr)
        refuse("AssemblyCheckFailed:" + log.name)
    return {"check": log.stem, "exit": 0, "log_sha256": digest(read(log, 64 * 1024 * 1024))}


def build(description, cli, output, scratch):
    document = decode(description)
    validate(document)
    selected = profile(document["profile"])
    cli = plain(cli)
    check_identity(document, cli)
    output = plain(output)
    scratch = plain(scratch)
    if output.exists() or output.is_relative_to(ROOT) or scratch.is_relative_to(ROOT):
        refuse("AssemblyOutputOrScratch")
    output.parent.mkdir(parents=True, exist_ok=True)
    scratch.mkdir(parents=True, exist_ok=True)
    environment = dict(os.environ)
    allowed = {"CARGO_HOME", "CARGO_TARGET_DIR", "RUSTUP_HOME"}
    for key in environment:
        if ((key.startswith(("CARGO_", "RUST", "CC_", "CXX_", "LD_", "PKG_CONFIG")) and key not in allowed)
                or key in {"CC", "CXX", "AR", "LD", "CFLAGS", "CXXFLAGS", "LDFLAGS", "PYTHONOPTIMIZE", "PYTHONPATH", "PYTHONHOME"}):
            refuse("AssemblyBuildEnvironment")
    for parent in {ROOT, *ROOT.parents, output.parent, *output.parents, scratch, *scratch.parents}:
        if parent != ROOT and any((parent / ".cargo" / name).exists() for name in ["config", "config.toml"]):
            refuse("AssemblyAmbientCargoConfiguration")
    cargo_home = Path(environment.get("CARGO_HOME", Path.home() / ".cargo"))
    if any((cargo_home / name).exists() for name in ["config", "config.toml"]):
        refuse("AssemblyAmbientCargoConfiguration")
    if any(key.startswith("ZENO_APP_") for key in environment):
        refuse("AssemblyAmbientAppConfiguration")
    environment["INGRESS_TEST_MEMORY"] = "1"
    environment.pop("APP_JOURNEY_TCP", None)
    with tempfile.TemporaryDirectory(prefix=".assembly-", dir=output.parent) as temporary:
        stage = Path(temporary)
        package = stage / "package"
        package.mkdir()
        evidence = package / "evidence"
        evidence.mkdir()
        frozen = stage / "contract"
        for name in selected["files"]:
            path = frozen / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(read(ROOT / selected["contract"] / name))
        if files(frozen, selected["files"]) != document["contract"]:
            refuse("AssemblyContractChanged")
        app = package / "source"
        records = []
        for label, command in [("rustc-version", ["rustc", "+1.97.1", "--version", "--verbose"]),
                               ("cargo-version", ["cargo", "+1.97.1", "--version"]),
                               ("node-version", ["node", "--version"]),
                               ("python-version", [sys.executable, "--version"])]:
            records.append(run_checked(command, evidence / (label + ".log"), ROOT, environment))
        records.append(run_checked([cli, "new", app, "--contract", frozen, "--app", "--source", ROOT, "--scratch", scratch], evidence / "factory.log", ROOT, environment))
        # Original factory receipt precedes our added acceptance test; retain it
        # under an explicit intermediate name, not as final file provenance.
        (app / "app-manifest.json").rename(evidence / "factory-manifest.json")
        (app / "tests/support").mkdir()
        shutil.copyfile(ROOT / "tools/runtime-assembly" / selected["support"], app / "tests/support/profile_journey.rs")
        shutil.copyfile(ROOT / selected["journey"], app / "app-checks/journey.json")
        with (app / "tests/app_journey.rs").open("a") as stream:
            stream.write('\ninclude!("support/profile_journey.rs");\n')
        environment["CARGO_TARGET_DIR"] = str(scratch / "runtime-assembly-target")
        cargo = ["cargo", "+1.97.1"]
        records.append(run_checked([*cargo, "test", "--locked", "--offline", "--all-features", "--test", "app_journey"], evidence / "runtime.log", app, environment))
        for test in selected["tests"]:
            if ("test " + test + " ... ok") not in (evidence / "runtime.log").read_text():
                refuse("AssemblyMissingRuntimeEvidence:" + test)
        records.append(run_checked([*cargo, "build", "--locked", "--offline", "--release", "--bin", "app-server"], evidence / "build.log", app, environment))
        binary = package / "app-server"
        shutil.copyfile(Path(environment["CARGO_TARGET_DIR"]) / "release/app-server", binary)
        binary.chmod(0o755)
        shutil.copyfile(ROOT / "tools/runtime-assembly/launch.py", package / "launch.py")
        (package / "assembly.json").write_bytes(canonical(document))
        # No external pass flag is accepted: each required check ran in this build.
        (evidence / "checks.json").write_bytes(canonical({"format": "zeno-fcis/assembly-checks/1", "transport": "memory-http", "checks": records, "complete_profile": False}))
        check_identity(document, cli)
        if files(frozen, selected["files"]) != document["contract"]:
            refuse("AssemblyContractChanged")
        names = [p.relative_to(package).as_posix() for p in sorted(package.rglob("*")) if p.is_file()]
        receipt = {"format": "zeno-fcis/assembled-app/1", "authority": "none", "assembly_sha256": digest(canonical(document)), "files": files(package, names),
                   "limits": ["trusted single host and writer", "local non-value-moving receiver", "manual authenticated adapter observations or local shipment-intent simulation only", "no OS sandbox", "no migration or backup qualification", "browser qualification not included", "A1-A12 incomplete"]}
        (package / "package.json").write_bytes(canonical(receipt))
        # Exercise the exact copied release binary and startup validator, not
        # merely the debug integration-test executable.
        records.append(run_checked([sys.executable, ROOT / "tools/runtime-assembly/check_package.py", "--package", package, "--scratch", scratch], evidence / "package-launch.log", ROOT, environment))
        (evidence / "checks.json").write_bytes(canonical({"format": "zeno-fcis/assembly-checks/1", "transport": "memory-http and loopback-smoke", "checks": records, "complete_profile": False}))
        check_identity(document, cli)
        names = [p.relative_to(package).as_posix() for p in sorted(package.rglob("*")) if p.is_file() and p != package / "package.json"]
        receipt["files"] = files(package, names)
        (package / "package.json").write_bytes(canonical(receipt))
        if output.exists():
            refuse("AssemblyOutputExists")
        package.rename(output)
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", required=True, type=Path)
    parser.add_argument("--prepare", action="store_true", help="Print a candidate binding; owner must review configuration and premises.")
    parser.add_argument("--profile", choices=sorted(CATALOG), default=PROFILE, help="Reviewed catalog entry for --prepare only.")
    parser.add_argument("--assembly", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--scratch", type=Path)
    args = parser.parse_args()
    if args.prepare:
        if args.assembly or args.out or args.scratch:
            refuse("AssemblyArguments")
        sys.stdout.buffer.write(canonical(prepare(args.cli, args.profile)))
    else:
        if not all([args.assembly, args.out, args.scratch]):
            refuse("AssemblyArguments")
        build(args.assembly, args.cli, args.out, args.scratch)
        print("Assembly built; profile qualification remains incomplete. See package.json.")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        print("assembly refused: " + str(error), file=sys.stderr)
        sys.exit(1)
