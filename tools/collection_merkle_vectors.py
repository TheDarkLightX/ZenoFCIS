#!/usr/bin/env python3
"""Independent SHA-256 reference vectors for the fixed collection-tree test universe."""
import argparse
import hashlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "crates/zeno-fcis-codec/tests/data/collection-roots.tsv"
KEYS = [bytes([0x5a]) * 31 + bytes([0x50 + index]) for index in range(8)]


def sha(data):
    return hashlib.sha256(data).digest()


def root(entries, depth=0):
    if not entries:
        return sha(b"ZFCIS-MAP-EMPTY\0")
    if len(entries) == 1:
        key, record = entries[0]
        return sha(b"ZFCIS-MAP-LEAF\0" + key + sha(record))
    left = [(key, record) for key, record in entries if not (key[depth // 8] >> (7 - depth % 8)) & 1]
    right = [(key, record) for key, record in entries if (key[depth // 8] >> (7 - depth % 8)) & 1]
    return sha(b"ZFCIS-MAP-NODE\0" + root(left, depth + 1) + root(right, depth + 1))


def render():
    lines = ["# Produced by tools/collection_merkle_vectors.py; mask decimal, root hex.",
             "# Keys: 31 bytes 5a followed by 50..57; record: one byte ordinal 0..7."]
    for mask in range(256):
        entries = [(key, bytes([index])) for index, key in enumerate(KEYS) if mask >> index & 1]
        lines.append(f"{mask}\t{root(entries).hex()}")
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    expected = render()
    if args.check:
        if not TARGET.is_file() or TARGET.read_text() != expected:
            raise SystemExit("collection Merkle reference vectors differ")
    else:
        TARGET.parent.mkdir(parents=True, exist_ok=True)
        TARGET.write_text(expected)
    print("collection Merkle reference: 256 subsets")


if __name__ == "__main__":
    main()
