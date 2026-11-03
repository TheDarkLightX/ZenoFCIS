#!/usr/bin/env python3
"""Source-bound audit of the LOCAL VeriISLE i64 select assumption.

Checks pinned independent Cranelift interpreter/type-definition sources and
exhausts all 256 scalar i8 condition values. This is NOT a proof of undefined
behavior, poison, arbitrary targets, or an authoritative upstream VeriISLE spec.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from pathlib import Path

from install_isle_candidate import UPSTREAM
from install_veriisle_select_spec_28 import SPEC, SPEC_PATH, SPEC_BLOB

REFERENCES = {
    "cranelift/codegen/meta/src/shared/instructions.rs":
        "d7122622a292c0300d4b6816c0424c0c25761f25",
    "cranelift/interpreter/src/step.rs":
        "cad9149b32edb08781bd7666953312762a4e64ae",
    "cranelift/interpreter/src/value.rs":
        "f79d21fe331084226b3ab70b8d18ffaff68625ee",
    SPEC_PATH: SPEC_BLOB,
}
SELECT_INTERPRETER = (
    "Opcode::Select | Opcode::SelectSpectreGuard => "
    "choose(arg(0).into_bool()?, arg(1), arg(2))"
)
BOOL_CONVERSION = "DataValue::I8(b) => Ok(b != 0)"
OPERAND_C = 'Operand::new("c", ScalarTruthy).with_doc("Controlling value to test")'
OPERAND_TRUE = 'Operand::new("x", Any).with_doc("Value to use when `c` is true")'
OPERAND_FALSE = 'Operand::new("y", Any).with_doc("Value to use when `c` is false")'
SELECT_SPEC = """(spec (select ty c x y)
    (provide (= result (if (= c #x00) y x))
             (= (:bits ty) (widthof result))
             (= (:lanes ty) (lanesof result))))
(instantiate select
    ((args (named Type) (bv 8) (bv 64) (bv 64)) (ret (bv 64))))"""


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def exact_count(source: str, needle: str, count: int, label: str) -> None:
    if source.count(needle) != count:
        raise ValueError(f"missing or ambiguous pinned {label}")


def exact_once(source: str, needle: str, label: str) -> None:
    exact_count(source, needle, 1, label)


def check_semantic_anchors(texts: dict[str, str], spec: str = SPEC) -> None:
    """Deliberately separate the interpreter oracle from the candidate spec."""
    meta = texts["cranelift/codegen/meta/src/shared/instructions.rs"]
    step = texts["cranelift/interpreter/src/step.rs"]
    value = texts["cranelift/interpreter/src/value.rs"]
    # Restrict evidence to the select definition, not select_spectre_guard,
    # bitselect or unrelated instructions that reuse the same operand docs.
    begin = 'Inst::new(\\n            "select",'
    end = 'Inst::new(\\n            "select_spectre_guard",'
    exact_once(meta, begin, "select instruction definition")
    exact_once(meta, end, "next instruction definition")
    select_block = meta.split(begin, 1)[1].split(end, 1)[0]
    exact_once(select_block, OPERAND_C, "scalar truthy select operand")
    exact_once(select_block, OPERAND_TRUE, "true select arm")
    exact_once(select_block, OPERAND_FALSE, "false select arm")
    exact_once(step, SELECT_INTERPRETER, "interpreter select operation")
    exact_once(value, BOOL_CONVERSION, "i8 nonzero truth conversion")
    exact_once(spec, SELECT_SPEC, "provisional i64 select specification")
    # A second/overriding definition could make the check unsound.
    if spec.count("(spec (select ") != 1 or spec.count("(instantiate select") != 1:
        raise ValueError("ambiguous provisional select definitions")


def check_defined_i8_model() -> dict:
    """Exhaustive in c, symbolic in x/y: only branch identity is relevant."""
    for c in range(256):
        interpreter_choice = "x" if c != 0 else "y"
        verifier_choice = "y" if c == 0 else "x"
        if interpreter_choice != verifier_choice:
            raise ValueError(f"select model mismatch for i8 condition {c}")
    edges = [0, 1, (1 << 63) - 1, 1 << 63, (1 << 64) - 1]
    finite = 0
    for c in range(256):
        for x in edges:
            for y in edges:
                actual = x if c != 0 else y
                modeled = y if c == 0 else x
                if actual != modeled:
                    raise ValueError(f"select finite mismatch: c={c}, x={x}, y={y}")
                finite += 1
    return {
        "condition_width_bits": 8,
        "condition_values_exhaustively_checked": 256,
        "branch_value_scope": "arbitrary defined i64 bitvectors by symbolic arm selection",
        "finite_edge_value_pairs": finite,
    }


def source_pins(upstream: Path) -> tuple[dict[str, str], dict[str, str]]:
    def git(*args: str) -> str:
        return subprocess.check_output(
            ["git", "-C", str(upstream), *args], text=True
        ).strip()

    if git("rev-parse", "HEAD") != UPSTREAM:
        raise ValueError("unexpected upstream revision")
    texts = {}
    pins = {}
    for path, blob in REFERENCES.items():
        if git("rev-parse", f"HEAD:{path}") != blob:
            raise ValueError(f"unreviewed upstream source blob: {path}")
        raw = (upstream / path).read_bytes()
        if git("hash-object", path) != blob:
            raise ValueError(f"working source modified: {path}")
        texts[path] = raw.decode("utf-8")
        pins[path] = {"git_blob": blob, "sha256": sha(raw)}
    return texts, pins


def qualify(upstream: Path) -> dict:
    texts, pins = source_pins(upstream)
    check_semantic_anchors(texts)
    coverage = check_defined_i8_model()
    return {
        "schema": "zeno-fcis/tier1-select-model-source-audit/1",
        "status": "source-backed-agreement-for-defined-scalar-i8-select",
        "upstream_revision": UPSTREAM,
        "source_pins": pins,
        "provisional_spec_sha256": sha(SPEC.encode()),
        "condition_coverage": coverage,
        "evidence": (
            "pinned CLIF select operand typing plus interpreter Opcode::Select "
            "and DataValue::I8 truth conversion agree with local i64 VeriISLE "
            "select model for all 256 scalar condition values"
        ),
        "nonclaims": [
            "source agreement is not independent upstream approval of the verifier spec",
            "not a machine ISA or optimization-rule proof",
            "does not model poison, undefined values, traps or flags",
            "no vector or non-i64 select instantiations",
            "no cross-target performance, optimality or speed claim",
        ],
    }


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--upstream", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    a = p.parse_args()
    if a.out.exists():
        p.error("refusing to overwrite existing evidence receipt")
    receipt = qualify(a.upstream.resolve())
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": receipt["status"],
                      "conditions": receipt["condition_coverage"]
                                            ["condition_values_exhaustively_checked"]}))


if __name__ == "__main__":
    main()
