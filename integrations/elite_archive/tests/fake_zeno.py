#!/usr/bin/env python3
"""ONLY a fake test double for the Python adapter; NEVER use as a proof checker."""
import hashlib
import itertools
import json
import os
from pathlib import Path
import sys


def hash_bytes(data):
    return hashlib.sha256(data).hexdigest()


def canon(obj):
    return (json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False) + "\n").encode()


def report(status, detail, code=0, schema="zeno-fcis/transform-result/1", **extra):
    print(json.dumps({"schema": schema, "status": status, "authority": "none", "detail": detail, **extra}))
    raise SystemExit(code)


def arg(name):
    args = sys.argv[1:]
    return args[args.index(name) + 1]


def evaluate(program, x):
    nodes = program["nodes"]
    values = []
    for node in nodes:
        if node[0] == "Input":
            values.append(x[node[1]])
        elif node[0] == "Not":
            values.append(1 - values[node[1]])
        else:
            raise ValueError("unhandled fake opcode")
    return [values[ref] for ref in program["roots"]]


def equiv(original, candidate, cap):
    if original["inputs"] != candidate["inputs"] or original["outputs"] != candidate["outputs"]:
        return "refused", None
    if not all(d["kind"] == "Bool" for d in original["inputs"]):
        return "refused", None
    n = 1 << len(original["inputs"])
    if n > cap:
        return "inconclusive", None
    for x in itertools.product((0, 1), repeat=len(original["inputs"])):
        if evaluate(original, x) != evaluate(candidate, x):
            return "counterexample", {"input": x}
    return "equivalent", n


def check(original_path, candidate_path, cap, receipt_path=None):
    a = original_path.read_bytes()
    b = candidate_path.read_bytes()
    first = json.loads(a)
    second = json.loads(b)
    status, detail = equiv(first, second, cap)
    if status != "equivalent":
        report(status, detail or {}, 2)
    rec = {
        "schema": "zeno-fcis/transform-receipt/1",
        "verdict": "equivalent",
        "authority": "none",
        "original": {"sha256": hash_bytes(a), "nodes": len(first["nodes"]), "bytes": len(a)},
        "candidate": {"sha256": hash_bytes(b), "nodes": len(second["nodes"]), "bytes": len(b)},
        "limits": {"steps": 256, "input_tuples": cap},
        "domain": {"size": detail, "enumeration": "ordered-product-last-input-fastest-v1"},
        "checker": {"semantics": "zeno-fcis/transform-check/1", "evaluator_identity": "a" * 64},
        "inputs_checked": detail,
        "usage": {"max_steps": {"original": len(first["nodes"]),
                                "candidate": len(second["nodes"])},
                  "usage_preserved": len(first["nodes"]) == len(second["nodes"]),
                  "candidate_uses_more": len(second["nodes"]) > len(first["nodes"])},
    }
    raw = canon(rec)
    if receipt_path is not None and os.environ.get("FAKE_SKIP_RECEIPT") != "1":
        receipt_path.write_bytes(raw)
    report("equivalent", {"receipt": rec, "receipt_sha256": hash_bytes(raw)},
           original_sha256=hash_bytes(a), candidate_sha256=hash_bytes(b))


def replay(original_path, candidate_path, cap, receipt_path):
    if os.environ.get("FAKE_REPLAY_FAIL") == "1":
        report("replay-mismatch", {"reason": "forced-failure"}, 1)
    recorded = receipt_path.read_bytes()
    before = json.loads(recorded)
    original = json.loads(original_path.read_bytes())
    candidate = json.loads(candidate_path.read_bytes())
    status, checked = equiv(original, candidate, cap)
    if status != "equivalent" or before["original"]["sha256"] != hash_bytes(original_path.read_bytes()) or before["candidate"]["sha256"] != hash_bytes(candidate_path.read_bytes()):
        report("replay-mismatch", {}, 1)
    if before["inputs_checked"] != checked:
        report("replay-mismatch", {}, 1)
    report("replayed", {"receipt_sha256": hash_bytes(recorded)})


def optimize():
    original = json.loads(Path(arg("--program")).read_bytes())
    cap = int(arg("--max-input-tuples"))
    if len(original["nodes"]) <= 1 or (1 << len(original["inputs"])) > cap:
        report("no-checked-improvement", {}, 2, schema="zeno-fcis/optimize-result/1")
    proposal = dict(original)
    proposal["nodes"] = [["Input", 0]]
    proposal["roots"] = [0]
    if equiv(original, proposal, cap)[0] != "equivalent":
        report("no-checked-improvement", {}, 2, schema="zeno-fcis/optimize-result/1")
    Path(arg("--candidate-out")).write_bytes(canon(proposal))
    report("improved", {}, schema="zeno-fcis/optimize-result/1")


def main():
    if sys.argv[1:3] == ["transform", "check"]:
        check(Path(arg("--original")), Path(arg("--candidate")), int(arg("--max-input-tuples")), Path(arg("--receipt")))
    elif sys.argv[1:3] == ["transform", "replay"]:
        replay(Path(arg("--original")), Path(arg("--candidate")), int(arg("--max-input-tuples")), Path(arg("--receipt")))
    elif sys.argv[1] == "optimize":
        optimize()
    else:
        report("refused", {}, 1)


if __name__ == "__main__":
    main()
