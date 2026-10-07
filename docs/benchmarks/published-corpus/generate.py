#!/usr/bin/env python3
"""Regenerate the published 100-case Boolean corpus as canonical programs.

The corpus is calibration data only. Its formulas come from the semantic
e-graph study (see README.md for the exact source and digests); this script
lowers each formula to a canonical `zeno-fcis/finite-i64/1` program exactly as
the study's harness did, so the optimizer and the transform checker can run on
it. Only the originals are produced. No study output, local baseline or
optimizer candidate is stored here.

Usage:

    python3 generate.py --check            # regenerate in memory and compare
    python3 generate.py --write            # rewrite originals/ and manifest.json
    python3 generate.py --extract CORPUS   # rebuild sources.json from the
                                           # study's corpus.json (digest checked)

Lowering: `or(a, b)` becomes `Not(And(Not(a), Not(b)))`, `ite` becomes
`Select`, and every instruction is hash-consed in first-visit order over all
outputs, which reproduces the study's stored raw programs byte for byte. Each
lowered program is then evaluated eagerly on every input tuple and compared
with the study's truth signature (bit `i` of the mask supplies `vi`).
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
SOURCES = HERE / "sources.json"
ORIGINALS = HERE / "originals"
MANIFEST = HERE / "manifest.json"
STUDY_CORPUS_SHA256 = "f0ad16732a453ef6c176e48aa46e544ad07015f4793bf32c84459a271ee9bc18"
SCHEMA = "zeno-fcis/finite-i64/1"
ARITY = {"and": 2, "or": 2, "not": 1, "ite": 3}
# Opcode tags of the canonical program encoding.
INPUT, BOOL, AND, NOT, SELECT = 0, 2, 7, 8, 9


def parse(source: str):
    """The study's s-expression language: v0..v5, true, false, and, or, not, ite."""
    tokens = source.replace("(", " ( ").replace(")", " ) ").split()
    position = 0

    def term():
        nonlocal position
        token = tokens[position]
        position += 1
        if token == "(":
            op = tokens[position]
            position += 1
            if op not in ARITY:
                raise ValueError(f"unknown operator {op!r}")
            children = [term() for _ in range(ARITY[op])]
            if tokens[position] != ")":
                raise ValueError("wrong arity or missing close")
            position += 1
            return (op, *children)
        if token in ("true", "false") or (token[:1] == "v" and token[1:].isdigit()):
            return token
        raise ValueError(f"unknown leaf {token!r}")

    expression = term()
    if position != len(tokens):
        raise ValueError("trailing tokens")
    return expression


def lower(outputs, nvars):
    """Ordered hash-consing in the Input/Bool/And/Not/Select basis."""
    nodes, interned = [], {}

    def intern(key):
        if key not in interned:
            interned[key] = len(nodes)
            nodes.append(key)
        return interned[key]

    def visit(expression):
        if isinstance(expression, str):
            if expression in ("true", "false"):
                return intern((BOOL, int(expression == "true")))
            index = int(expression[1:])
            if index >= nvars:
                raise ValueError(f"{expression} outside {nvars} inputs")
            return intern((INPUT, index))
        children = [visit(child) for child in expression[1:]]
        op = expression[0]
        if op == "or":
            left = intern((NOT, children[0]))
            right = intern((NOT, children[1]))
            return intern((NOT, intern((AND, left, right))))
        if op == "ite":
            return intern((SELECT, *children))
        return intern(({"and": AND, "not": NOT}[op], *children))

    roots = [visit(parse(text)) for text in outputs]
    return nodes, roots


def evaluate(nodes, roots, values):
    """Eager evaluation; every instruction is total in this fragment."""
    results = []
    for node in nodes:
        tag = node[0]
        if tag == INPUT:
            results.append(values[node[1]])
        elif tag == BOOL:
            results.append(node[1])
        elif tag == AND:
            results.append(results[node[1]] & results[node[2]])
        elif tag == NOT:
            results.append(1 - results[node[1]])
        elif tag == SELECT:
            results.append(results[node[2]] if results[node[1]] else results[node[3]])
        else:
            raise ValueError(f"unexpected tag {tag}")
    return [results[root] for root in roots]


def encode(value) -> bytes:
    """Canonical value encoding: lists, strings, raw tag bytes, signed and
    unsigned 128-bit integers."""
    if isinstance(value, list):
        return b"\x08" + len(value).to_bytes(4, "big") + b"".join(encode(item) for item in value)
    if isinstance(value, str):
        data = value.encode()
        return b"\x06" + len(data).to_bytes(4, "big") + data
    if isinstance(value, tuple) and value[0] == "tag":
        return bytes([value[1]])
    if isinstance(value, tuple) and value[0] == "unsigned":
        return b"\x03" + value[1].to_bytes(16, "big", signed=False)
    if isinstance(value, int):
        return b"\x04" + value.to_bytes(16, "big", signed=True)
    raise TypeError(value)


def program_bytes(nodes, roots, nvars) -> bytes:
    boolean = [("tag", 2), 0, 1]
    return encode(
        [
            SCHEMA,
            [[boolean] * nvars, [boolean] * len(roots)],
            [list(node) for node in nodes],
            [("unsigned", root) for root in roots],
        ]
    )


def build(sources):
    """Every case's canonical bytes and manifest entry, checked against its
    truth signatures."""
    built = []
    for case in sources["cases"]:
        nvars, outputs = case["nvars"], case["outputs"]
        nodes, roots = lower(outputs, nvars)
        for mask in range(1 << nvars):
            values = [(mask >> index) & 1 for index in range(nvars)]
            got = evaluate(nodes, roots, values)
            want = [int(signature[mask]) for signature in case["truth_signatures"]]
            if got != want:
                raise SystemExit(f"{case['id']}: lowering disagrees with its truth signature at mask {mask}")
        data = program_bytes(nodes, roots, nvars)
        built.append(
            (
                case["id"],
                data,
                {
                    "family": case["family"],
                    "inputs": nvars,
                    "outputs": len(roots),
                    "nodes": len(nodes),
                    "bytes": len(data),
                    "sha256": hashlib.sha256(data).hexdigest(),
                },
            )
        )
    return built


def manifest(built, sources):
    return {
        "schema": "zeno-fcis/published-corpus-manifest/1",
        "authority": "none",
        "use": "calibration only",
        "source": sources["source"],
        "cases": {case_id: entry for case_id, _, entry in built},
        "totals": {
            "cases": len(built),
            "nodes": sum(entry["nodes"] for _, _, entry in built),
        },
    }


def dump(value) -> str:
    return json.dumps(value, indent=2, sort_keys=True) + "\n"


def extract(path: Path):
    raw = path.read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    if digest != STUDY_CORPUS_SHA256:
        raise SystemExit(f"{path}: SHA-256 {digest}, expected {STUDY_CORPUS_SHA256}")
    study = json.loads(raw)
    keep = ("id", "family", "nvars", "outputs", "truth_signatures")
    sources = {
        "schema": "zeno-fcis/published-corpus-sources/1",
        "source": {
            "repository": "https://github.com/TheDarkLightX/ZenoFCIS",
            "branch": "research/semantic-egraphs",
            "commit": "a69ed8db594d95279a46bff0f65185ef67d51f98",
            "path": "experiments/semantic-egraphs/corpus.json",
            "sha256": STUDY_CORPUS_SHA256,
            "seed": study["seed"],
            "kept_fields": list(keep),
        },
        "cases": [{key: case[key] for key in keep} for case in study["cases"]],
    }
    SOURCES.write_text(dump(sources))
    print(f"wrote {SOURCES.name}: {len(sources['cases'])} cases")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--extract", type=Path, metavar="CORPUS_JSON")
    arguments = parser.parse_args()
    if arguments.extract:
        extract(arguments.extract)
        return 0
    sources = json.loads(SOURCES.read_text())
    built = build(sources)
    expected = dump(manifest(built, sources))
    if arguments.write:
        ORIGINALS.mkdir(exist_ok=True)
        for case_id, data, _ in built:
            (ORIGINALS / f"{case_id}.zcve").write_bytes(data)
        MANIFEST.write_text(expected)
        print(f"wrote {len(built)} originals and {MANIFEST.name}")
        return 0
    problems = []
    if MANIFEST.read_text() != expected:
        problems.append(f"{MANIFEST.name} differs from the regeneration")
    present = {path.name for path in ORIGINALS.glob("*.zcve")}
    wanted = {f"{case_id}.zcve" for case_id, _, _ in built}
    problems += [f"unexpected file {name}" for name in sorted(present - wanted)]
    for case_id, data, _ in built:
        path = ORIGINALS / f"{case_id}.zcve"
        if not path.exists() or path.read_bytes() != data:
            problems.append(f"{path.name} differs from the regeneration")
    for problem in problems:
        print(problem, file=sys.stderr)
    if not problems:
        print(f"{len(built)} originals match their sources and truth signatures")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
