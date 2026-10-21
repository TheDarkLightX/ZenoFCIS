#!/usr/bin/env python3
"""Check the ingress source boundary and optional add-only reviewed lock delta.

The credential rules deliberately admit one full-width comparison and one
all-entry scan form. They do not establish compiled timing independence or
replace transport tests.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parents[1]
SOURCES = tuple(f"crates/zeno-fcis-ingress/src/{name}.rs" for name in ("lib", "http", "durable", "trace", "assets"))
APPROVED = {
    "atomic-waker": "1.1.2", "bytes": "1.12.1", "http": "1.5.0", "http-body": "1.1.0",
    "http-body-util": "0.1.5", "httparse": "1.10.1", "httpdate": "1.0.3", "hyper": "1.12.0",
    "hyper-util": "0.1.21", "mio": "1.2.4", "socket2": "0.6.5", "tokio": "1.53.2",
    "wasi": "0.11.1+wasi-snapshot-preview1",
}
COMPARE = '''fn credential_matches(candidate: &[u8; 32], configured: &[u8; 32]) -> bool {
    let mut difference = 0u8;
    for index in 0..32 {
        difference |= std::hint::black_box(candidate[index] ^ configured[index]);
    }
    difference == 0
}'''
SCAN = '''fn matched_principal(candidate: &[u8; 32], parties: &[([u8; 32], Hash32)]) -> Option<Hash32> {
    let mut found = 0u8;
    let mut principal = [0u8; 32];
    for (secret, owner) in parties {
        #[cfg(test)]
        tests::VISITS.with(|visits| visits.set(visits.get() + 1));
        let mask = 0u8.wrapping_sub(u8::from(std::hint::black_box(credential_matches(
            candidate, secret,
        ))));
        found |= mask;
        for (byte, owned) in principal.iter_mut().zip(owner.as_bytes()) {
            *byte |= owned & mask;
        }
    }
    (found != 0).then_some(Hash32::new(principal))
}'''

def source_failures(root: Path) -> list[str]:
    failures = []
    actual = {str(path.relative_to(root)) for path in (root / "crates/zeno-fcis-ingress/src").rglob("*.rs")}
    if actual != set(SOURCES):
        failures.append("ingress-source-coverage: source inventory changed; review every new boundary")
    try:
        http = (root / SOURCES[1]).read_text()
        compact = lambda text: re.sub(r"\s+", "", text)
        if compact(COMPARE) not in compact(http) or http.count("fn credential_matches(") != 1:
            failures.append("credential-constant-time: reviewed full-width comparison changed")
        if compact(SCAN) not in compact(http) or http.count("fn matched_principal(") != 1 or http.count("credential_matches(") != 2:
            failures.append("credential-all-entries: reviewed all-entry scan changed")
        if compact('matched_principal(&candidate, &config.parties).ok_or(Refusal::Credential)') not in compact(http) or http.count("matched_principal(") != 3:
            failures.append("credential-constant-time: credential admission bypassed")
        manifest = tomllib.loads((root / "crates/zeno-fcis-ingress/Cargo.toml").read_text())
        deps = manifest["dependencies"]
        expected = {
            "hyper": {"version": "=1.12.0", "default-features": False, "features": ["http1", "server"]},
            "hyper-util": {"version": "=0.1.21", "default-features": False, "features": ["tokio"]},
            "http-body-util": "=0.1.5",
            "tokio": {"version": "=1.53.2", "default-features": False, "features": ["rt", "net", "time"]},
        }
        external = {name: value for name, value in deps.items() if not isinstance(value, dict) or "path" not in value}
        if external != expected:
            failures.append("ingress-dependencies: external shell dependency/features differ from reviewed set")
    except (OSError, KeyError, tomllib.TOMLDecodeError):
        failures.append("ingress-source: unreadable source or manifest")
    return failures


def lock_delta(base: Path, candidate: Path) -> dict:
    def packages(path):
        return {(p["name"], p["version"], p.get("source", "")): p for p in tomllib.loads(path.read_text())["package"]}
    before, after = packages(base), packages(candidate)
    # Existing registry records, including checksum and dependencies, stay exact.
    removed = sorted(key for key in before if key not in after)
    changed = sorted(key for key in before.keys() & after.keys() if before[key] != after[key])
    added = {name: version for name, version, source in after.keys() - before.keys() if source}
    if removed or changed or added != APPROVED:
        raise ValueError(f"non-additive or unreviewed lock delta: removed={removed}, changed={changed}, added={added}")
    return {"registry_added": added, "removed": removed, "changed_existing": changed}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--require-loopback", action="store_true")
    parser.add_argument("--base-lock", type=Path)
    parser.add_argument("--receipt", type=Path)
    args = parser.parse_args()
    failures = source_failures(ROOT)
    if args.require_loopback and os.environ.get("INGRESS_TEST_MEMORY"):
        failures.append("loopback-required: socket-free tests cannot qualify loopback acceptance")
    receipt = {"sources": {name: hashlib.sha256((ROOT/name).read_bytes()).hexdigest() for name in SOURCES if (ROOT/name).is_file()}}
    if args.base_lock:
        try:
            receipt["lock_delta"] = lock_delta(args.base_lock, ROOT/"Cargo.lock")
        except (ValueError, OSError) as error:
            failures.append(str(error))
    receipt["failures"] = failures
    if args.receipt:
        args.receipt.write_text(json.dumps(receipt, indent=2, sort_keys=True)+"\n")
    for failure in failures:
        print(f"ingress: FAIL {failure}")
    if not failures:
        print("ingress: PASS (source coverage, reviewed credential comparison and shell dependencies)")
    return int(bool(failures))

if __name__ == "__main__":
    raise SystemExit(main())
