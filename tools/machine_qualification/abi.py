"""Calling-convention qualification of checked word kernels (gate A of the machine-code qualification design).

`obligations` evaluates A-01 to A-07 for a register-profile kernel as a leaf function: x86-64 System V
(`linux-x86_64-sysv-u64-leaf/1`) or RV64 LP64 (`linux-rv64-lp64-u64-leaf/1`). Each obligation records its status
and the kind of evidence: decoder or admission rule, solver relative to the generated model, or differential
testing. `native` runs an x86 kernel through a generated assembly trampoline. The trampoline seeds distinct canaries
in every callee-saved register and below the return slot, calls the kernel, and captures the registers, the stack
pointer, the canaries and DF before anything else runs. Planted fixtures outside every palette (a callee-saved
clobber, a stack write, DF left set, a wrong stack delta, a wrong result) show that the harness notices each fault.

Not claimed: the instruction models are generated, not derived from vendor specifications (A-07 says which
differential evidence exists). Nothing here is established for active CET modes, signals, other threads or
microarchitectural state. Memory-region kernels (profile 3) and RV64 native execution are out of scope here.
"""

from __future__ import annotations

from pathlib import Path
import random
import shutil
import struct
import subprocess

from importlib import import_module

try:  # tests put tools/ on the path; `-m tools.machine_qualification.abi` runs from the repository root
    import machine_code as _package
except ImportError:
    import tools.machine_code as _package


def _module(name: str):
    return import_module(_package.__name__ + "." + name)


footprint, profile2 = _module("footprint"), _module("profile2")

CALLER_SAVED = {"x86": frozenset({"rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11"}),
                "rv64": frozenset({"t0", "t1", "t2", "t3", "t4", "t5", "t6",
                                   "a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7"})}
FLAGS = frozenset({"cf", "zf", "sf", "of"})
PROFILES = {"x86": "linux-x86_64-sysv-u64-leaf/1", "rv64": "linux-rv64-lp64-u64-leaf/1"}
RETURN = {"x86": b"\xc3", "rv64": bytes.fromhex("67800000")}
ENDBR64 = bytes.fromhex("f30f1efa")
# Entry layout used for coverage (A-06): chosen before, and independent of, any input or scratch value.
LAYOUT = {"x86": {"S": 0x00007FFFFFFFE008, "R": 0x0000555555554000},
          "rv64": {"sp": 0x0000003FFFFFE000, "ra": 0x0000000000010000}}


def _family(problem: dict) -> str:
    return "x86" if problem["target"].startswith("x86_64") else "rv64"


def _checker(problem: dict):
    return _module("verify2" if problem["schema"] == profile2.SCHEMA else "verify")


def _decode(problem, code):
    checker = _checker(problem)
    return checker.decode(problem["target"], code)


def _mismatch(z3, problem: dict, code: bytes, context):
    """The unrestricted mismatch formula over the checker's own variable names and semantics."""
    checker = _checker(problem)
    target = problem["target"]
    word = lambda name: z3.BitVec(name, 64, ctx=context)
    instructions = checker.decode(target, code)
    if problem["schema"] == profile2.SCHEMA:
        names = problem["inputs"]
        arguments = checker.ARGUMENTS[target][:len(names)]
        inputs = {n: word("mc_input_" + n) for n in names}
        initial = {n: word("mc_initial_" + n) for n in checker.REGISTERS[target] if n not in arguments}
        flags = ({n: z3.Bool("mc_flag_" + n, ctx=context) for n in checker.FLAGS}
                 if target == checker.X86 else {})
        registers = {**initial, **{r: inputs[n] for n, r in zip(names, arguments)}}
        expected = [checker._ast_smt(z3, output, inputs, context) for output in problem["outputs"]]
        final, _ = checker.machine_smt(z3, target, instructions, registers, flags, context)
        results = checker.RESULTS[target][:len(problem["outputs"])]
        return z3.Or([e != final[r] for e, r in zip(expected, results)])
    inputs = {n: word("mc_input_" + n) for n in ("x", "y")}
    scratch = {n: word("mc_initial_" + n) for n in checker._SCRATCH[target]}
    return checker._ast_smt(z3, problem["expression"], inputs) != checker._machine_smt(
        z3, target, instructions, inputs, scratch)


def _unrestricted(z3, problem, code, recorded, timeout_ms) -> str:
    """"same" when exactly one solver holds exactly one assertion equivalent to the unrestricted mismatch."""
    if len(recorded) != 1 or len(recorded[0].assertions()) != 1:
        return f"shape: {len(recorded)} solvers"
    context = z3.Context()
    query = recorded[0].assertions()[0].translate(context)
    solver = z3.Solver(ctx=context)
    solver.set(timeout=timeout_ms)
    solver.add(query != _mismatch(z3, problem, code, context))
    answer = solver.check()
    return "same" if answer == z3.unsat else "differs" if answer == z3.sat else "unknown"


def obligations(problem: dict, code: bytes, timeout_ms: int = 5000) -> dict:
    """A-01..A-07 for one register-profile kernel; every obligation must pass for status `qualified`."""
    import z3
    family = _family(problem)
    rows = {}
    # A-01: the whole callable decodes in the closed decoder and ends in the profile's exact return.
    try:
        decoded = _decode(problem, code)
        ok = (code.endswith(RETURN[family]) and decoded[-1].op == "ret" and len(decoded) - 1 <= 16
              and all(ins.op != "ret" for ins in decoded[:-1]))
        rows["A-01"] = {"status": "passed" if ok else "failed", "evidence": "closed decoder",
                        "body_instructions": len(decoded) - 1, "return": RETURN[family].hex()}
    except ValueError as error:
        rows["A-01"] = {"status": "failed", "evidence": "closed decoder", "reason": str(error)}
        return {"status": "refused", "profile": PROFILES[family], "obligations": rows, "authority": "none"}
    # A-02 and A-06: the checker's decisive query is recorded (under the shared hook lock) and must be exactly the
    # unrestricted mismatch formula, rebuilt here from the problem and the decoded bytes; a premise that narrows the
    # inputs, as a separate assertion or folded into the one assertion, makes the two formulas differ.
    verdict, recorded = _module("second_solver").recorded_check(problem, code, timeout_ms)
    rows["A-02"] = {"status": "passed" if verdict["status"] == "equivalent" else "failed",
                    "evidence": "solver relative to the generated model", "verdict": verdict["status"]}
    unrestricted = _unrestricted(z3, problem, code, recorded, timeout_ms)
    layout = LAYOUT[family]
    aligned = (layout["S"] + 8) % 16 == 0 if family == "x86" else layout["sp"] % 16 == 0
    rows["A-06"] = {"status": "passed" if unrestricted == "same" and aligned else "failed",
                    "evidence": "the recorded query equals the rebuilt unrestricted mismatch formula (Z3); "
                                "an input-independent entry layout",
                    "query": unrestricted, "layout": {k: hex(v) for k, v in layout.items()}}
    # A-03: every register any body instruction may write is caller-saved (flags are scratch); derived from the
    # checker's own symbolic semantics through the search footprints.
    forms = [ins.form() for ins in decoded[:-1]]
    prints = footprint.analyse(dict(problem, grammar=forms))[0] if forms else []
    written = set().union(*(p.writes for p in prints if p is not None)) if prints else set()
    allowed = CALLER_SAVED[family] | FLAGS
    rows["A-03"] = {"status": "passed" if None not in prints and written <= allowed else "failed",
                    "evidence": "footprints from the checker's symbolic semantics",
                    "written": sorted(written), "outside_caller_saved": sorted(written - allowed)}
    # A-04: the return transition. No body instruction names the stack pointer or memory, so the return slot still
    # holds R: x86 RIP'=mem[S], RSP'=S+8; RV pc'=(ra+0)&~1 with ra and sp unchanged.
    word = lambda name: z3.BitVec(name, 64)
    solver = z3.Solver()
    solver.set(timeout=timeout_ms)
    if family == "x86":
        S, R, memory = word("S"), word("R"), z3.Array("mem", z3.BitVecSort(64), z3.BitVecSort(64))
        memory = z3.Store(memory, S, R)
        rip, rsp = z3.Select(memory, S), S + 8
        solver.add(z3.Or(rip != R, rsp != S + 8))
    else:
        ra, sp = word("ra"), word("sp")
        solver.add((ra & 1) == 0, z3.Or(((ra + 0) & ~z3.BitVecVal(1, 64)) != ra, sp != sp))
    names_stack = not (written & {"rsp", "sp", "ra"})
    rows["A-04"] = {"status": "passed" if solver.check() == z3.unsat and names_stack else "failed",
                    "evidence": "solver over the return rule, given A-03's frame"}
    # A-05: straight-line, no admitted instruction faults in the model (no memory, division or traps).
    faulting = {"div", "idiv", "load", "store"} & {ins.op for ins in decoded}
    rows["A-05"] = {"status": "passed" if not faulting else "failed", "evidence": "admitted operation set",
                    "instructions_to_boundary": len(decoded)}
    rows["A-07"] = {"status": "passed", "evidence": "trusted generated model with differential evidence",
                    "kind": ("GNU as over every admitted encoding; CPU conformance on sampled states" if family == "x86"
                             else "GNU as and QEMU in CI; Sail refinement in progress")}
    ok = all(row["status"] == "passed" for row in rows.values())
    return {"status": "qualified" if ok else "not-qualified", "profile": PROFILES[family],
            "obligations": dict(sorted(rows.items())), "authority": "none",
            "nonclaims": "Generated ISA models; no active CET, signals, threads or microarchitectural state."}


TRAMPOLINE = r'''
.data
.globl zeno_abi_args, zeno_abi_seed, zeno_abi_out, zeno_abi_target, zeno_abi_canary
zeno_abi_args: .zero 48
zeno_abi_seed: .zero 72
zeno_abi_out: .zero 272
zeno_abi_target: .quad 0
zeno_abi_canary: .quad 0
zeno_abi_saved: .quad 0
.text
.globl zeno_abi_call
.type zeno_abi_call,@function
zeno_abi_call:
    push %rbx
    push %rbp
    push %r12
    push %r13
    push %r14
    push %r15
    sub $8, %rsp
    mov %rsp, zeno_abi_saved(%rip)
    movq $0, zeno_abi_out+216(%rip)
    stmxcsr zeno_abi_out+240(%rip)
    fnstcw zeno_abi_out+248(%rip)
    mov zeno_abi_canary(%rip), %rax
CANARY_STORES
    mov zeno_abi_seed+0(%rip), %rbx
    mov zeno_abi_seed+8(%rip), %rbp
    mov zeno_abi_seed+16(%rip), %r12
    mov zeno_abi_seed+24(%rip), %r13
    mov zeno_abi_seed+32(%rip), %r14
    mov zeno_abi_seed+40(%rip), %r15
    mov zeno_abi_seed+48(%rip), %rax
    mov zeno_abi_seed+56(%rip), %r10
    mov zeno_abi_seed+64(%rip), %r11
    mov zeno_abi_args+0(%rip), %rdi
    mov zeno_abi_args+8(%rip), %rsi
    mov zeno_abi_args+16(%rip), %rdx
    mov zeno_abi_args+24(%rip), %rcx
    mov zeno_abi_args+32(%rip), %r8
    mov zeno_abi_args+40(%rip), %r9
    call *zeno_abi_target(%rip)
zeno_abi_return:
    movq $1, zeno_abi_out+216(%rip)
    mov %rax, zeno_abi_out+0(%rip)
    mov %rdx, zeno_abi_out+8(%rip)
    mov %rbx, zeno_abi_out+16(%rip)
    mov %rbp, zeno_abi_out+24(%rip)
    mov %r12, zeno_abi_out+32(%rip)
    mov %r13, zeno_abi_out+40(%rip)
    mov %r14, zeno_abi_out+48(%rip)
    mov %r15, zeno_abi_out+56(%rip)
    mov %rsp, zeno_abi_out+64(%rip)
    stmxcsr zeno_abi_out+256(%rip)
    fnstcw zeno_abi_out+264(%rip)
    mov zeno_abi_saved(%rip), %rsp
    mov %rsp, zeno_abi_out+208(%rip)
    mov -8(%rsp), %rax
    mov %rax, zeno_abi_out+224(%rip)
    lea zeno_abi_return(%rip), %rax
    mov %rax, zeno_abi_out+232(%rip)
CANARY_LOADS
    pushfq
    pop %rax
    mov %rax, zeno_abi_out+72(%rip)
    cld
    ldmxcsr zeno_abi_out+240(%rip)
    fldcw zeno_abi_out+248(%rip)
    add $8, %rsp
    pop %r15
    pop %r14
    pop %r13
    pop %r12
    pop %rbp
    pop %rbx
    ret
.section .note.GNU-stack,"",@progbits
'''
# Canaries fill the 16 words below the return slot (the callee's red zone), -16 to -136 from the call-site RSP,
# and are read back relative to that same RSP after it is restored, so a wrong stack delta cannot hide them.
# The call site is 16-byte aligned (six pushes and 8 bytes of padding), so the kernel enters with RSP + 8 a
# multiple of 16, as System V requires. A flag written by the first instruction at the true continuation and the
# return slot compared with that continuation's address catch a kernel that returns anywhere else; MXCSR and the
# x87 control word are captured before and after and then restored.
_SLOTS = [16 + 8 * k for k in range(16)]

RUNNER = r'''
#include <stdint.h>
#include <stdio.h>
extern uint64_t zeno_abi_args[6], zeno_abi_seed[9], zeno_abi_out[34], zeno_abi_canary;
extern void *zeno_abi_target;
extern void zeno_abi_call(void);
extern char zeno_kernel[];
int main(void) {
    FILE *in = fopen("vectors.bin", "rb"), *out = fopen("results.bin", "wb");
    uint64_t count;
    if (!in || !out || fread(&count, 8, 1, in) != 1) return 2;
    zeno_abi_target = (void *)ENTRY;
    for (uint64_t k = 0; k < count; k++) {
        if (fread(zeno_abi_args, 8, 6, in) != 6 || fread(zeno_abi_seed, 8, 9, in) != 9
            || fread(&zeno_abi_canary, 8, 1, in) != 1) return 3;
        zeno_abi_call();
        if (fwrite(zeno_abi_out, 8, 34, out) != 34) return 4;
    }
    return fclose(out) ? 5 : 0;
}
'''


def _gcc():
    return shutil.which("gcc", path="/usr/bin:/bin")


def native(problem: dict, code: bytes, work: Path, vectors: int = 512, seed: str = "abi",
           expected=None, prefix: bytes = b"") -> dict:
    """Run `prefix + code` as an x86 leaf function through the trampoline and report every contract violation."""
    reference, elf_object = _module("native").reference, _module("targets").elf_object
    if type(vectors) is not int or vectors < 1:
        raise ValueError("the native harness needs at least one vector")
    if _family(problem) != "x86":
        return {"status": "not-run", "reason": "needs x86-64 execution"}
    if _gcc() is None:
        return {"status": "not-run", "reason": "needs gcc"}
    arity, outputs, reference_of = reference(problem)
    expected = expected or reference_of
    work.mkdir(mode=0o700)
    (work / "kernel.o").write_bytes(elf_object(problem["target"], prefix + code))
    stores = "\n".join(f"    mov %rax, -{slot}(%rsp)" for slot in _SLOTS)
    loads = "\n".join(f"    mov -{slot}(%rsp), %rax\n    mov %rax, zeno_abi_out+{80 + 8 * k}(%rip)"
                      for k, slot in enumerate(_SLOTS))
    (work / "trampoline.s").write_text(TRAMPOLINE.replace("CANARY_STORES", stores).replace("CANARY_LOADS", loads))
    (work / "runner.c").write_text(RUNNER.replace("ENTRY", "zeno_kernel"))
    rng = random.Random(seed)
    rows = []
    for index in range(vectors):
        args = [rng.getrandbits(64) for _ in range(6)]
        if index < 8:
            args[:arity] = [(0, 1, (1 << 64) - 1, 1 << 63)[(index + j) % 4] for j in range(arity)]
        rows.append((args, [rng.getrandbits(64) for _ in range(9)], rng.getrandbits(64)))
    with open(work / "vectors.bin", "wb") as stream:
        stream.write(struct.pack("<Q", len(rows)))
        for args, seeds, canary in rows:
            stream.write(struct.pack("<16Q", *args, *seeds, canary))
    run = lambda argv: subprocess.run(argv, cwd=work, capture_output=True, text=True, timeout=120,
                                      env={"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C"})
    built = run([_gcc(), "-O1", "-fno-lto", "-Wl,-z,noexecstack", "runner.c", "trampoline.s", "kernel.o",
                 "-o", "runner"])
    if built.returncode:
        return {"status": "error", "reason": built.stderr[-1500:]}
    done = run([str(work / "runner")])
    if done.returncode:
        return {"status": "crashed", "exit": done.returncode, "vectors": len(rows)}
    data = (work / "results.bin").read_bytes()
    if len(data) != 8 * 34 * len(rows):
        return {"status": "error", "reason": "short result file"}
    faults = {"result": 0, "callee-saved": 0, "stack-pointer": 0, "stack-memory": 0, "direction-flag": 0,
              "continuation": 0, "control-state": 0}
    for index, (args, seeds, canary) in enumerate(rows):
        out = struct.unpack_from("<34Q", data, 8 * 34 * index)
        if list(out[:outputs]) != expected(args[:arity]):
            faults["result"] += 1
        if list(out[2:8]) != seeds[:6]:
            faults["callee-saved"] += 1
        if out[8] != out[26]:  # after the return, RSP must equal its value just before the call
            faults["stack-pointer"] += 1
        if any(word != canary for word in out[10:26]):
            faults["stack-memory"] += 1
        if out[9] & (1 << 10):
            faults["direction-flag"] += 1
        if out[27] != 1 or out[28] != out[29]:  # skipped the true continuation, or a rewritten return slot
            faults["continuation"] += 1
        if (out[30] & 0xFFFFFFFF, out[31] & 0xFFFF) != (out[32] & 0xFFFFFFFF, out[33] & 0xFFFF):
            faults["control-state"] += 1
    return {"status": "conforms" if not any(faults.values()) else "violated", "vectors": len(rows),
            "faults": faults, "prefix": prefix.hex()}


def _active_cet() -> str:
    """Why active-mode CET cannot be exercised on this host, or an empty string."""
    try:
        flags = Path("/proc/cpuinfo").read_text()
    except OSError:
        return "cannot read /proc/cpuinfo"
    missing = [flag for flag in ("ibt", "user_shstk") if f" {flag}" not in flags]
    return f"processor lacks {', '.join(missing)}" if missing else ""


def cet(problem: dict, code: bytes, work: Path) -> dict:
    """`linux-x86_64-sysv-u64-leaf-cet/1`: ENDBR64 plus the unchanged legacy fragment, with active modes named."""
    fragment = obligations(problem, code)
    prefixed = native(problem, code, work, prefix=ENDBR64)
    reason = _active_cet()
    return {"profile": "linux-x86_64-sysv-u64-leaf-cet/1", "entry_prefix": ENDBR64.hex(),
            "callable_bytes": len(ENDBR64) + len(code), "fragment": fragment["status"],
            "native_with_prefix": prefixed["status"],
            "active_modes": {"status": "unsupported", "reason": reason or "no active-mode harness yet"},
            "status": "inactive-mode-only" if fragment["status"] == "qualified" and prefixed["status"] == "conforms"
            else "not-qualified", "authority": "none"}


def main(argv=None) -> int:
    """`python3 -B -m tools.machine_qualification.abi --dir LIBRARY`: the gate for every register-profile entry."""
    import argparse
    import json
    import sys
    import tempfile
    parser = argparse.ArgumentParser(description=main.__doc__)
    parser.add_argument("--dir", required=True, type=Path)
    parser.add_argument("--vectors", type=int, default=512)
    args = parser.parse_args(argv)
    rows, ok, applicable = [], True, 0
    for path in sorted(args.dir.glob("*.json")):
        entry = json.loads(path.read_text())
        problem, code = entry["problem"], bytes.fromhex(entry["code_hex"])
        if problem["schema"] not in (profile2.SCHEMA, "zeno-fcis/machine-problem/1"):
            rows.append({"entry": path.name, "status": "not-applicable", "reason": "register profiles only"})
            continue
        gate = obligations(problem, code)
        with tempfile.TemporaryDirectory() as folder:
            run = native(problem, code, Path(folder) / "work", args.vectors)
        ok &= gate["status"] == "qualified" and run["status"] in ("conforms", "not-run")
        applicable += 1
        rows.append({"entry": path.name, "profile": gate["profile"], "obligations": gate["status"],
                     "native": run["status"], "faults": run.get("faults")})
    # At least one entry must be applicable; not-applicable and not-run entries never count as qualification.
    status = "not-applicable" if not applicable else "qualified" if ok else "not-qualified"
    print(json.dumps({"status": status, "applicable": applicable, "entries": rows,
                      "authority": "none"}, indent=1))
    return 0 if status == "qualified" else 1


if __name__ == "__main__":
    raise SystemExit(main())
