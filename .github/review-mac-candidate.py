"""Admit a Mac build for encrypted review, never for publication."""
import hashlib
import json
from pathlib import Path
import sys
import tarfile

SOURCE = "7d175c47099866939c86acd463ac85219abba882"
HOST = "aarch64-apple-darwin"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def check(exit_code, directory, log):
    root = Path(directory)
    record = json.loads((root / "HOST-BINARY.json").read_text())
    inputs = json.loads((root / f"COMPILED-INPUTS-{HOST}.json").read_text())
    require((record["source_commit"], record["version"], record["host"]) == (SOURCE, "2.6.0", HOST),
            "unexpected host record")
    require(record["deployment_target"] == "11.0", "unexpected deployment target")
    require((inputs["source_commit"], inputs["version"], inputs["host"]) == (SOURCE, "2.6.0", HOST),
            "unexpected compiled input record")
    name = f"zeno-fcis-2.6.0-{HOST}.tar.gz"
    require(record["archive"]["name"] == name, "unexpected archive name")
    archive = root / "binaries" / name
    require(archive.is_file() and not archive.is_symlink()
            and sha(archive.read_bytes()) == record["archive"]["sha256"], "archive differs from record")
    with tarfile.open(archive, "r:gz") as package:
        members = package.getmembers()
        executables = [m for m in members if m.isfile() and m.name == f"zeno-fcis-2.6.0-{HOST}/zeno-fcis"]
        require(len(executables) == 1, "missing unique executable")
        executable = package.extractfile(executables[0]).read()
    binary_sha = sha(executable)
    require(binary_sha == record["executable"]["sha256"]
            and binary_sha == inputs["executable"]["sha256"], "executable differs from records")
    reports = []
    for line in Path(log).read_text(errors="replace").splitlines():
        try:
            value = json.loads(line)
        except ValueError:
            continue
        if isinstance(value, dict) and value.get("schema") == "zeno-fcis/release-privacy-check/1":
            reports.append(value)
    require(len(reports) == 1 and not reports[0]["errors"], "missing or invalid privacy report")
    report = reports[0]
    if exit_code == 0:
        require(report["status"] == "passed" and not report["findings"], "unexpected passing scan")
    else:
        require(exit_code == 2 and report["status"] == "failed" and report["findings"],
                "unexpected build failure")
        expected_location = f"binaries/{name}!zeno-fcis-2.6.0-{HOST}/zeno-fcis"
        for finding in report["findings"]:
            require(finding.get("code") == "model-api-token"
                    and finding.get("location") == expected_location
                    and finding.get("file_sha256") == binary_sha,
                    "privacy refusal includes an unreviewed class or location")
    return {"build_exit": exit_code, "privacy_status": report["status"],
            "privacy_findings": len(report["findings"]),
            "archive_sha256": record["archive"]["sha256"], "executable_sha256": binary_sha,
            "source_commit": SOURCE, "host": HOST, "qualification": "encrypted review only"}


if __name__ == "__main__":
    try:
        print(json.dumps(check(int(sys.argv[1]), sys.argv[2], sys.argv[3]), sort_keys=True))
    except (OSError, ValueError, KeyError, IndexError, tarfile.TarError) as error:
        print(f"candidate review refused: {error}", file=sys.stderr)
        raise SystemExit(1)
