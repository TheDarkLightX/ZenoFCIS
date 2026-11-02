#!/usr/bin/env python3
"""Assemble reviewed, already-qualified 2.7.0 host bytes without changing tagged source."""
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tarfile
from pathlib import Path

SOURCE = "4160086fb9d19f3ebe25047e69b1caa467aa74fd"
TAG = "92f6cb07c7c920d7684be772507961602e29df15"
MAC_SHA = "3ea5df333fa4fb46d436a15cd2f69ea16179223ecada3ff07702405e02a390dd"
LINUX_SUMS = "37e1c6fe62b4a3cae846dd9b4561df26febfd2e82ba1ef7173bfa6b194b0f5e2"
CAPTURE_SHA = "08edb66857e209b9d2b345e66fb49bc98da1b370c431cef4e51ba5d2beef2abb"
POLICY_SHA = "92559e614bb9155ec8757c1599b7562c60358212202ef42e8b0b3bb891726259"
SCANNER_SHA = "1a2cc59095d61bf3f2beb4cdfb8c8d852af1f237515671d7cae3556b29073734"
ROOT = Path.cwd().resolve()
CONTROL = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / "tools"))
import check_release_privacy as privacy
import check_release_custody as custody
import rc_package

def require(value, message):
    if not value:
        raise RuntimeError(message)

def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for part in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(part)
    return digest.hexdigest()

def load(path):
    return json.loads(path.read_text())

def command(args, **kwargs):
    return subprocess.run(args, check=True, text=True, capture_output=True, **kwargs)

def api(path):
    return json.loads(command(["gh", "api", "repos/TheDarkLightX/ZenoFCIS/" + path]).stdout)

def unique(root, name):
    paths = list(root.rglob(name))
    require(len(paths) == 1, f"Expected one {name}, got {len(paths)}")
    return paths[0]

def verify_sources():
    require(command(["git", "rev-parse", "HEAD"]).stdout.strip() == SOURCE, "Wrong source checkout")
    require(not command(["git", "status", "--porcelain=v1"]).stdout, "Dirty source checkout")
    require(command(["git", "rev-parse", "refs/tags/v2.7.0"]).stdout.strip() == TAG, "Wrong tag object")
    require(command(["git", "rev-parse", "v2.7.0^{}"]).stdout.strip() == SOURCE, "Wrong tagged source")
    require(sha(ROOT / "tools/check_release_privacy.py") == SCANNER_SHA, "Scanner source changed")
    require(sha(CONTROL / "privacy-allowlist.json") == POLICY_SHA, "Reviewed policy changed")

def verify_origin():
    linux = api("actions/jobs/125093378200")
    mac = api("actions/jobs/125108985672")
    require(linux["run_id"] == 41384803560 and linux["head_sha"] == SOURCE
            and linux["conclusion"] == "success", "Linux qualification job did not pass")
    require(mac["run_id"] == 41393248258
            and mac["head_sha"] == "10cf9f715f8c02dd54dcc678cd9e714eb1a18782",
            "Wrong native macOS capture controller")
    native = "Record the native installed-binary journey for independent review"
    require(any(s["name"] == native and s["conclusion"] == "success" for s in mac["steps"]),
            "Native macOS installed-binary journey did not pass")
    failures = {s["name"] for s in mac["steps"] if s["conclusion"] == "failure"}
    require(failures == {"Run the unchanged host builder and retain its refusal"},
            "Native capture has an unreviewed failure")
    wanted = [(13116327353, 41384803560,
               "sha256:a89d33e8259d184bbae0d3f3782982e4aa2c81d9484d464eb32f566230dba67e"),
              (13118900597, 41393248258,
               "sha256:8226e53b53dab170459ca24dc8d07b824a33034ac364a9048109914743decf26")]
    for artifact_id, run_id, digest in wanted:
        item = api(f"actions/artifacts/{artifact_id}")
        require(item["workflow_run"]["id"] == run_id and not item["expired"]
                and item["digest"] == digest, "Artifact origin/digest changed")
    return {"linux_run": 41384803560, "linux_job": 125093378200,
            "macos_capture_run": 41393248258, "macos_native_job": 125108985672,
            "macos_baseline_privacy": "refused eleven reviewed public-label matches",
            "native_journeys": "both passed at source 4160086f; reused byte-for-byte"}

def verify_sums(release):
    listed = rc_package.read_sha256sums(release)
    for name, expected in listed.items():
        path = release / name
        require(path.resolve().is_relative_to(release.resolve())
                and path.is_file() and not path.is_symlink(), "Unsafe/missing release path")
        require(sha(path) == expected, f"Checksum mismatch: {name}")
    return len(listed)

def scan_folder(folder, policy):
    scan = privacy.Scan(allowlist=policy)
    for child in sorted(folder.rglob("*")):
        if not child.is_dir() or child.is_symlink():
            scan.file(child, child.relative_to(folder).as_posix())
    report = scan.report()
    require(report["status"] == "passed", "Privacy gate refused reviewed artifacts")
    return report

def decrypt_capture(folder, output, key_file):
    capture = unique(folder, "CAPTURE.json")
    require(sha(capture) == CAPTURE_SHA, "Capture manifest changed")
    info = load(capture)
    require(info["source_commit"] == SOURCE and info["tag_object"] == TAG,
            "Capture describes another source/tag")
    ciphertext = unique(folder, "macos-review.cms")
    require(sha(ciphertext) == info["ciphertext_sha256"], "Ciphertext hash differs")
    certificate = CONTROL / "recipient.crt"
    require(sha(certificate) == info["certificate_sha256"], "Recipient certificate changed")
    transient = key_file is None
    if transient:
        key_file = output / ".capture-transport.key"
        key = os.environ.pop("CAPTURE_DECRYPTION_KEY")
        key_file.write_text(key)
        key_file.chmod(0o600)
    plain = output / ".macos-review.tar"
    try:
        command(["openssl", "cms", "-decrypt", "-binary", "-inform", "DER",
                 "-in", str(ciphertext), "-recip", str(certificate),
                 "-inkey", str(key_file), "-out", str(plain)])
    finally:
        if transient:
            key_file.unlink(missing_ok=True)
    require(sha(plain) == info["plaintext_tar_sha256"], "Decrypted package changed")
    extracted = output / ".macos-capture"
    extracted.mkdir(mode=0o700)
    with tarfile.open(plain) as archive:
        members = archive.getmembers()
        require(len(members) <= 100, "Excess diagnostic members")
        total = 0
        for member in members:
            path = extracted / member.name
            require(path.resolve().is_relative_to(extracted.resolve()), "Unsafe diagnostic path")
            if member.isdir():
                path.mkdir(parents=True, exist_ok=True, mode=0o700)
                continue
            total += member.size
            require(member.isfile() and member.size <= privacy.MAX_FILE_BYTES
                    and total <= privacy.MAX_TOTAL_BYTES, "Unsupported diagnostic member")
            path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            path.write_bytes(archive.extractfile(member).read())
            path.chmod(0o600)
    return extracted / "host", info

def host_bindings(release, host, bindings):
    record = load(release / "HOST-BINARY.json")
    require((record["source_commit"], record["version"], record["host"])
            == (SOURCE, "2.7.0", host), "Wrong host record")
    files = {"archive_sha256": release / "binaries" / bindings["archive"],
             "record_sha256": release / "HOST-BINARY.json",
             "inputs_sha256": release / f"COMPILED-INPUTS-{host}.json",
             "journey_sha256": release / "INSTALLED-JOURNEY.json"}
    for field, path in files.items():
        require(sha(path) == bindings[field], f"Host output differs: {field}")
    require(not custody.journey_problems(load(files["journey_sha256"]), host, "2.7.0",
                                        bindings["archive_sha256"]), "Journey record refused")
    problems, _ = custody.binary_archive_problems(
        files["archive_sha256"], "2.7.0", host, rc_package.CommittedDocuments(ROOT, SOURCE), record)
    require(not problems, "Host archive refused: " + "; ".join(problems[:3]))
    require(record["executable"]["sha256"] == MAC_SHA, "Unreviewed macOS executable")

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--linux", type=Path, required=True)
    parser.add_argument("--capture", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--key-file", type=Path)
    args = parser.parse_args()
    verify_sources()
    require(not args.output.resolve().is_relative_to(ROOT), "Output must be outside frozen source")
    require(not args.output.exists(), "Output directory must be new")
    args.output.mkdir(mode=0o700)
    origin = verify_origin()
    evidence = args.output / "evidence"
    evidence.mkdir()
    linux_manifest = unique(args.linux, "RC-MANIFEST.json")
    linux = linux_manifest.parent
    require(load(linux_manifest)["source_commit"] == SOURCE, "Wrong Linux source")
    require(sha(linux / "SHA256SUMS") == LINUX_SUMS, "Linux input inventory changed")
    previous = load(unique(args.linux, "CUSTODY.json"))
    require(previous["status"] == "passed" and previous["source_commit"] == SOURCE,
            "Linux custody was not passed")
    bindings = load(unique(args.linux, "JOB-OUTPUTS.json"))
    require(set(bindings) == {"x86_64-unknown-linux-gnu"}, "Unexpected base host set")
    input_count = verify_sums(linux)
    host, capture = decrypt_capture(args.capture, args.output, args.key_file)
    host_bindings(host, "aarch64-apple-darwin", capture["host_bindings"])
    base = privacy.load_allowlist(ROOT / "release/privacy-allowlist.json")
    reviewed = privacy.load_allowlist(CONTROL / "privacy-allowlist.json")
    require(base <= reviewed and len(base) == 62 and len(reviewed) == 84,
            "Reviewed policy does not preserve the original exceptions")
    mac_privacy = scan_folder(host, reviewed)
    require(mac_privacy["allowlisted_matches"] == 11, "Unexpected macOS match count")
    release = args.output / "release"
    shutil.copytree(linux, release, symlinks=True)
    rc_package.merge_hosts(release, [host], {}, repo=ROOT)
    bindings["aarch64-apple-darwin"] = capture["host_bindings"]
    (evidence / "JOB-OUTPUTS.json").write_text(json.dumps(bindings, indent=2, sort_keys=True) + "\n")
    (evidence / "DOWNLOADS.sha256").write_text(rc_package.downloads_lines(release))
    full_privacy = scan_folder(release, reviewed)
    (evidence / "PRIVACY.json").write_text(json.dumps(full_privacy, indent=2) + "\n")
    for name in ("privacy-allowlist.json", "MACOS-PRIVACY-REVIEW.json"):
        shutil.copyfile(CONTROL / name, evidence / name)
    origin.update({"source_commit": SOURCE, "tag_object": TAG,
                   "controller_commit": os.environ.get("GITHUB_SHA", "local-preflight"),
                   "scanner_sha256": SCANNER_SHA, "reviewed_policy_sha256": POLICY_SHA,
                   "linux_input_files_verified": input_count,
                   "qualification": "Completed native host bytes reused; privacy supplement reviewed independently; no release source changed"})
    (evidence / "RELEASE-CONTROLLER.json").write_text(json.dumps(origin, indent=2) + "\n")
    result = command(["python3", "-B", "tools/check_release_custody.py", "--release", str(release),
                      "--report", str(evidence / "CUSTODY.json"), "--bindings", str(evidence / "JOB-OUTPUTS.json"),
                      "--downloads", str(evidence / "DOWNLOADS.sha256"), "--tag", "v2.7.0",
                      "--remote-tag-commit", SOURCE], cwd=ROOT)
    report = load(evidence / "CUSTODY.json")
    require(report["status"] == "passed" and not report["not_run"] and len(report["checks"]) == 11,
            "Combined custody did not pass all eleven checks")
    verify_sources()
    print(json.dumps({"status": "passed", "source_commit": SOURCE, "hosts": sorted(bindings),
                      "custody_checks": len(report["checks"]),
                      "privacy_allowlisted_matches": full_privacy["allowlisted_matches"],
                      "downloads": (evidence / "DOWNLOADS.sha256").read_text().splitlines(),
                      "output": str(args.output)}))

if __name__ == "__main__":
    main()

