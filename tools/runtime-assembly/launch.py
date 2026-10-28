#!/usr/bin/env python3
"""Validate one assembled local test application before launching it.

The deployment owner and filesystem are trusted. This is startup drift checking,
not protection against an owner replacing the launcher or invoking a raw binary.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys

MAX_FILE = 256 * 1024 * 1024


def refuse(code):
    raise ValueError(code)


def plain(path):
    path = Path(path).absolute()
    if any(p.is_symlink() for p in [path, *path.parents]):
        refuse("LaunchSymlink")
    return path.resolve()


def read(path, maximum=MAX_FILE):
    with plain(path).open("rb") as stream:
        data = stream.read(maximum + 1)
    if len(data) > maximum:
        refuse("LaunchByteLimit")
    return data


def pairs(items):
    value = {}
    for key, item in items:
        if key in value:
            refuse("LaunchDuplicateField")
        value[key] = item
    return value


def decode(data):
    return json.loads(data, object_pairs_hook=pairs)


def canonical(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True) + "\n").encode()


def validate(root, environment):
    root = plain(root)
    if any(key.startswith("ZENO_APP_") for key in environment):
        refuse("LaunchAmbientAppConfiguration")
    receipt = decode(read(root / "package.json", 4 * 1024 * 1024))
    if receipt.get("format") != "zeno-fcis/assembled-app/1" or receipt.get("authority") != "none":
        refuse("LaunchManifest")
    names = receipt.get("files")
    if not isinstance(names, dict) or not {"app-server", "launch.py", "assembly.json", "evidence/checks.json"}.issubset(names):
        refuse("LaunchManifestFiles")
    for name, expected in names.items():
        if not isinstance(name, str) or not name or Path(name).is_absolute() or ".." in Path(name).parts or Path(name).as_posix() != name:
            refuse("LaunchManifestPath")
        if hashlib.sha256(read(root / name)).hexdigest() != expected:
            refuse("LaunchArtifactDrift")
    assembly_bytes = read(root / "assembly.json", 65536)
    if hashlib.sha256(assembly_bytes).hexdigest() != receipt.get("assembly_sha256"):
        refuse("LaunchAssemblyBinding")
    assembly = decode(assembly_bytes)
    profiles = {
        "task-status-sqlite-local-test/1": [("operator", "170"), ("adapter", "171"), ("observer", "172")],
        "inventory-reservation-sqlite-local-simulation/1": [("operator", "1"), ("observer", "0")],
    }
    if assembly.get("format") != "zeno-fcis/app-assembly/1" or assembly.get("profile") not in profiles:
        refuse("LaunchProfile")
    if assembly.get("capabilities") != ["authenticated-loopback", "single-sqlite-store", "local-deduplicating-receiver"]:
        refuse("LaunchCapabilities")
    roles = profiles[assembly["profile"]]
    config = assembly.get("configuration", {})
    if set(config) != {"address", "data_env", "parties"} or config["address"] != "127.0.0.1:8080" or config["data_env"] != "ZENO_ASSEMBLY_DATA":
        refuse("LaunchConfiguration")
    if not isinstance(config["parties"], list) or len(config["parties"]) != len(roles):
        refuse("LaunchParties")
    data_name = environment.get(config["data_env"], "")
    if not data_name or not Path(data_name).is_absolute():
        refuse("LaunchDataReference")
    data = plain(data_name)
    if data == root or data.is_relative_to(root):
        refuse("LaunchDataInsidePackage")
    parties = []
    secrets = set()
    principals = set()
    for party, (name, role) in zip(config["parties"], roles):
        if set(party) != {"name", "principal", "role", "credential_env"} or party["name"] != name or party["role"] != role or party["credential_env"] != "ZENO_ASSEMBLY_" + name.upper() + "_SECRET":
            refuse("LaunchPartyBinding")
        principal = party["principal"]
        if not isinstance(principal, str) or not re.fullmatch("[0-9a-f]{64}", principal) or principal == "0" * 64 or principal in principals:
            refuse("LaunchPrincipal")
        principals.add(principal)
        reference = environment.get(party["credential_env"], "")
        if not reference or not Path(reference).is_absolute():
            refuse("LaunchSecretReference")
        credential = plain(reference)
        if credential.is_relative_to(root):
            refuse("LaunchSecretInsidePackage")
        mode = credential.stat()
        if not stat.S_ISREG(mode.st_mode) or mode.st_mode & 0o077 or mode.st_uid != os.getuid():
            refuse("LaunchSecretPermissions")
        secret = read(credential, 65).removesuffix(b"\n")
        if not re.fullmatch(b"[0-9a-f]{64}", secret) or secret == b"0" * 64 or secret in secrets:
            refuse("LaunchSecretFormat")
        secrets.add(secret)
        parties.append({"name": name, "principal": principal, "credential": str(credential), "context": str(data / (name + ".context.json"))})
    # Existing application validates credentials again; never retain their bytes.
    secrets.clear()
    return data, config, parties


def launch(root, environment):
    data, config, parties = validate(root, environment)
    data.mkdir(mode=0o700, parents=True, exist_ok=True)
    if data.stat().st_mode & 0o077 or data.stat().st_uid != os.getuid():
        refuse("LaunchDataPermissions")
    binding = data / "assembly-binding.json"
    expected = read(root / "assembly.json", 65536)
    if binding.exists():
        if read(binding, 65536) != expected:
            refuse("LaunchStoredBindingDrift")
    else:
        # No implicit adoption, migration or reinitialization of retained data.
        if any(data.iterdir()):
            refuse("LaunchUnboundExistingData")
        with binding.open("xb") as stream:
            stream.write(expected)
            stream.flush()
            os.fsync(stream.fileno())
    for party, entry in zip(config["parties"], parties):
        path = plain(entry["context"])
        contents = canonical([party["role"]])
        if path.exists() and read(path, 65536) != contents:
            refuse("LaunchContextDrift")
        if not path.exists():
            path.write_bytes(contents)
    parties_path = plain(data / "parties.json")
    content = canonical({"schema": "zeno-fcis/app-parties/1", "parties": parties})
    # Secret rotation changes only reviewed references; principal/roles stay bound.
    parties_path.write_bytes(content)
    child = {"PATH": "/usr/bin:/bin", "ZENO_APP_ADDRESS": config["address"], "ZENO_APP_PARTIES": str(parties_path),
             "ZENO_APP_STORE": str(data / "store.sqlite"), "ZENO_APP_RECEIVER": str(data / "receiver")}
    os.execve(root / "app-server", [str(root / "app-server")], child)


if __name__ == "__main__":
    try:
        if len(sys.argv) != 1:
            refuse("LaunchArguments")
        launch(Path(__file__).resolve().parent, dict(os.environ))
    except (ValueError, OSError, TypeError, KeyError) as error:
        # Fixed codes only: paths and credential bytes do not enter diagnostics.
        message = str(error) if isinstance(error, ValueError) and str(error).startswith("Launch") else "LaunchConfigurationUnavailable"
        print("application refused: " + message, file=sys.stderr)
        sys.exit(1)
