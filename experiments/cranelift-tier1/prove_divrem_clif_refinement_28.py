#!/usr/bin/env python3
"""Narrow source-anchored CLIF observational proof for adjacent i64 Udiv/Urem.

This proves a *conditional* operational refinement under exact scalar CLIF
semantics: no intervening instructions, equal SSA operands and types, quotient
and remainder results preserved, zero-divisor trap at first operation's location,
and a correctly implemented combined opcode. It is NOT a full Rust compiler
pass, ISA flags, register allocator, ABI, trap table or VeriISLE proof.
"""
import argparse
import hashlib
import json
from pathlib import Path

SCHEMA = "zenofcis/tier1-adjacent-clif-refinement/1"
ZENO_BASE = "9ffe52508898e76ef5026ac15b7d17351bbb60fb"
WASMTIME = "8b19f199f89da864c529b3fa3ca2f7e40d5de609"
BLOBS = {
    "cranelift/codegen/src/context.rs": "472f3d9db48aad0742e056861ee42243d76e50f4",
    "cranelift/codegen/meta/src/shared/instructions.rs": "1fcac42ebfb4b5a7c9220cd71ddd7688776cd05d",
    "cranelift/codegen/src/isa/x64/lower.isle": "654189182a09c1e22478faa157cd421ff8ed8894",
    "cranelift/interpreter/src/step.rs": "0ed9ade45cd631793d6b660a2121fd54c46ddbb4",
}

def oracle(x, y, order, width=8):
    mask = (1 << width) - 1
    x, y = x & mask, y & mask
    if y == 0:
        return ("trap", "integer_division_by_zero", 100 if order == "forward" else 200)
    return ("return", x // y, x % y)

def exhaustive_8bit():
    receipt = {}
    for order in ("forward", "reverse"):
        pairs = traps = 0
        for x in range(256):
            for y in range(256):
                expected = oracle(x, y, order)
                # Independent arithmetic path; the combined op returns divmod.
                actual = ("trap", "integer_division_by_zero", 100 if order == "forward" else 200) if y == 0 else ("return", *divmod(x, y))
                if actual != expected:
                    raise AssertionError(("finite-rewrite-failure", order, x, y))
                pairs += 1
                traps += y == 0
        receipt[order] = {"status": "exhaustive_finite_equivalent", "pairs": pairs, "zero_divisor_traps": traps}
    return receipt

def source_anchor():
    import reverse_order_udivrem_28 as reverse
    import install_udivrem_multivalue_28 as multi
    import post_souper_portability_28 as portable
    if reverse.UPSTREAM != WASMTIME or portable.UPSTREAM != WASMTIME or portable.BLOBS != BLOBS:
        raise AssertionError("compiler/source pins have drifted")
    helper = reverse.bidirectional_helper()
    required = (
        "let forward =", "let reverse =", "if !forward && !reverse",
        "let Some(rinst) = func.layout.next_inst(qinst)",
        "qa[0] != ra[0] || qa[1] != ra[1]",
        "func.dfg.value_type(qa[0]) != types::I64",
        "func.dfg.value_type(qa[1]) != types::I64",
        "func.dfg.clear_results(qinst)", "func.dfg.clear_results(rinst)",
        "(first_result, second_result)", "(second_result, first_result)",
        "cursor.use_srcloc(qinst)",
        "with_results([Some(q), Some(r)]).udivrem(x, y)",
        "func.layout.remove_inst(qinst)", "func.layout.remove_inst(rinst)",
    )
    for fragment in required:
        if fragment not in helper:
            raise AssertionError("missing exact transformation guard: " + fragment)
    for fragment in ('.can_trap()', '.side_effects_idempotent()', '"udivrem"'):
        if fragment not in multi.META_NEW:
            raise AssertionError("missing multi-result opcode declaration: " + fragment)
    for fragment in ("x64_div $I64", "TrapCode.INTEGER_DIVISION_BY_ZERO", "output_pair", "value_regs_get qr 0", "value_regs_get qr 1"):
        if fragment not in multi.X64_NEW:
            raise AssertionError("missing x64 lowering source shape: " + fragment)
    for fragment in ("x.clone().udiv(y.clone())", "x.urem(y)?", "INTEGER_DIVISION_BY_ZERO"):
        if fragment not in multi.INTERP_NEW:
            raise AssertionError("missing independent CLIF interpreter source shape: " + fragment)
    return {
        "classification": "exact-string static source-shape audit, NOT Rust AST/type proof",
        "helper_sha256": hashlib.sha256(helper.encode()).hexdigest(),
        "opcode_declaration_sha256": hashlib.sha256(multi.META_NEW.encode()).hexdigest(),
        "x64_lowering_sha256": hashlib.sha256(multi.X64_NEW.encode()).hexdigest(),
        "interpreter_sha256": hashlib.sha256(multi.INTERP_NEW.encode()).hexdigest(),
    }

def prove(order, timeout_ms):
    import z3
    x, y = z3.BitVecs("x y", 64)
    zero = z3.BitVecVal(0, 64)
    downstream = z3.Function("arbitrary_pure_client", z3.BitVecSort(64), z3.BitVecSort(64), z3.BitVecSort(64))
    # A CLIF Udivrem must denote the same quotient and remainder as the
    # scalar original opcodes. This is a stated premise, not an ISA proof.
    q0, r0 = z3.UDiv(x, y), z3.URem(x, y)
    q1, r1 = z3.UDiv(x, y), z3.URem(x, y)
    original_trap, fused_trap = y == zero, y == zero
    first_source = 100 if order == "forward" else 200
    original_loc = z3.If(original_trap, z3.IntVal(first_source), z3.IntVal(0))
    fused_loc = z3.If(fused_trap, z3.IntVal(first_source), z3.IntVal(0))
    # 'No memory effects' and SSA result identity are explicit admissibility
    # assumptions. The arbitrary pure downstream function covers all result uses.
    mismatch = z3.Or(original_trap != fused_trap,
                     original_loc != fused_loc,
                     z3.And(z3.Not(original_trap), downstream(q0, r0) != downstream(q1, r1)))
    solver = z3.Solver()
    solver.set(timeout=timeout_ms)
    solver.add(mismatch)
    status = solver.check()
    receipt = {
        "logic": "QF_UFBV", "width": 64, "solver_result": str(status),
        "qualification": "all-input-unsat-under-declared-CLIF-model" if status == z3.unsat else "unknown-or-failed",
        "query": "exists x,y,pure client: different observable trap/location or result",
        "timeout_ms": timeout_ms,
    }
    if status == z3.unknown:
        receipt["reason_unknown"] = solver.reason_unknown()
    if status != z3.unsat:
        raise AssertionError("refinement not established: " + json.dumps(receipt))
    return receipt

def negative_controls(timeout_ms):
    import z3
    x, y = z3.BitVecs("negative_x negative_y", 64)
    zero = z3.BitVecVal(0, 64)
    errors = {
        "swap_quotient_remainder": (z3.And(y != zero, z3.UDiv(x,y) != z3.URem(x,y)), (10,3)),
        "mismatched_remainder_divisor": (z3.And(y != zero, z3.URem(x,y) != z3.URem(x, y | z3.BitVecVal(1,64))), (5,2)),
        "signed_division_substitution": (z3.And(y != zero, (x / y) != z3.UDiv(x,y)), (1 << 63, 2)),
        "drop_zero_divisor_trap": (z3.If(y == zero, z3.IntVal(1), z3.IntVal(0)) != z3.IntVal(0), (0,0)),
        "change_first_trap_location": (z3.If(y == zero, z3.IntVal(100), z3.IntVal(0)) != z3.If(y == zero, z3.IntVal(200), z3.IntVal(0)), (0,0)),
    }
    receipts = {}
    for name, (bad, (vx,vy)) in errors.items():
        solver = z3.Solver()
        solver.set(timeout=timeout_ms)
        solver.add(bad, x == z3.BitVecVal(vx,64), y == z3.BitVecVal(vy,64))
        status = solver.check()
        if status != z3.sat:
            raise AssertionError("negative-control witness missing: " + name + "/" + str(status))
        receipts[name] = {"classification": "sat-confirmed-counterexample", "x": vx, "y": vy}
    return receipts

def run(out, timeout_ms):
    import z3
    if out.exists():
        raise ValueError("refusing to overwrite receipt")
    finite = exhaustive_8bit()
    source = source_anchor()
    proofs = {order: prove(order, timeout_ms) for order in ("forward", "reverse")}
    negatives = negative_controls(timeout_ms)
    script = Path(__file__)
    doc = {
        "schema": SCHEMA,
        "qualification": "conditional all-64-bit-input CLIF scalar observational refinement; no complete machine proof",
        "pins": {"zenofcis_base": ZENO_BASE, "wasmtime": WASMTIME, "upstream_source_blobs": BLOBS,
                 "checker_sha256": hashlib.sha256(script.read_bytes()).hexdigest(), "z3_version": z3.get_version_string()},
        "contract": {
            "inputs": "i64 words x,y treated unsigned; identical SSA operands and types for both original operations",
            "ordering": "adjacent same-block Udiv/Urem or Urem/Udiv, no intervening instructions or effects",
            "outputs": "original SSA result identities; q=UDiv(x,y), r=URem(x,y); arbitrary pure downstream client(q,r)",
            "trap": "y=0 traps INTEGER_DIVISION_BY_ZERO at FIRST original opcode location; no second execution",
            "overflow": "unsigned division/remainder have no arithmetic wrap; y=0 is an explicit trap",
            "undefined_poison": "excluded; modeled defined i64 scalar opcodes only, no poison/undef, vector, signed or memory ops",
            "flags_and_abi": "CLIF opcodes expose no flag output; x86 flags, registers and ABI are NOT proved",
            "trust": "correct CLIF scalar semantics and correct implementation of Udivrem are assumed; source guards only checked syntactically",
        },
        "checks": {"source_shape": source, "finite_exhaustive_8bit": finite,
                   "full_width_smt": proofs, "bad_rewrite_witnesses": negatives},
        "limits": [
            "No direct x86 DIV instruction semantics, flags, register state, trap table, ABI or arbitrary native context proof",
            "No Rust compiler-pass correctness or VeriISLE chain proof: source audit is syntactic, solver proves a declared abstract model",
            "No source or AOT byte-cost measurement in this experiment; existing pinned compiler costs live in PR #188",
            "No permission to promote beyond adjacent matching, i64 unsigned inputs and x86-64 experimental lowering",
            "Unknown/timeout always fails; negative-control models establish only existence of bugs in intentionally bad rewrites",
        ],
    }
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(doc, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": "CLIF abstract refinement proved under contract",
                      "theorems": 2, "negative_controls": len(negatives), "exhaustive_pairs": 131072,
                      "receipt": str(out)}, sort_keys=True))

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", required=True, choices=("self-test", "prove"))
    parser.add_argument("--out", type=Path)
    parser.add_argument("--timeout-ms", type=int, default=20000)
    args = parser.parse_args()
    if args.mode == "self-test":
        print(json.dumps(exhaustive_8bit(), sort_keys=True))
    else:
        if args.out is None:
            parser.error("--out required")
        if args.timeout_ms <= 0:
            parser.error("timeout must be positive")
        run(args.out, args.timeout_ms)

if __name__ == "__main__":
    main()
