"""Native execution of checked x86 kernel bytes against the independent integer reference.

This executes; it never decides equivalence. The kernel's ELF object is rebuilt from its bytes and linked with a
generated C runner, which calls it on edge and deterministic pseudo-random argument vectors. Every result word is
compared with the problem's integer reference: `model.evaluate` for profile 1 and `verify2._ast_concrete` for
profile 2. Agreement covers the vectors run on this processor only; it is not a proof, and RV64I code is not run.

A profile-3 x86 kernel runs only after its bytes pass profile-3 admission, so every access is an aligned word
inside its region. Each region is its own 64-byte-aligned static array, so regions are disjoint and apart from
the stack. Per state the runner loads the initial words, calls the kernel with each region's address in its base
register, and writes back the results and every region word. Read-write words are compared with the problem's
expected final contents (`profile3.expected`), and read-only words must come back unchanged.
"""

from __future__ import annotations

from itertools import product
from pathlib import Path
import platform
import random
import shutil
import struct
import subprocess

from . import profile3
from .model import MASK, evaluate
from .targets import elf_object

MEMORY_X86 = profile3.TARGETS[0]
X86_TARGETS = ("x86_64-sysv-register/1", "x86_64-sysv-register/2", MEMORY_X86)
EDGE_STATES = 1024
EDGE = (0, 1, 2, MASK, 1 << 63, (1 << 63) - 1, MASK - 1, 195)
RANDOM_VECTORS = 4096
MAX_EDGE_VECTORS = 4096
NOT_RV64I = {"status": "not-run", "reason": "needs RV64I execution"}
SCOPE = ("Native results compare every result word of each x86 kernel, run on this processor, with the problem's "
         "integer reference on the listed vectors; for memory kernels also every region word, on aligned, disjoint "
         "static buffers. Not a proof, not timing, and RV64I code is not run.")

RUNNER = r'''
#include <stdint.h>
#include <stdio.h>
typedef struct { uint64_t r0, r1; } pair;
extern RESULT zeno_kernel(PARAMETERS);
int main(void) {
    FILE *in = fopen("vectors.bin", "rb"), *out = fopen("results.bin", "wb");
    uint64_t count, a[ARITY];
    if (!in || !out || fread(&count, 8, 1, in) != 1) return 2;
    for (uint64_t k = 0; k < count; k++) {
        if (fread(a, 8, ARITY, in) != ARITY) return 3;
        RESULT r = zeno_kernel(ARGUMENTS);
        uint64_t words[2] = {WORDS};
        if (fwrite(words, 8, OUTPUTS, out) != OUTPUTS) return 4;
    }
    return fclose(out) ? 5 : 0;
}
'''


def unsupported_host():
    """The reason this host cannot run x86 kernels natively, or None."""
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        return "needs a Linux x86-64 host"
    if shutil.which("gcc", path="/usr/bin:/bin") is None:
        return "needs gcc on this host"
    return None


def reference(problem: dict):
    """(argument count, result count, integer reference) for a profile-1 or profile-2 problem."""
    if "outputs" not in problem:
        return 2, 1, lambda vector: [evaluate(problem["expression"], *vector)]
    from .verify2 import _ast_concrete
    names, outputs = problem["inputs"], problem["outputs"]
    return len(names), len(outputs), lambda vector: [_ast_concrete(o, dict(zip(names, vector))) for o in outputs]


def vectors(arity: int, seed: str) -> tuple:
    """(edge vectors, pseudo-random vectors); all edge combinations when there are at most 4,096."""
    if len(EDGE) ** arity <= MAX_EDGE_VECTORS:
        edge = list(product(EDGE, repeat=arity))
    else:
        picker = random.Random("edge:" + seed)
        edge = [tuple(picker.choice(EDGE) for _ in range(arity)) for _ in range(MAX_EDGE_VECTORS)]
    rng = random.Random("random:" + seed)
    return edge, [tuple(rng.getrandbits(64) for _ in range(arity)) for _ in range(RANDOM_VECTORS)]


def _run(arguments, cwd):
    result = subprocess.run(arguments, cwd=cwd, capture_output=True, text=True, timeout=120, check=False,
                            env={"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C"})
    if result.returncode:
        raise ValueError(f"{Path(arguments[0]).name} exited {result.returncode}: {result.stderr[-1500:]}")


def memory_states(problem: dict, seed: str) -> tuple:
    """(seed states, edge states, pseudo-random states), each a list of (inputs, initial region words)."""
    from .session import _memory_seeds
    seeds = [(tuple(inputs), {name: list(words) for name, words in memory.items()})
             for inputs, _, _, _, memory in _memory_seeds(problem)]
    count = len(problem["inputs"]) + sum(region["words"] for region in problem["regions"])

    def state(draw):
        values = [draw() for _ in range(count)]
        inputs, rest, regions = tuple(values[:len(problem["inputs"])]), values[len(problem["inputs"]):], {}
        for region in problem["regions"]:
            regions[region["name"]], rest = rest[:region["words"]], rest[region["words"]:]
        return inputs, regions

    picker, rng = random.Random("edge:" + seed), random.Random("random:" + seed)
    edge = [state(lambda: picker.choice(EDGE)) for _ in range(EDGE_STATES)]
    return seeds, edge, [state(lambda: rng.getrandbits(64)) for _ in range(RANDOM_VECTORS)]


def _memory_runner(problem: dict) -> str:
    """C runner: static, aligned, disjoint region arrays and the kernel's argument registers in order."""
    inputs, bases = profile3.registers(problem)
    arguments = profile3.ARGUMENTS[problem["target"]]
    regions = problem["regions"]
    passed = {register: f"a[{k}]" for k, register in enumerate(inputs.values())}
    passed.update({region["base"]: f"(uint64_t)(uintptr_t)region{j}" for j, region in enumerate(regions)})
    count = 1 + max(arguments.index(register) for register in passed)
    outputs = len(problem["outputs"])
    lines = ["#include <stdint.h>", "#include <stdio.h>", "typedef struct { uint64_t r0, r1; } pair;",
             f"extern {'pair' if outputs == 2 else 'uint64_t'} zeno_kernel({', '.join(['uint64_t'] * count)});"]
    lines += [f"static uint64_t region{j}[{region['words']}] __attribute__((aligned(64)));"
              for j, region in enumerate(regions)]
    lines += ["int main(void) {", '    FILE *in = fopen("vectors.bin", "rb"), *out = fopen("results.bin", "wb");',
              f"    uint64_t count, a[{len(inputs) + 1}];",
              "    if (!in || !out || fread(&count, 8, 1, in) != 1) return 2;",
              "    for (uint64_t k = 0; k < count; k++) {"]
    if inputs:
        lines.append(f"        if (fread(a, 8, {len(inputs)}, in) != {len(inputs)}) return 3;")
    lines += [f"        if (fread(region{j}, 8, {region['words']}, in) != {region['words']}) return 3;"
              for j, region in enumerate(regions)]
    call = ", ".join(passed.get(arguments[k], "0") for k in range(count))
    lines.append(f"        {'pair' if outputs == 2 else 'uint64_t'} r = zeno_kernel({call});")
    lines.append(f"        uint64_t words[2] = {{{'r.r0, r.r1' if outputs == 2 else 'r, 0'}}};")
    if outputs:
        lines.append(f"        if (fwrite(words, 8, {outputs}, out) != {outputs}) return 4;")
    else:
        lines.append("        (void)words;")
    lines += [f"        if (fwrite(region{j}, 8, {region['words']}, out) != {region['words']}) return 4;"
              for j, region in enumerate(regions)]
    lines += ["    }", "    return fclose(out) ? 5 : 0;", "}"]
    return "\n".join(lines) + "\n"


def _execute_memory(problem: dict, code: bytes, work: Path, seed: str) -> dict:
    """Run an admitted profile-3 x86 kernel on real buffers and compare results and every region word."""
    from . import verify3
    try:
        verify3.admit(problem, verify3.decode(problem["target"], code))
    except ValueError as error:
        return {"status": "not-run", "reason": "memory access not admitted: " + str(error)}
    seeds, edge, pseudo = memory_states(problem, seed)
    rows = seeds + edge + pseudo
    work.mkdir(mode=0o700)
    (work / "kernel.o").write_bytes(elf_object(problem["target"], code))
    (work / "runner.c").write_text(_memory_runner(problem))
    with open(work / "vectors.bin", "wb") as stream:
        stream.write(struct.pack("<Q", len(rows)))
        for inputs, regions in rows:
            words = [*inputs, *(w for region in problem["regions"] for w in regions[region["name"]])]
            stream.write(struct.pack(f"<{len(words)}Q", *words))
    try:
        _run([shutil.which("gcc", path="/usr/bin:/bin"), "-std=c11", "-O2", "-fno-lto", "-Wl,-z,noexecstack",
              "runner.c", "kernel.o", "-o", "runner"], work)
        _run([str(work / "runner")], work)
    except (ValueError, OSError, subprocess.TimeoutExpired) as error:
        return {"status": "error", "reason": str(error)[:2000], "vectors": len(rows)}
    outputs = len(problem["outputs"])
    width = outputs + sum(region["words"] for region in problem["regions"])
    data = (work / "results.bin").read_bytes()
    if len(data) != 8 * width * len(rows):
        return {"status": "error", "reason": "runner wrote a short result file", "vectors": len(rows)}
    mismatches, first = 0, None
    for index, (inputs, regions) in enumerate(rows):
        words = list(struct.unpack_from(f"<{width}Q", data, 8 * width * index))
        actual, rest = words[:outputs], words[outputs:]
        final = {}
        for region in problem["regions"]:
            final[region["name"]], rest = rest[:region["words"]], rest[region["words"]:]
        results, writable = profile3.expected(problem, dict(zip(problem["inputs"], inputs)), regions)
        wanted = {region["name"]: writable.get(region["name"], regions[region["name"]]) for region in problem["regions"]}
        if actual != results or final != wanted:
            mismatches += 1
            if first is None:
                text = lambda values: [str(v) for v in values]
                first = {"inputs": dict(zip(problem["inputs"], text(inputs))),
                         "initial_regions": {n: text(v) for n, v in regions.items()},
                         "expected": text(results), "actual": text(actual),
                         "expected_regions": {n: text(v) for n, v in wanted.items()},
                         "actual_regions": {n: text(v) for n, v in final.items()}}
    result = {"status": "agreed" if mismatches == 0 else "mismatch", "vectors": len(rows), "seed_vectors": len(seeds),
              "edge_vectors": len(edge), "random_vectors": len(pseudo),
              "region_words": sum(region["words"] for region in problem["regions"]),
              "read_only_regions": [r["name"] for r in problem["regions"] if r["access"] == "read-only"]}
    if mismatches:
        result.update(mismatches=mismatches, first_mismatch=first)
    return result


def execute(problem: dict, code: bytes, work: Path, seed: str) -> dict:
    """Link the rebuilt object, run every vector, and compare each result word with the integer reference."""
    if problem["target"] not in X86_TARGETS:
        return dict(NOT_RV64I)
    if problem["target"] == MEMORY_X86:
        return _execute_memory(problem, code, work, seed)
    arity, outputs, expected = reference(problem)
    edge, pseudo = vectors(arity, seed)
    rows = edge + pseudo
    work.mkdir(mode=0o700)
    (work / "kernel.o").write_bytes(elf_object(problem["target"], code))
    pair = outputs == 2
    fields = {"RESULT": "pair" if pair else "uint64_t", "PARAMETERS": ", ".join(["uint64_t"] * arity),
              "ARGUMENTS": ", ".join(f"a[{k}]" for k in range(arity)), "WORDS": "r.r0, r.r1" if pair else "r, 0",
              "OUTPUTS": str(outputs), "ARITY": str(arity)}
    runner = RUNNER
    for key in sorted(fields, key=len, reverse=True):
        runner = runner.replace(key, fields[key])
    (work / "runner.c").write_text(runner)
    with open(work / "vectors.bin", "wb") as stream:
        stream.write(struct.pack("<Q", len(rows)))
        for row in rows:
            stream.write(struct.pack(f"<{arity}Q", *row))
    try:
        _run([shutil.which("gcc", path="/usr/bin:/bin"), "-std=c11", "-O2", "-fno-lto", "-Wl,-z,noexecstack",
              "runner.c", "kernel.o", "-o", "runner"], work)
        _run([str(work / "runner")], work)
    except (ValueError, OSError, subprocess.TimeoutExpired) as error:
        return {"status": "error", "reason": str(error)[:2000], "vectors": len(rows)}
    data = (work / "results.bin").read_bytes()
    if len(data) != 8 * outputs * len(rows):
        return {"status": "error", "reason": "runner wrote a short result file", "vectors": len(rows)}
    mismatches, first = 0, None
    for index, row in enumerate(rows):
        actual = list(struct.unpack_from(f"<{outputs}Q", data, 8 * outputs * index))
        wanted = expected(row)
        if actual != wanted:
            mismatches += 1
            if first is None:
                first = {"arguments": [str(v) for v in row], "expected": [str(v) for v in wanted],
                         "actual": [str(v) for v in actual]}
    result = {"status": "agreed" if mismatches == 0 else "mismatch", "vectors": len(rows),
              "edge_vectors": len(edge), "random_vectors": len(pseudo)}
    if mismatches:
        result.update(mismatches=mismatches, first_mismatch=first)
    return result
