#!/usr/bin/env python3
"""Produce four finite ladder examples from two recurring contract requirements.

These are declared observation projections, not a contract parser or checker.
The tests bind each projection to its source law and compare finite valuations.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = Path(__file__).with_name("examples") / "patterns"


def source(path, law):
    raw = (ROOT / path).read_bytes()
    text = re.sub(r"//[^\n]*", "", raw.decode())
    match = re.search(rf"law {law} (\w+) on ([^=]+)=\s*(.*?);", text, re.S)
    if match is None:
        raise ValueError(f"source law missing: {path}:{law}")
    return {"path": path, "sha256": hashlib.sha256(raw).hexdigest(), "law": law,
            "name": match[1], "when": match[2].strip(), "body": " ".join(match[3].split())}


def domain(member, plural, values):
    return {"member": member, "plural": plural, "noun": plural, "integer": True, "values": values}


def function(args, noun, state=False):
    return {"kind": "function", "arity": len(args), "args": args, "noun": noun, "state": state}


def base(name, contract, law, scope):
    return {"profile": "zal/ladder/0", "theory": "steps-over-integers", "id": name, "kind": "law",
            "authority": {"status": "OPEN", "source": "Finite reading of the cited example contract; no runtime authority."},
            "scope": scope, "contract_source": source(contract, law)}


def frame_fixture(copies, chosen, before, after, commits=True, label="valuation", expected=True):
    return {"id": label, "domains": {"T": [0], "C": list(copies)}, "symbols": {
        "Committed": {"rows": [[[0], commits]]}, "chosen": {"rows": [[[0], chosen]]},
        "holder": {"rows": [[[0, c], value] for c, value in zip(copies, before, strict=True)]},
        "holder'": {"rows": [[[0, c], value] for c, value in zip(copies, after, strict=True)]}},
        "expected": expected, "note": "Only committed steps are constrained by this law; rejection framing is separate."}


def frame_example(name, path, copies):
    data = base(name, path, 502, "One committed-step observation; all declared copies; holders 0..3. "
                "The chosen copy is the command's copy field. No rule, role or rejection guarantee is added.")
    body = "forall c in C: c != chosen(t) => holder'(t, c) = holder(t, c)"
    data.update(domains={"T": domain("step", "steps", [0]), "C": domain("copy", "copies", copies)},
                symbols={"Committed": {"kind": "predicate", "arity": 1, "args": ["T"], "clause": "{0} commits"},
                         "chosen": function(["T"], "the chosen copy"),
                         "holder": function(["T", "C"], "the holder", state=True)},
                definitions=[{"name": "OnlyChosenChanges", "params": [{"name": "t", "domain": "T"}],
                              "body": body, "clause": "{0} changes only the chosen copy"}],
                formula="forall t in T: Committed(t) => (" + body + ")")
    empty = [0] * len(copies)
    chosen_change, other_change = list(empty), list(empty)
    chosen_change[0], other_change[1] = 1, 1
    data["fixtures"] = [
        frame_fixture(copies, copies[0], empty, chosen_change, label="chosen_changes", expected=True),
        frame_fixture(copies, copies[0], empty, other_change, label="other_changes", expected=False),
        frame_fixture(copies, copies[0], empty, other_change, commits=False, label="outside_commit_scope", expected=True)]
    return data


def missing_fixture(present, values, label="valuation", expected=True):
    entries = list(range(len(present)))
    return {"id": label, "domains": {"T": [0], "E": entries}, "symbols": {
        "present": {"rows": [[[0, e], v] for e, v in zip(entries, present, strict=True)]},
        "amount": {"rows": [[[0, e], v] for e, v in zip(entries, values, strict=True)]}},
        "expected": expected, "note": "A finite per-entry invariant projection, not an enumeration of all possible maps."}


def missing_example(name, path, noun, quantity, maximum):
    data = base(name, path, 501, f"One post-state snapshot at genesis or commit; two {noun} slots, "
                f"presence 0..1, {quantity} 0..{maximum}. The source law is per entry; unrelated map behavior is outside this projection.")
    body = "forall e in E: present(t, e) = 0 => amount(t, e) = 0"
    data.update(domains={"T": domain("snapshot", "snapshots", [0]), "E": domain(noun[:-1], noun, [0, 1])},
                symbols={"present": function(["T", "E"], "entry presence"),
                         "amount": function(["T", "E"], quantity)},
                definitions=[{"name": "MissingHoldNothing", "params": [{"name": "t", "domain": "T"}],
                              "body": body, "clause": f"{{0}} gives missing {noun} a zero {quantity}"}],
                formula="forall t in T: " + body,
                fixtures=[missing_fixture([0, 1], [0, maximum], "missing_is_zero", True),
                          missing_fixture([0, 1], [1, maximum], "missing_holds_one", False)])
    return data


def documents():
    examples = [frame_example("library_three_frame", "examples/apps/library-loans/project.zeno", [1, 2, 3]),
                frame_example("library_six_frame", "examples/contract-tables/library-loans/project.zeno", list(range(6))),
                missing_example("token_missing_zero", "examples/collections/token-balances/project.zeno", "accounts", "balance", 3),
                missing_example("member_missing_zero", "examples/collections/library/project.zeno", "members", "loan count", 2)]
    return {data["id"] + ".json": json.dumps(data, ensure_ascii=True, indent=2) + "\n" for data in examples}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    files = documents()
    if args.check:
        for name, text in files.items():
            if not (OUTPUT / name).is_file() or (OUTPUT / name).read_text() != text:
                raise SystemExit("generated pattern differs: " + name)
        print(f"PASS: {len(files)} contract-derived examples match their producer")
    else:
        OUTPUT.mkdir(parents=True, exist_ok=True)
        for name, text in files.items():
            (OUTPUT / name).write_text(text)
        print(f"Generated {len(files)} finite pattern examples")


if __name__ == "__main__":
    main()
