"""Independently execute both exact Zeno x86 signed-saturating-add kernels.

Finite native testing and an unbounded Python integer oracle are separate from
the Zeno machine-model SMT checker; neither establishes Cranelift savings.
"""
from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import platform
import random
import subprocess
import tempfile
from pathlib import Path

OLD = bytes.fromhex("4889fa48c1fa3f48b9ffffffffffffff7f4831ca4889f84801f0480f40c2c3")
NEW = bytes.fromhex("4883c8ff48d1e84839f84883d0004801f7480f41c7c3")
MIN, MAX = -(1 << 63), (1 << 63) - 1
SEED = 0xC0FFEE


def expected(x: int, y: int) -> int:
    return max(MIN, min(MAX, x + y))


def run(out: Path) -> dict:
    if platform.machine().lower() not in ("x86_64", "amd64"):
        raise RuntimeError("x86-64 required")
    if out.exists():
        raise ValueError("fresh --out directory required")
    out.mkdir(parents=True, mode=0o700)
    with tempfile.TemporaryDirectory(prefix="tier1-native-") as name:
        work = Path(name)
        asm = [".text"]
        for symbol, code in (("zeno_old", OLD), ("zeno_new", NEW)):
            asm += [f".globl {symbol}", f".type {symbol},@function", f"{symbol}:",
                    ".byte " + ",".join(str(x) for x in code),
                    f".size {symbol},.-{symbol}"]
        (work / "kernels.S").write_text("\n".join(asm) + "\n")
        (work / "baseline.c").write_text("""#include <stdint.h>
#include <limits.h>
__attribute__((noinline)) int64_t gcc_baseline(int64_t x, int64_t y) {
    __int128 z = (__int128)x + (__int128)y;
    if (z > INT64_MAX) return INT64_MAX;
    if (z < INT64_MIN) return INT64_MIN;
    return (int64_t)z;
}
""")
        subprocess.run(["gcc", "-O2", "-fcf-protection=none", "-fno-stack-protector",
                        "-shared", "-fPIC", str(work / "kernels.S"),
                        str(work / "baseline.c"), "-o", str(work / "trial.so")],
                       check=True, capture_output=True, text=True)
        dump = subprocess.run(["objdump", "-d", "-Mintel", str(work / "trial.so")],
                              check=True, capture_output=True, text=True).stdout
        (out / "disassembly.txt").write_text(dump)
        dll = ctypes.CDLL(str(work / "trial.so"))
        functions = {}
        for symbol in ("zeno_old", "zeno_new", "gcc_baseline"):
            fn = getattr(dll, symbol)
            fn.argtypes = [ctypes.c_int64, ctypes.c_int64]
            fn.restype = ctypes.c_int64
            functions[symbol] = fn
        edge = [MIN, MIN + 1, MIN + 2, -(1 << 62), -256, -2, -1, 0, 1, 2,
                3, 255, 256, (1 << 62), MAX - 2, MAX - 1, MAX]
        vectors = [(x, y) for x in edge for y in edge]
        randomizer = random.Random(SEED)
        for _ in range(20000):
            a, b = randomizer.getrandbits(64), randomizer.getrandbits(64)
            vectors.append((a if a <= MAX else a - (1 << 64),
                            b if b <= MAX else b - (1 << 64)))
        failures = {k: [] for k in functions}
        for x, y in vectors:
            oracle = expected(x, y)
            for symbol, fn in functions.items():
                actual = fn(x, y)
                if actual != oracle:
                    failures[symbol].append([x, y, oracle, actual])
                    if len(failures[symbol]) > 20:
                        raise RuntimeError("too many native mismatches")
        report = {
            "schema": "zeno-fcis/tier1-native-finite/1",
            "zenofcis_base_commit": "31afaef8d9fb723b44918f430e10d63f0cb93ccf",
            "target": "x86_64-sysv-register/2",
            "old": {"bytes": len(OLD), "sha256": hashlib.sha256(OLD).hexdigest()},
            "new": {"bytes": len(NEW), "sha256": hashlib.sha256(NEW).hexdigest()},
            "compiler": subprocess.run(["gcc", "--version"], capture_output=True,
                                       text=True).stdout.splitlines()[0],
            "host": platform.platform(),
            "input_cases": len(vectors),
            "vectors": {"edge_pairs": len(edge)**2, "random_pairs": 20000, "seed": SEED},
            "failures": failures,
            "status": "pass" if not any(failures.values()) else "fail",
            "nonclaims": ["Finite input testing, not SMT proof", "No Cranelift emitted byte comparison",
                          "No global target equivalence"]
        }
        (out / "native-check.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
        return report


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    result = run(args.out)
    print(json.dumps({"status": result["status"], "input_cases": result["input_cases"],
                      "raw_bytes_old": result["old"]["bytes"],
                      "raw_bytes_new": result["new"]["bytes"],
                      "cranelift_comparison": "not measured"}, sort_keys=True))
    if result["status"] != "pass":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
