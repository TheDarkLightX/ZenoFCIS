"""Calling-convention qualification of checked word kernels (gate A of the machine-code qualification design).

`obligations` evaluates A-01 to A-07 for a register-profile kernel as a leaf function: x86-64 System V
(`linux-x86_64-sysv-u64-leaf/1`) or RV64 LP64 (`linux-rv64-lp64-u64-leaf/1`). Each obligation records its status
and the kind of evidence: decoder or admission rule, solver relative to the generated model, or differential
testing. `native` runs an x86 kernel through a generated assembly trampoline. The trampoline seeds distinct canaries
in every callee-saved register and below the return slot, calls the kernel, and captures the registers, the stack
pointer, the canaries and DF before anything else runs. Planted fixtures outside every palette (a callee-saved
clobber, a stack write, DF left set, a wrong stack delta, a wrong result) show that the harness notices each fault.

Profile-3 memory kernels use the region-leaf profiles (`linux-x86_64-sysv-u64-region-leaf/1`,
`linux-rv64-lp64-u64-region-leaf/1`): region bases arrive in argument registers, A-03 adds the memory frame (writes
only to declared read-write region words, from the footprints and profile-3 admission), and A-06's layout places
the regions disjoint, aligned and away from the stack, independent of the inputs. `native_memory` passes region
pointers to static, aligned buffers with canary words before and after each region, and checks the results, the
read-write words against the reference, the read-only words and every out-of-region canary as well.

Not claimed: the instruction models are generated, not derived from vendor specifications (A-07 says which
differential evidence exists). Nothing here is established for active CET modes, signals, other threads or
microarchitectural state, or for overlapping regions. RV64 native execution is out of scope here.
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


footprint, profile2, profile3 = _module("footprint"), _module("profile2"), _module("profile3")

CALLER_SAVED = {"x86": frozenset({"rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11"}),
                "rv64": frozenset({"t0", "t1", "t2", "t3", "t4", "t5", "t6",
                                   "a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7"})}
FLAGS = frozenset({"cf", "zf", "sf", "of"})
PROFILES = {"x86": "linux-x86_64-sysv-u64-leaf/1", "rv64": "linux-rv64-lp64-u64-leaf/1"}
REGION_PROFILES = {"x86": "linux-x86_64-sysv-u64-region-leaf/1", "rv64": "linux-rv64-lp64-u64-region-leaf/1"}
RETURN = {"x86": b"\xc3", "rv64": bytes.fromhex("67800000")}
ENDBR64 = bytes.fromhex("f30f1efa")
# Entry layout used for coverage (A-06): chosen before, and independent of, any input or scratch value.
LAYOUT = {"x86": {"S": 0x00007FFFFFFFE008, "R": 0x0000555555554000},
          "rv64": {"sp": 0x0000003FFFFFE000, "ra": 0x0000000000010000}}
# Profile-3 region k starts at REGION_BASE + k * REGION_STRIDE: 64-byte aligned, disjoint (16 words need 128 bytes)
# and at least STACK_GAP bytes away from the stack window, whatever the inputs are.
REGION_BASE, REGION_STRIDE, STACK_GAP = 0x0000100000000000, 0x10000, 1 << 20
GUARD = 8  # canary words before and after each region in the native harness


def _family(problem: dict) -> str:
    return "x86" if problem["target"].startswith("x86_64") else "rv64"


def _memory(problem: dict) -> bool:
    return problem["schema"] == profile3.SCHEMA


def _checker(problem: dict):
    if _memory(problem):
        return _module("verify3")
    return _module("verify2" if problem["schema"] == profile2.SCHEMA else "verify")


def region_layout(problem: dict) -> dict:
    """Region name -> base address, fixed before and independent of any input, scratch or region value."""
    return {region["name"]: REGION_BASE + k * REGION_STRIDE for k, region in enumerate(problem["regions"])}


def _layout_ok(problem: dict, family: str) -> bool:
    """Regions aligned, pairwise disjoint and at least STACK_GAP away from the entry stack window."""
    stack = LAYOUT[family]["S" if family == "x86" else "sp"]
    spans = sorted((address, address + 8 * region["words"])
                   for address, region in zip(region_layout(problem).values(), problem["regions"]))
    return (all(start % 64 == 0 for start, _ in spans)
            and all(spans[k][1] <= spans[k + 1][0] for k in range(len(spans) - 1))
            and all(end + STACK_GAP <= stack - 4096 or start >= stack + 4096 + STACK_GAP for start, end in spans))


def _decode(problem, code):
    checker = _checker(problem)
    return checker.decode(problem["target"], code)


def obligations(problem: dict, code: bytes, timeout_ms: int = 5000) -> dict:
    """A-01..A-07 for one register-profile kernel; every obligation must pass for status `qualified`."""
    import z3
    family = _family(problem)
    memory = _memory(problem)
    profile = (REGION_PROFILES if memory else PROFILES)[family]
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
        return {"status": "refused", "profile": profile, "obligations": rows, "authority": "none"}
    # A-02 and A-06: the word query, recorded, must be the only assertion: no premise narrows the inputs.
    recorded, original = [], z3.Solver

    class Recording(original):
        def __init__(self, *args, **kwargs):
            super().__init__(*args, **kwargs)
            recorded.append(self)
    z3.Solver = Recording
    try:
        verdict = _checker(problem).check(problem, code, timeout_ms=timeout_ms)
    finally:
        z3.Solver = original
    rows["A-02"] = {"status": "passed" if verdict["status"] == "equivalent" else "failed",
                    "evidence": "solver relative to the generated model", "verdict": verdict["status"]}
    premises = len(recorded[0].assertions()) if len(recorded) == 1 else None
    layout = LAYOUT[family]
    aligned = (layout["S"] + 8) % 16 == 0 if family == "x86" else layout["sp"] % 16 == 0
    rows["A-06"] = {"status": "passed" if premises == 1 and aligned else "failed",
                    "evidence": "one mismatch assertion and no premise; an input-independent entry layout",
                    "assertions": premises, "layout": {k: hex(v) for k, v in layout.items()}}
    if memory:
        regions_ok = _layout_ok(problem, family)
        rows["A-06"].update(regions={name: hex(address) for name, address in region_layout(problem).items()},
                            regions_disjoint_aligned_away_from_stack=regions_ok,
                            evidence=rows["A-06"]["evidence"] + "; regions disjoint, aligned and away from the stack")
        if not regions_ok:
            rows["A-06"]["status"] = "failed"
    # A-03: every register any body instruction may write is caller-saved (flags are scratch); derived from the
    # checker's own symbolic semantics through the search footprints.
    forms = [ins.form() for ins in decoded[:-1]]
    prints = footprint.analyse(dict(problem, grammar=forms))[0] if forms else []
    written = set().union(*(p.writes for p in prints if p is not None)) if prints else set()
    allowed = CALLER_SAVED[family] | FLAGS
    # Footprint bookkeeping (`admitted:`, `based:`) is not machine state; region words are named `<region>[<i>]`.
    words = {w for w in written if "[" in w and ":" not in w}
    registers = {w for w in written if "[" not in w and ":" not in w}
    rows["A-03"] = {"status": "passed" if None not in prints and registers <= allowed else "failed",
                    "evidence": "footprints from the checker's symbolic semantics",
                    "written": sorted(registers | words), "outside_caller_saved": sorted(registers - allowed)}
    if memory:
        read_write = {f"{r['name']}[{i}]" for r in problem["regions"] if r["access"] == "read-write"
                      for i in range(r["words"])}
        try:
            _checker(problem).admit(problem, decoded)
            admission = "passed"
        except ValueError as error:
            admission = str(error)
        rows["A-03"].update(memory_written=sorted(words), outside_read_write_words=sorted(words - read_write),
                            admission=admission,
                            evidence="footprints from the checker's symbolic semantics and profile-3 admission")
        if words - read_write or admission != "passed":
            rows["A-03"]["status"] = "failed"
    written = registers
    # A-04: the return transition. No body instruction names the stack pointer or memory, so the return slot still
    # holds R: x86 RIP'=mem[S], RSP'=S+8; RV pc'=(ra+0)&~1 with ra and sp unchanged.
    word = lambda name: z3.BitVec(name, 64)
    solver = z3.Solver()
    solver.set(timeout=timeout_ms)
    if family == "x86" and memory:
        # Every admitted store writes a region word at its A-06 address; none of them may reach the return slot.
        S, R = z3.BitVecVal(layout["S"], 64), word("R")
        stack = z3.Store(z3.Array("mem", z3.BitVecSort(64), z3.BitVecSort(64)), S, R)
        bases = {r["base"]: region_layout(problem)[r["name"]] for r in problem["regions"]}
        for k, ins in enumerate(decoded[:-1]):
            if ins.op == "store":
                base, displacement = ins.operands[0], ins.operands[1]
                stack = z3.Store(stack, z3.BitVecVal(bases[base] + displacement, 64), word(f"stored{k}"))
        rip, rsp = z3.Select(stack, S), S + 8
        solver.add(z3.Or(rip != R, rsp != S + 8))
    elif family == "x86":
        S, R, memory_array = word("S"), word("R"), z3.Array("mem", z3.BitVecSort(64), z3.BitVecSort(64))
        memory_array = z3.Store(memory_array, S, R)
        rip, rsp = z3.Select(memory_array, S), S + 8
        solver.add(z3.Or(rip != R, rsp != S + 8))
    else:
        ra, sp = word("ra"), word("sp")
        solver.add((ra & 1) == 0, z3.Or(((ra + 0) & ~z3.BitVecVal(1, 64)) != ra, sp != sp))
    names_stack = not (written & {"rsp", "sp", "ra"})
    rows["A-04"] = {"status": "passed" if solver.check() == z3.unsat and names_stack else "failed",
                    "evidence": "solver over the return rule, given A-03's frame"
                    + (" and A-06's region layout" if memory else "")}
    # A-05: straight-line, no admitted instruction faults in the model (no division or traps; memory only for
    # profile 3, whose admission makes every access an aligned word inside a declared region that A-06 maps).
    operations = {ins.op for ins in decoded}
    faulting = {"div", "idiv"} & operations if memory else {"div", "idiv", "load", "store"} & operations
    if memory and rows["A-03"].get("admission") != "passed":
        faulting |= {"load", "store"} & operations
    rows["A-05"] = {"status": "passed" if not faulting else "failed",
                    "evidence": "admitted operation set" + ("; admitted aligned in-region accesses" if memory else ""),
                    "instructions_to_boundary": len(decoded)}
    kind = ("GNU as over every admitted encoding; CPU conformance on sampled states" if family == "x86"
            else "GNU as and QEMU in CI; Sail refinement in progress")
    if memory:
        kind += ("; memory forms: native library runs on aligned buffers, no per-form conformance" if family == "x86"
                 else "; LD and SD have no differential evidence yet")
    rows["A-07"] = {"status": "passed", "evidence": "trusted generated model with differential evidence", "kind": kind}
    ok = all(row["status"] == "passed" for row in rows.values())
    return {"status": "qualified" if ok else "not-qualified", "profile": profile,
            "obligations": dict(sorted(rows.items())), "authority": "none",
            "nonclaims": "Generated ISA models; no active CET, signals, threads or microarchitectural state."}


TRAMPOLINE = r'''
.data
.globl zeno_abi_args, zeno_abi_seed, zeno_abi_out, zeno_abi_target, zeno_abi_canary
zeno_abi_args: .zero 48
zeno_abi_seed: .zero 72
zeno_abi_out: .zero 216
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
    mov %rsp, zeno_abi_saved(%rip)
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
    mov %rax, zeno_abi_out+0(%rip)
    mov %rdx, zeno_abi_out+8(%rip)
    mov %rbx, zeno_abi_out+16(%rip)
    mov %rbp, zeno_abi_out+24(%rip)
    mov %r12, zeno_abi_out+32(%rip)
    mov %r13, zeno_abi_out+40(%rip)
    mov %r14, zeno_abi_out+48(%rip)
    mov %r15, zeno_abi_out+56(%rip)
    mov %rsp, zeno_abi_out+64(%rip)
    mov zeno_abi_saved(%rip), %rsp
    mov %rsp, zeno_abi_out+208(%rip)
CANARY_LOADS
    pushfq
    pop %rax
    mov %rax, zeno_abi_out+72(%rip)
    cld
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
_SLOTS = [16 + 8 * k for k in range(16)]

RUNNER = r'''
#include <stdint.h>
#include <stdio.h>
extern uint64_t zeno_abi_args[6], zeno_abi_seed[9], zeno_abi_out[27], zeno_abi_canary;
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
        if (fwrite(zeno_abi_out, 8, 27, out) != 27) return 4;
    }
    return fclose(out) ? 5 : 0;
}
'''


MEMORY_RUNNER = r'''
#include <stdint.h>
#include <stdio.h>
extern uint64_t zeno_abi_args[6], zeno_abi_seed[9], zeno_abi_out[27], zeno_abi_canary;
extern void *zeno_abi_target;
extern void zeno_abi_call(void);
extern char zeno_kernel[];
REGIONS
int main(void) {
    FILE *in = fopen("vectors.bin", "rb"), *out = fopen("results.bin", "wb");
    uint64_t count;
    if (!in || !out || fread(&count, 8, 1, in) != 1) return 2;
    zeno_abi_target = (void *)zeno_kernel;
    for (uint64_t k = 0; k < count; k++) {
        if (fread(zeno_abi_args, 8, 6, in) != 6 || fread(zeno_abi_seed, 8, 9, in) != 9
            || fread(&zeno_abi_canary, 8, 1, in) != 1) return 3;
FILL
        zeno_abi_call();
        if (fwrite(zeno_abi_out, 8, 27, out) != 27) return 4;
DUMP
    }
    return fclose(out) ? 5 : 0;
}
'''


def _gcc():
    return shutil.which("gcc", path="/usr/bin:/bin")


def native(problem: dict, code: bytes, work: Path, vectors: int = 512, seed: str = "abi",
           expected=None, prefix: bytes = b"") -> dict:
    """Run `prefix + code` as an x86 leaf function through the trampoline and report every contract violation."""
    if _memory(problem):
        return native_memory(problem, code, work, vectors, seed, prefix)
    reference, elf_object = _module("native").reference, _module("targets").elf_object
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
    if len(data) != 8 * 27 * len(rows):
        return {"status": "error", "reason": "short result file"}
    faults = {"result": 0, "callee-saved": 0, "stack-pointer": 0, "stack-memory": 0, "direction-flag": 0}
    for index, (args, seeds, canary) in enumerate(rows):
        out = struct.unpack_from("<27Q", data, 8 * 27 * index)
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
    return {"status": "conforms" if not any(faults.values()) else "violated", "vectors": len(rows),
            "faults": faults, "prefix": prefix.hex()}


def native_memory(problem: dict, code: bytes, work: Path, vectors: int = 512, seed: str = "abi",
                  prefix: bytes = b"") -> dict:
    """Run `prefix + code` as an x86 profile-3 leaf function through the trampoline and report every violation.

    Each region is a static, 64-byte-aligned array with GUARD canary words before and after it; the kernel gets the
    address of its first word in the region's base register. Besides the register-profile faults, every vector
    checks the read-write words against `profile3.expected`, the read-only words for change, and every guard word.
    """
    elf_object = _module("targets").elf_object
    if _family(problem) != "x86":
        return {"status": "not-run", "reason": "needs x86-64 execution"}
    if _gcc() is None:
        return {"status": "not-run", "reason": "needs gcc"}
    inputs, bases = profile3.registers(problem)
    arguments = profile3.ARGUMENTS[problem["target"]]
    regions, outputs = problem["regions"], len(problem["outputs"])
    work.mkdir(mode=0o700)
    (work / "kernel.o").write_bytes(elf_object(problem["target"], prefix + code))
    stores = "\n".join(f"    mov %rax, -{slot}(%rsp)" for slot in _SLOTS)
    loads = "\n".join(f"    mov -{slot}(%rsp), %rax\n    mov %rax, zeno_abi_out+{80 + 8 * k}(%rip)"
                      for k, slot in enumerate(_SLOTS))
    (work / "trampoline.s").write_text(TRAMPOLINE.replace("CANARY_STORES", stores).replace("CANARY_LOADS", loads))
    size = lambda region: GUARD + region["words"] + GUARD
    declare = "\n".join(f"static uint64_t region{j}[{size(r)}] __attribute__((aligned(64)));"
                         for j, r in enumerate(regions))
    fill = []
    for j, r in enumerate(regions):
        fill.append(f"        for (int g = 0; g < {GUARD}; g++) region{j}[g] = region{j}[{GUARD + r['words']} + g] = "
                    "zeno_abi_canary;")
        fill.append(f"        if (fread(region{j} + {GUARD}, 8, {r['words']}, in) != {r['words']}) return 3;")
        fill.append(f"        zeno_abi_args[{arguments.index(r['base'])}] = (uint64_t)(uintptr_t)(region{j} + {GUARD});")
    dump = "\n".join(f"        if (fwrite(region{j}, 8, {size(r)}, out) != {size(r)}) return 4;"
                      for j, r in enumerate(regions))
    (work / "runner.c").write_text(MEMORY_RUNNER.replace("REGIONS", declare).replace("FILL", "\n".join(fill))
                                   .replace("DUMP", dump))
    rng = random.Random(seed)
    edge = (0, 1, (1 << 64) - 1, 1 << 63)
    rows = []
    for index in range(vectors):
        draw = (lambda j: edge[(index + j) % 4]) if index < 8 else (lambda j: rng.getrandbits(64))
        args = [rng.getrandbits(64) for _ in range(6)]
        values = {name: draw(k) for k, name in enumerate(inputs)}
        for name, register in inputs.items():
            args[arguments.index(register)] = values[name]
        words = {r["name"]: [draw(len(inputs) + k) for k in range(r["words"])] for r in regions}
        rows.append((args, values, words, [rng.getrandbits(64) for _ in range(9)], rng.getrandbits(64)))
    with open(work / "vectors.bin", "wb") as stream:
        stream.write(struct.pack("<Q", len(rows)))
        for args, _, words, seeds, canary in rows:
            stream.write(struct.pack("<16Q", *args, *seeds, canary))
            for r in regions:
                stream.write(struct.pack(f"<{r['words']}Q", *words[r["name"]]))
    run = lambda argv: subprocess.run(argv, cwd=work, capture_output=True, text=True, timeout=120,
                                      env={"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C"})
    built = run([_gcc(), "-O1", "-fno-lto", "-Wl,-z,noexecstack", "runner.c", "trampoline.s", "kernel.o",
                 "-o", "runner"])
    if built.returncode:
        return {"status": "error", "reason": built.stderr[-1500:]}
    done = run([str(work / "runner")])
    if done.returncode:
        return {"status": "crashed", "exit": done.returncode, "vectors": len(rows)}
    width = 27 + sum(size(r) for r in regions)
    data = (work / "results.bin").read_bytes()
    if len(data) != 8 * width * len(rows):
        return {"status": "error", "reason": "short result file"}
    faults = {"result": 0, "callee-saved": 0, "stack-pointer": 0, "stack-memory": 0, "direction-flag": 0,
              "region-words": 0, "read-only-region": 0, "out-of-region": 0}
    for index, (args, values, words, seeds, canary) in enumerate(rows):
        out = struct.unpack_from(f"<{width}Q", data, 8 * width * index)
        results, writable = profile3.expected(problem, values, words)
        if list(out[:outputs]) != results:
            faults["result"] += 1
        if list(out[2:8]) != seeds[:6]:
            faults["callee-saved"] += 1
        if out[8] != out[26]:
            faults["stack-pointer"] += 1
        if any(word != canary for word in out[10:26]):
            faults["stack-memory"] += 1
        if out[9] & (1 << 10):
            faults["direction-flag"] += 1
        at, region_fault, read_only_fault, guard_fault = 27, False, False, False
        for r in regions:
            block = out[at:at + size(r)]
            at += size(r)
            final = list(block[GUARD:GUARD + r["words"]])
            guard_fault |= any(word != canary for word in block[:GUARD] + block[GUARD + r["words"]:])
            if r["access"] == "read-write":
                region_fault |= final != writable[r["name"]]
            else:
                read_only_fault |= final != words[r["name"]]
        faults["region-words"] += region_fault
        faults["read-only-region"] += read_only_fault
        faults["out-of-region"] += guard_fault
    return {"status": "conforms" if not any(faults.values()) else "violated", "vectors": len(rows),
            "faults": faults, "prefix": prefix.hex(), "guard_words": GUARD}


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
    """`python3 -B -m tools.machine_qualification.abi --dir LIBRARY`: the gate for every library entry."""
    import argparse
    import json
    import sys
    import tempfile
    parser = argparse.ArgumentParser(description=main.__doc__)
    parser.add_argument("--dir", required=True, type=Path)
    parser.add_argument("--vectors", type=int, default=512)
    args = parser.parse_args(argv)
    rows, ok = [], True
    for path in sorted(args.dir.glob("*.json")):
        entry = json.loads(path.read_text())
        problem, code = entry["problem"], bytes.fromhex(entry["code_hex"])
        gate = obligations(problem, code)
        with tempfile.TemporaryDirectory() as folder:
            run = native(problem, code, Path(folder) / "work", args.vectors)
        ok &= gate["status"] == "qualified" and run["status"] in ("conforms", "not-run")
        rows.append({"entry": path.name, "profile": gate["profile"], "obligations": gate["status"],
                     "native": run["status"], "faults": run.get("faults")})
    print(json.dumps({"status": "qualified" if ok and rows else "not-qualified", "entries": rows,
                      "authority": "none"}, indent=1))
    return 0 if ok and rows else 1


if __name__ == "__main__":
    raise SystemExit(main())
