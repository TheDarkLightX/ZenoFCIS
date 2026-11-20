#!/usr/bin/env python3
"""Unseeded ZenoFCIS synthesis-to-Cranelift *screen*, not a lowering patch.

A known unsigned-minimum contract is deliberately used as a calibration case.
Existing library matches are classified as rediscovery, never original novelty.
Native fixed-ABI kernel bytes and compiler-emitted CLIF function bytes are
reported separately; comparing them does NOT establish an interchangeable ABI.
"""
from __future__ import annotations
import argparse
import ctypes
import hashlib
import json
import os
import platform
import random
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from tools.machine_code import session, targets, verify2
from tools.machine_code.model import canonical, read_json

SCHEMA = "zenofcis/tier1-unseeded-umin-clif-screen/1"
WASMTIME = "e355dc640d047ab76dbf4402567d0c124bfdecd6"
PROBLEM = ROOT / "test-data/machine-code/umin-x86.json"
KNOWN = ROOT / "test-data/machine-code/kernels/umin-x86.json"
TARGET = "x86_64-sysv-register/2"
CLIF = """test verifier
set opt_level=speed_and_size
target x86_64

; Unsigned u64 minimum. Same two input values as the Zeno fixed-ABI kernel.
function %tier1_umin(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
    v2 = icmp ult v0, v1
    v3 = select v2, v0, v1
    return v3
}
"""

def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()

def filehash(p: Path) -> str:
    return sha(p.read_bytes())

def git(*args: str) -> str:
    return subprocess.check_output(["git", "-C", str(ROOT), *args], text=True).strip()

def admitted(problem: dict) -> None:
    assert problem["schema"] == "zeno-fcis/machine-problem/2"
    assert problem["target"] == TARGET and problem["inputs"] == ["x", "y"]
    assert problem["outputs"] == [["ite", ["ule", ["input", "x"], ["input", "y"]], ["input", "x"], ["input", "y"]]]
    assert problem["objective"] == "bytes" and problem["max_instructions"] == 3
    assert len(problem["grammar"]) == 6

def native_test(artifact: Path, out: Path) -> dict:
    if platform.machine().lower() not in ("x86_64", "amd64"):
        raise ValueError("native qualification requires x86-64")
    so = out / "kernel.so"
    subprocess.run(["gcc", "-O2", "-shared", "-fPIC", "-o", str(so), str(artifact / "kernel.S")],
                   check=True, capture_output=True, text=True)
    dll = ctypes.CDLL(str(so.resolve()))
    fn = dll.zeno_kernel
    fn.argtypes = [ctypes.c_uint64, ctypes.c_uint64]
    fn.restype = ctypes.c_uint64
    edge = [0, 1, 2, 3, 255, 256, 1 << 31, 1 << 32,
            (1 << 63) - 1, 1 << 63, (1 << 64) - 2, (1 << 64) - 1]
    vec = [(x, y) for x in edge for y in edge]
    rng = random.Random(20261120)
    vec.extend((rng.getrandbits(64), rng.getrandbits(64)) for _ in range(10000))
    for x, y in vec:
        got = fn(x, y)
        want = x if x <= y else y
        if got != want:
            raise ValueError("native unsigned-min counterexample x=%d y=%d got=%d want=%d" % (x,y,got,want))
    return {"class": "finite-native-execution", "checked": len(vec),
            "edge_pairs": len(edge) ** 2, "random_pairs": 10000,
            "seed": 20261120, "failures": 0,
            "shared_object_sha256": filehash(so)}

def run(out: Path) -> None:
    if out.exists():
        raise ValueError("output directory already exists")
    problem = read_json(PROBLEM)
    admitted(problem)
    known = read_json(KNOWN)
    if known["problem"] != problem:
        raise ValueError("pinned known candidate contract differs")
    out.mkdir(parents=True)
    opened = session.open_session(PROBLEM, out / "session")
    if (out / "session/events.jsonl").read_bytes():
        raise ValueError("search cannot start with a seeded proposal")
    result = session.search(out / "session", max_candidates=5000,
                            max_checks=8, deadline_ms=20000, timeout_ms=1000, prune=True)
    if result["status"] != "checked-candidate":
        raise ValueError("no independently verified search result: " + repr(result))
    resumed = session.resume(out / "session", timeout_ms=3000)
    if resumed["status"] != "replayed":
        raise ValueError("search incumbent did not independently recheck")
    emitted = session.emit(out / "session", out / "artifact", timeout_ms=3000)
    recheck = session.replay_artifact(out / "artifact", timeout_ms=3000)
    if emitted["status"] != "emitted" or recheck["status"] != "replayed":
        raise ValueError("artifact receipt replay failed")
    code = (out / "artifact/kernel.bin").read_bytes()
    fresh = verify2.check(problem, code, timeout_ms=3000)
    if fresh["status"] != "equivalent":
        raise ValueError("independent fresh SMT model check was not equivalent")
    if fresh["cost"] != emitted["cost"] or len(code) != fresh["cost"]["bytes"]:
        raise ValueError("native byte cost not bound to checked code")
    wrong = targets.encode_program(TARGET, [["mov", "rax", "rdi"]])
    negative = verify2.check(problem, wrong, timeout_ms=3000)
    if negative["status"] != "counterexample":
        raise ValueError("planted wrong candidate was not refuted: " + repr(negative))
    native = native_test(out / "artifact", out)
    (out / "umin.clif").write_text(CLIF)
    report = {
        "schema": SCHEMA, "status": "checked-synthesized-existing-contract",
        "zenofcis_commit": git("rev-parse", "HEAD"),
        "upstream_commit_to_measure": WASMTIME,
        "problem_sha256": filehash(PROBLEM), "known_library_sha256": filehash(KNOWN),
        "clif_sha256": filehash(out / "umin.clif"),
        "semantic_contract": {
            "inputs": "all pairs of unsigned i64 words; output unsigned min(x,y)",
            "signedness": "unsigned; signed minimum not admitted",
            "overflow": "no arithmetic overflow in min",
            "traps": "none in admitted pure integer contract",
            "flags": "machine flag intermediates verified under Zeno profile-2 model, not full Cranelift state",
            "undefined_poison": "no poison/undefined admitted by this kernel profile; general CLIF poison proof absent",
            "abi": TARGET, "hardware": "x86-64 SysV; native finite test on runner",
        },
        "search": result, "no_seed": True, "source": "session.search exact ZenoFCIS implementation",
        "search_originality": "rediscovery" if code.hex() == known["code_hex"] else "distinct from the pinned library entry; novelty beyond that not assessed",
        "source_identity": session.identity(),
        "candidate": {"code_hex": code.hex(), "sha256": sha(code),
                      "cost": fresh["cost"], "verification": fresh,
                      "artifact_sha256": filehash(out / "artifact/kernel.o")},
        "negative_control": {"verdict": negative["status"], "candidate": wrong.hex()},
        "native": native,
        "cranelift": "not yet measured",
        "limits": ["SMT proves only registered x86 model and declared returned value",
                   "native finite test is not exhaustive", "no compiler-pass or ISLE adaptation",
                   "machine flags, abi preservation and poison outside admitted model unproved",
                   "raw fixed-ABI kernel bytes cannot be credited as a Cranelift reduction",
                   "search stop reason is not an unrestricted optimality proof"],
    }
    (out / "synthesis.json").write_text(json.dumps(report, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"result": result["status"], "search_origin": report["search_originality"],
                      "checks": result["solver_checks"], "bytes": len(code),
                      "native_vectors": native["checked"]}, sort_keys=True))

def check_compiled_machine_core(out: Path, obj: Path, expected_symbol_bytes: int) -> dict:
    """Verify the ACTUAL compiler-emitted core only for one exact x86 frame.

    A recognized frame and proof of its inner body do not establish a full
    compiler, ABI, flag-liveness, unwind, poison or trap-table refinement.
    """
    raw_path = out / "clif-text.bin"
    subprocess.run(["objcopy", "--only-section=.text", "-O", "binary",
                    str(obj), str(raw_path)], check=True, capture_output=True, text=True)
    raw = raw_path.read_bytes()
    if len(raw) != expected_symbol_bytes:
        raise ValueError("unexpected .text size or multiple functions; cannot isolate body")
    prologue = bytes.fromhex("554889e5")       # push rbp; mov rbp,rsp
    epilogue = bytes.fromhex("4889ec5dc3")     # mov rsp,rbp; pop rbp; ret
    if not raw.startswith(prologue) or not raw.endswith(epilogue):
        raise ValueError("frame differs from EXACT recognized pattern; refuse body attribution")
    core = raw[len(prologue):-len(epilogue)] + b"\xc3"
    body_instructions = [["cmp", "rsi", "rdi"], ["mov", "rax", "rdi"],
                         ["cmovb", "rax", "rsi"]]
    if core != targets.encode_program(TARGET, body_instructions):
        raise ValueError("actual native body changed; do not reuse a stale instruction model")
    admitted_problem = read_json(PROBLEM)
    admitted(admitted_problem)
    forms = {tuple(row) for row in admitted_problem["grammar"]}
    expanded = dict(admitted_problem)
    expanded["grammar"] = admitted_problem["grammar"] + [
        row for row in body_instructions if tuple(row) not in forms]
    verdict = verify2.check(expanded, core, timeout_ms=3000)
    if verdict["status"] != "equivalent":
        raise ValueError("actual compiled arithmetic body not independently SMT verified: " + repr(verdict))
    native_dir = out / "compiled-core"
    native_dir.mkdir()
    (native_dir / "kernel.S").write_bytes(targets.assembly_source(core))
    native = native_test(native_dir, native_dir)
    return {
        "status": "compiled-arithmetic-body-model-and-native-qualified",
        "scope": "exact recognized x86 frame removed; NOT compiler-pass/ABI/flags/unwind/trap-table proof",
        "frame": {"prologue_hex": prologue.hex(), "epilogue_hex": epilogue.hex(),
                  "prologue_bytes": len(prologue), "epilogue_bytes_excluding_ret": len(epilogue)-1},
        "full_symbol_bytes": expected_symbol_bytes, "body_including_ret_bytes": len(core),
        "core_hex": core.hex(), "core_sha256": sha(core),
        "model_verdict": verdict, "native_finite": native,
    }

def measure(out: Path, upstream: Path, obj: Path) -> None:
    report = json.loads((out / "synthesis.json").read_text())
    if report["schema"] != SCHEMA or report["zenofcis_commit"] != git("rev-parse", "HEAD"):
        raise ValueError("source revision drift")
    if subprocess.check_output(["git", "-C", str(upstream), "rev-parse", "HEAD"], text=True).strip() != WASMTIME:
        raise ValueError("upstream source revision drift")
    if report["problem_sha256"] != filehash(PROBLEM) or report["known_library_sha256"] != filehash(KNOWN):
        raise ValueError("problem/library inputs changed")
    if report["clif_sha256"] != filehash(out / "umin.clif"):
        raise ValueError("CLIF cost fixture changed")
    if report["candidate"]["sha256"] != filehash(out / "artifact/kernel.bin"):
        raise ValueError("machine kernel changed")
    outnm = subprocess.check_output(["nm", "-S", "--defined-only", "--format=posix", str(obj)], text=True)
    sized = [(parts[0], int(parts[3],16)) for line in outnm.splitlines()
             if len(parts := line.split()) == 4 and parts[1] in ("T","t")]
    if len(sized) != 1 or sized[0][1] <= 0:
        raise ValueError("expected exactly one sized Cranelift function: " + repr(sized))
    dump = subprocess.check_output(["objdump", "-dr", "-Mintel", str(obj)], text=True)
    (out / "cranelift-disassembly.txt").write_text(dump)
    native_bytes = report["candidate"]["cost"]["bytes"]
    symbol_bytes = sized[0][1]
    compiled = check_compiled_machine_core(out, obj, symbol_bytes)
    core_bytes = compiled["body_including_ret_bytes"]
    catalog = {
        "schema": "zenofcis/tier1-synthesized-clif-cost-catalog/1",
        "experiment": report["zenofcis_commit"], "upstream": WASMTIME,
        "semantic_contract": report["semantic_contract"],
        "dedup_key": sha(canonical({"contract": report["semantic_contract"], "code_hex": report["candidate"]["code_hex"]})),
        "search_class": report["search_originality"],
        "checker_class": "registered machine-state SMT model equivalence",
        "finite_native": report["native"],
        "negative_control": report["negative_control"],
        "source_receipts": {
            "machine_code_sha256": report["candidate"]["sha256"],
            "machine_object_sha256": report["candidate"]["artifact_sha256"],
            "problem_sha256": report["problem_sha256"], "clif_sha256": report["clif_sha256"],
            "clif_object_sha256": filehash(obj),
            "disassembly_sha256": filehash(out / "cranelift-disassembly.txt"),
        },
        "costs": {
            "raw_zeno_sysv_machine_code_bytes_including_ret": native_bytes,
            "cranelift_compiled_full_function_symbol_bytes": symbol_bytes,
            "cranelift_exact_frame_neutralized_body_bytes_including_ret": core_bytes,
            "compiler_core_vs_zeno_core_bytes_saved": core_bytes - native_bytes,
            "cranelift_frame_overhead_bytes": symbol_bytes - core_bytes,
            "cranelift_opt_level": "speed_and_size", "cranelift_target": "x86_64",
            "difference_arithmetic_only_not_a_valid_saving_claim": symbol_bytes-native_bytes,
        },
        "compiled_body_refinement": compiled,
        "qualification": "measured separate code paths; not a same-WAT patched-versus-unpatched compiler A/B",
        "limits": report["limits"] + [
            "CLIF calling convention/physical registers not independently bridged to raw SysV kernel",
            "No evidence of an upstream optimization or novel sequence if rediscovery",
            "No representative workload performance or throughput measurement"],
    }
    (out / "catalog.json").write_text(json.dumps(catalog, sort_keys=True, indent=2)+"\n")
    print(json.dumps({"native_kernel_bytes": native_bytes,
                      "cranelift_function_bytes": symbol_bytes,
                      "qualified_saving": False,
                      "search_origin": report["search_originality"]}, sort_keys=True))

if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--mode", required=True, choices=("run", "measure"))
    p.add_argument("--out", required=True, type=Path)
    p.add_argument("--upstream", type=Path)
    p.add_argument("--obj", type=Path)
    a = p.parse_args()
    if a.mode == "run":
        run(a.out)
    else:
        if a.upstream is None or a.obj is None:
            p.error("measure requires --upstream and --obj")
        measure(a.out, a.upstream, a.obj)
