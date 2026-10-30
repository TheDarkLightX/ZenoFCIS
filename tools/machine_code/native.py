"""Native execution of checked x86 kernel bytes against the independent integer reference.

This executes; it never decides equivalence. The kernel's ELF object is rebuilt from its bytes and linked with a
generated C runner, which calls it on edge and deterministic pseudo-random argument vectors. Every result word is
compared with the problem's integer reference: `model.evaluate` for profile 1 and `verify2._ast_concrete` for
profile 2. Agreement covers the vectors run on this processor only; it is not a proof, and RV64I code is not run.
"""

from __future__ import annotations

from itertools import product
from pathlib import Path
import platform
import random
import shutil
import struct
import subprocess

from .model import MASK, evaluate
from .targets import elf_object

X86_TARGETS = ("x86_64-sysv-register/1", "x86_64-sysv-register/2")
EDGE = (0, 1, 2, MASK, 1 << 63, (1 << 63) - 1, MASK - 1, 195)
RANDOM_VECTORS = 4096
MAX_EDGE_VECTORS = 4096
NOT_RV64I = {"status": "not-run", "reason": "needs RV64I execution"}
SCOPE = ("Native results compare every result word of each x86 kernel, run on this processor, with the problem's "
         "integer reference on the listed vectors. Not a proof, not timing, and RV64I code is not run.")

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


def execute(problem: dict, code: bytes, work: Path, seed: str) -> dict:
    """Link the rebuilt object, run every vector, and compare each result word with the integer reference."""
    if problem["target"] not in X86_TARGETS:
        return dict(NOT_RV64I)
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
