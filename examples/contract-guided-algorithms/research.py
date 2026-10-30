"""Bounded, contract-guided algorithm research pilot (NOT production Authority).

Python 3.11+ standard library only. All candidates are inert expression trees;
independent finite-domain verification lives in reference.py. The reported
cost is *abstract expression-node count*, never native code size or speed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile

import reference

SCHEMA = "zeno-fcis/algorithm-research-corpus/1"
CATALOG_SCHEMA = "zeno-fcis/algorithm-research-catalog/1"
PROFILE = "u8-mod-word/1"
OPS = ("and", "or", "xor", "add", "sub", "not", "lshr", "shl")
COMMUTATIVE = frozenset(("and", "or", "xor", "add"))
REFS = frozenset((
    "lowest-set-bit-u8", "clear-lowest-set-bit-u8",
    "divide-by-eight-u8", "modulo-eight-u8", "gray-code-u8", "rotate-left-one-u8",
))
PROBES = (0, 1, 2, 3, 7, 8, 15, 16, 31, 63, 127, 128, 254, 255)
MAX_CASES = 16
MAX_NODES = 7
MAX_BUDGET = 100000
MAX_GRAMMAR_EXPRESSIONS = 200000


class Refusal(ValueError):
    """The artifact is malformed, unsupported, or does not match its binding."""


def required(condition: bool, message: str) -> None:
    if not condition:
        raise Refusal(message)


def is_int(value: object, minimum: int, maximum: int) -> bool:
    return type(value) is int and minimum <= value <= maximum


def object_no_duplicates(pairs: list[tuple[str, object]]) -> dict:
    out = {}
    for key, val in pairs:
        required(key not in out, "duplicate JSON key")
        out[key] = val
    return out


def read_json(path: Path) -> dict:
    try:
        obj = json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=object_no_duplicates,
                         parse_constant=lambda _: (_ for _ in ()).throw(Refusal("non-finite JSON number")))
    except (OSError, ValueError, UnicodeError) as exc:
        raise Refusal(f"cannot parse {path}: {exc}") from exc
    required(type(obj) is dict, "root must be an object")
    return obj


def canonical(obj: object) -> bytes:
    return json.dumps(obj, sort_keys=True, ensure_ascii=True,
                      separators=(",", ":"), allow_nan=False).encode("ascii")


def digest(obj: object) -> str:
    return hashlib.sha256(canonical(obj)).hexdigest()


def file_hash(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def exact_keys(obj: object, keys: set[str], context: str) -> dict:
    required(type(obj) is dict and set(obj) == keys, f"{context}: missing or extra field")
    return obj


def validate_expr(expr: object, max_nodes: int = MAX_NODES) -> int:
    """Validate a closed, acyclic tree encoded by JSON; return node count."""
    required(type(expr) is list and len(expr) > 0 and type(expr[0]) is str,
             "expression must be nonempty operation array")
    op = expr[0]
    if op == "input":
        required(expr == ["input", "x"], "only declared input x is supported")
        return 1
    if op == "const":
        required(len(expr) == 2 and is_int(expr[1], 0, 255), "constant must be u8")
        return 1
    required(op in OPS, "unsupported operation")
    if op in ("shl", "lshr"):
        required(len(expr) == 3 and is_int(expr[2], 0, 7), "shift must be 0..7")
        n = 1 + validate_expr(expr[1], max_nodes)
    elif op == "not":
        required(len(expr) == 2, "not requires one argument")
        n = 1 + validate_expr(expr[1], max_nodes)
    else:
        required(len(expr) == 3, "binary operation requires two arguments")
        n = 1 + validate_expr(expr[1], max_nodes) + validate_expr(expr[2], max_nodes)
    required(n <= max_nodes, "expression exceeds node bound")
    return n


def execute(expr: list, x: int) -> int:
    """Candidate interpreter; distinct source from mathematical reference."""
    op = expr[0]
    if op == "input":
        return x
    if op == "const":
        return expr[1]
    if op == "not":
        return (~execute(expr[1], x)) & 255
    if op in ("shl", "lshr"):
        a = execute(expr[1], x)
        return ((a << expr[2]) & 255) if op == "shl" else (a >> expr[2])
    a, b = execute(expr[1], x), execute(expr[2], x)
    if op == "and":
        return a & b
    if op == "or":
        return a | b
    if op == "xor":
        return a ^ b
    if op == "add":
        return (a + b) & 255
    if op == "sub":
        return (a - b) & 255
    raise Refusal("unrecognized operation")


def validate_problem(item: object) -> dict:
    exact_keys(item, {"id", "reference", "profile", "palettes", "baseline"}, "problem")
    required(type(item["id"]) is str and item["id"] and len(item["id"]) <= 80,
             "invalid problem id")
    required(type(item["reference"]) is str and item["reference"] in REFS and item["profile"] == PROFILE,
             "unsupported reference or semantic profile")
    required(type(item["palettes"]) is list and 0 < len(item["palettes"]) <= 8,
             "invalid palettes")
    names = set()
    for palette in item["palettes"]:
        exact_keys(palette, {"name", "operators", "constants", "shifts", "max_nodes"}, "palette")
        name = palette["name"]
        required(type(name) is str and name and name not in names, "duplicate/invalid palette")
        names.add(name)
        ops, consts, shifts = palette["operators"], palette["constants"], palette["shifts"]
        required(type(ops) is list and len(ops) <= len(OPS)
                 and all(type(op) is str and op in OPS for op in ops) and len(set(ops)) == len(ops),
                 "unknown/duplicate palette operation")
        required(type(consts) is list and len(consts) <= 8
                 and all(is_int(v, 0, 255) for v in consts) and len(set(consts)) == len(consts),
                 "invalid/duplicate palette constant")
        required(type(shifts) is list and len(shifts) <= 8
                 and all(is_int(v, 0, 7) for v in shifts) and len(set(shifts)) == len(shifts),
                 "invalid/duplicate shift")
        required(is_int(palette["max_nodes"], 1, MAX_NODES), "invalid size bound")
    validate_expr(item["baseline"])
    verify(item, item["baseline"])
    return item


def validate_corpus(obj: dict) -> list[dict]:
    exact_keys(obj, {"schema", "problems", "source_notes"}, "corpus")
    required(obj["schema"] == SCHEMA, "unsupported corpus schema")
    required(type(obj["source_notes"]) is str and len(obj["source_notes"]) < 1000,
             "source notes must be short")
    required(type(obj["problems"]) is list and 0 < len(obj["problems"]) <= MAX_CASES,
             "invalid problem count")
    ids = set()
    for item in obj["problems"]:
        validate_problem(item)
        required(item["id"] not in ids, "duplicate problem id")
        ids.add(item["id"])
    return obj["problems"]


def verify(problem: dict, expr: list) -> dict:
    """Full finite comparison; mismatch returns a named input, never success."""
    n = validate_expr(expr)
    for x in range(256):
        observed = execute(expr, x)
        expected = reference.result(problem["reference"], x)
        if observed != expected:
            raise Refusal(f"semantic counterexample: x={x}, actual={observed}, expected={expected}")
    return {"method": "exhaustive-enumeration", "declared_domain": "all-u8-inputs",
            "cases_checked": 256, "status": "verified-within-profile", "nodes": n}


def grammar_upper_bound(palette: dict) -> int:
    """Conservative preflight against combinatorial materialization."""
    counts = {1: 1 + len(palette["constants"])}
    total = counts[1]
    for n in range(2, palette["max_nodes"] + 1):
        count = 0
        for op in palette["operators"]:
            if op == "not":
                count += counts.get(n - 1, 0)
            elif op in ("shl", "lshr"):
                count += counts.get(n - 1, 0) * len(palette["shifts"])
            else:
                count += sum(counts.get(i, 0) * counts.get(n - 1 - i, 0)
                             for i in range(1, n - 1))
        counts[n] = count
        total += count
        if total > MAX_GRAMMAR_EXPRESSIONS:
            return total
    return total


def palette_expressions(palette: dict):
    """Enumerate expressions in increasing tree size, with a fixed order."""
    max_nodes = palette["max_nodes"]
    by_size: dict[int, list[list]] = {1: [["input", "x"]] +
             [["const", n] for n in sorted(palette["constants"])]}
    for n in range(1, max_nodes + 1):
        if n > 1:
            formed = []
            for op in sorted(palette["operators"]):
                if op == "not":
                    for a in by_size.get(n-1, []):
                        formed.append([op, a])
                elif op in ("shl", "lshr"):
                    for a in by_size.get(n-1, []):
                        for shift in sorted(palette["shifts"]):
                            formed.append([op, a, shift])
                else:
                    for left_size in range(1, n-1):
                        right_size = n-1-left_size
                        for a in by_size.get(left_size, []):
                            for b in by_size.get(right_size, []):
                                if op in COMMUTATIVE and canonical(a) > canonical(b):
                                    continue
                                formed.append([op, a, b])
            by_size[n] = formed
        yield from by_size[n]


def search(problem: dict, palette: dict, max_candidates: int) -> dict:
    required(is_int(max_candidates, 1, MAX_BUDGET), "invalid candidate budget")
    # Do not construct enormous intermediate levels before enforcing work caps.
    if grammar_upper_bound(palette) > MAX_GRAMMAR_EXPRESSIONS:
        return {"status": "inconclusive-grammar-size", "candidates_inspected": 0,
                "probe_survivors": 0, "candidate": None}
    inspected = 0
    sample_survivors = 0
    for expr in palette_expressions(palette):
        if inspected >= max_candidates:
            return {"status": "inconclusive-budget", "candidates_inspected": inspected,
                    "probe_survivors": sample_survivors, "candidate": None}
        inspected += 1
        if not all(execute(expr, x) == reference.result(problem["reference"], x)
                   for x in PROBES):
            continue
        sample_survivors += 1
        try:
            receipt = verify(problem, expr)
        except Refusal:
            continue
        return {"status": "verified-within-profile", "candidates_inspected": inspected,
                "probe_survivors": sample_survivors, "candidate": expr, "verification": receipt}
    return {"status": "none-within-grammar", "candidates_inspected": inspected,
            "probe_survivors": sample_survivors, "candidate": None}


def tool_sources() -> dict:
    root = Path(__file__).resolve().parent
    return {"research.py": file_hash(root / "research.py"),
            "reference.py": file_hash(root / "reference.py")}


def run(corpus: dict, max_candidates: int) -> dict:
    problems = validate_corpus(corpus)
    entries = []
    experiments = []
    for problem in problems:
        contract_hash = digest({k:v for k,v in problem.items() if k != "palettes"})
        baseline_verification = verify(problem, problem["baseline"])
        baseline_nodes = baseline_verification["nodes"]
        for palette in problem["palettes"]:
            result = search(problem, palette, max_candidates)
            record = {"problem": problem["id"], "contract_sha256": contract_hash,
                      "palette_name": palette["name"], "palette_sha256": digest(palette),
                      "status": result["status"],
                      "candidates_inspected": result["candidates_inspected"],
                      "probe_survivors": result["probe_survivors"],
                      "candidate_budget": max_candidates}
            experiments.append(record)
            if result["candidate"] is None:
                continue
            candidate = result["candidate"]
            entry = {"contract_sha256": contract_hash, "problem": problem["id"],
                     "profile": PROFILE, "expression": candidate,
                     "expression_sha256": digest(candidate),
                     "cost": {"metric": "abstract-tree-nodes", "candidate": result["verification"]["nodes"],
                              "baseline": baseline_nodes},
                     "verification": {k: v for k, v in result["verification"].items() if k != "nodes"},
                     "experiment_palette": palette["name"], "palette_sha256": digest(palette)}
            # The same contract/semantic candidate can be proposed by many palettes.
            if not any(e["contract_sha256"] == contract_hash and
                       e["expression_sha256"] == entry["expression_sha256"] for e in entries):
                entries.append(entry)
    return {"schema": CATALOG_SCHEMA, "corpus_sha256": digest(corpus),
            "tool_source_sha256": tool_sources(), "cost_nonclaims": [
                "Abstract tree nodes are NOT machine bytes, latency, gas or CLIF cost",
                "Finite u8 verification is NOT a proof for another width or ABI",
                "Search success is NOT global optimality outside the declared grammar"
            ], "entries": entries, "experiments": experiments}


def replay(corpus: dict, catalog: dict) -> int:
    problems = {p["id"]: p for p in validate_corpus(corpus)}
    exact_keys(catalog, {"schema", "corpus_sha256", "tool_source_sha256",
                         "cost_nonclaims", "entries", "experiments"}, "catalog")
    required(catalog["schema"] == CATALOG_SCHEMA and catalog["corpus_sha256"] == digest(corpus),
             "catalog corpus identity drift")
    required(catalog["tool_source_sha256"] == tool_sources(), "checker/interpreter identity drift")
    required(type(catalog["entries"]) is list and type(catalog["experiments"]) is list,
             "invalid catalog arrays")
    known = set()
    expected_experiments = {(p["id"], pa["name"]) for p in problems.values()
                            for pa in p["palettes"]}
    observed_experiments = {(e.get("problem"), e.get("palette_name"))
                            for e in catalog["experiments"] if type(e) is dict}
    required(len(catalog["experiments"]) == len(expected_experiments) and
             observed_experiments == expected_experiments,
             "missing/duplicate/unregistered experiments")
    for entry in catalog["entries"]:
        exact_keys(entry, {"contract_sha256", "problem", "profile", "expression",
                           "expression_sha256", "cost", "verification", "experiment_palette",
                           "palette_sha256"}, "entry")
        required(entry["problem"] in problems, "unknown problem")
        problem = problems[entry["problem"]]
        contract_hash = digest({k:v for k,v in problem.items() if k != "palettes"})
        required(entry["contract_sha256"] == contract_hash and entry["profile"] == PROFILE,
                 "contract/profile mismatch")
        required(entry["expression_sha256"] == digest(entry["expression"]),
                 "candidate bytes/hash mismatch")
        palette_lookup = {p["name"]: p for p in problem["palettes"]}
        required(entry["experiment_palette"] in palette_lookup, "unregistered palette")
        palette = palette_lookup[entry["experiment_palette"]]
        exact_keys(entry["cost"], {"metric", "candidate", "baseline"}, "entry cost")
        exact_keys(entry["verification"], {"method", "declared_domain", "cases_checked",
                                           "status"}, "entry verification")
        required(is_int(entry["cost"]["candidate"], 1, MAX_NODES), "invalid candidate node cost")
        required(entry["palette_sha256"] == digest(palette), "palette binding drift")
        required(entry["cost"]["candidate"] <= palette["max_nodes"],
                 "candidate outside palette size bound")
        v = verify(problem, entry["expression"])
        baseline_nodes = validate_expr(problem["baseline"])
        required(entry["cost"] == {"metric": "abstract-tree-nodes", "candidate": v["nodes"],
                                    "baseline": baseline_nodes}, "cost evidence drift")
        required(entry["verification"] == {k:x for k,x in v.items() if k != "nodes"},
                 "verification scope drift")
        identity = (contract_hash, entry["expression_sha256"])
        required(identity not in known, "duplicate catalog candidate")
        known.add(identity)
    # Replay the declared experiment protocol too; not just a self-hashed receipt.
    required(is_int(len(catalog["experiments"]), 1, MAX_CASES * 8), "invalid experiment count")
    for record in catalog["experiments"]:
        exact_keys(record, {"problem", "contract_sha256", "palette_name", "palette_sha256",
                            "status", "candidates_inspected", "probe_survivors", "candidate_budget"},
                   "experiment")
        required(record["problem"] in problems, "unknown experiment problem")
        p = problems[record["problem"]]
        matches = [pa for pa in p["palettes"] if pa["name"] == record["palette_name"]]
        required(len(matches) == 1 and record["palette_sha256"] == digest(matches[0]),
                 "unknown experiment palette")
        required(record["contract_sha256"] == digest({k:v for k,v in p.items() if k != "palettes"}),
                 "experiment contract mismatch")
        expected = search(p, matches[0], record["candidate_budget"])
        required(record["status"] == expected["status"] and
                 record["candidates_inspected"] == expected["candidates_inspected"] and
                 record["probe_survivors"] == expected["probe_survivors"],
                 "experiment replay mismatch")
        if expected["candidate"] is not None:
            required(any(e["problem"] == record["problem"] and
                         e["expression_sha256"] == digest(expected["candidate"])
                         for e in catalog["entries"]), "verified experiment candidate missing")
    # No extra entries may be promoted merely because they verify semantically:
    # every retained implementation must be a winning, reproduced search result.
    winning = set()
    for record in catalog["experiments"]:
        p = problems[record["problem"]]
        palette = next(pa for pa in p["palettes"] if pa["name"] == record["palette_name"])
        result = search(p, palette, record["candidate_budget"])
        if result["candidate"] is not None:
            winning.add((record["contract_sha256"], digest(result["candidate"])))
    required(known == winning, "catalog entries not equal to replayed winners")
    return len(known)


def atomic_write(path: Path, obj: dict) -> None:
    required(path.parent.is_dir() and not path.exists(), "output must be new in existing directory")
    data = json.dumps(obj, sort_keys=True, indent=2, ensure_ascii=True) + "\n"
    fd, tmp = tempfile.mkstemp(prefix=".algorithm-research-", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(tmp, path)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("run", help="Search reviewed palettes; create immutable evidence catalog")
    p.add_argument("--corpus", type=Path, default=Path(__file__).with_name("corpus.json"))
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--max-candidates", type=int, default=10000)
    p = sub.add_parser("replay", help="Re-run independent full-domain checks and search")
    p.add_argument("--corpus", type=Path, default=Path(__file__).with_name("corpus.json"))
    p.add_argument("--catalog", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        corpus = read_json(args.corpus)
        if args.command == "run":
            artifact = run(corpus, args.max_candidates)
            atomic_write(args.out, artifact)
            print(json.dumps({"status": "written", "entries": len(artifact["entries"]),
                              "experiments": artifact["experiments"]}, sort_keys=True))
        else:
            count = replay(corpus, read_json(args.catalog))
            print(json.dumps({"status": "replayed", "verified_entries": count}, sort_keys=True))
        return 0
    except (Refusal, OSError) as exc:
        print(json.dumps({"status": "refused", "reason": str(exc)}, sort_keys=True), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
