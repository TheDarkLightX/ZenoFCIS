#!/usr/bin/env python3
"""Independently replay archived x86-64 forward i64 div/rem A/B fixture.

Requires x86-64 Linux capable of running both archived Wasmtime executables.
This is only 76 finite guest executions; not a proof of compiler-pass soundness,
machine flags, ABI, trap locations, or full 64-bit state refinement.
"""
import argparse
import hashlib
import json
import pathlib
import random
import subprocess
import tempfile
import zipfile

EXPECTED_ZIP_SHA256 = "144fa027f2fb7207792f7c535158fb059e47191314a944f7328cfbb71bb1473d"
MOD = 1 << 64
MASK = MOD - 1


def replay(archive):
    archive = pathlib.Path(archive)
    if hashlib.sha256(archive.read_bytes()).hexdigest() != EXPECTED_ZIP_SHA256:
        raise AssertionError("archive ZIP digest differs from pinned original")
    with zipfile.ZipFile(archive) as z:
        q = json.loads(z.read("context-controls/qualification.json"))
        wbytes = z.read("context-controls/adjacent/input.wat")
        if hashlib.sha256(wbytes).hexdigest() != q["cases"]["adjacent"]["wat_sha256"]:
            raise AssertionError("WAT digest mismatch")
        rng = random.Random(20261120)
        xs = [0, 1, 2, 3, 127, 128, 255, 256, 65535, (1 << 31)-1, 1 << 31,
              (1 << 32)-1, 1 << 32, (1 << 63)-1, 1 << 63, MASK]
        xs += [rng.getrandbits(64) for _ in range(16)]
        ys = [0, 1, 2, 3, 7, 9, 13, 255, 256, 65535, (1 << 31)-1, 1 << 31,
              1 << 32, (1 << 63)-1, 1 << 63, MASK]
        vectors = [(x, ys[(i * 5) % len(ys)]) for i, x in enumerate(xs)]
        vectors += [(MASK, 0), (0, 0), (1, 0), (1, 1), (MASK, 1), (MASK, MASK)]
        counters = {"value": 0, "trap": 0}
        with tempfile.TemporaryDirectory(prefix="tier1-replay-") as d:
            d = pathlib.Path(d)
            wat = d / "adjacent.wat"
            wat.write_bytes(wbytes)
            for mode in ("baseline", "patched"):
                executable = d / ("wasmtime-" + mode)
                contents = z.read(mode + "/wasmtime")
                expected_hash = z.read(mode + "-binary.sha256").decode()
                if hashlib.sha256(contents).hexdigest() not in expected_hash:
                    raise AssertionError("binary SHA-256 mismatch " + mode)
                executable.write_bytes(contents)
                executable.chmod(0o700)
                for x, y in vectors:
                    xarg = x if x < (1 << 63) else x-MOD
                    yarg = y if y < (1 << 63) else y-MOD
                    cmd = [str(executable), "--invoke", "work", str(wat), str(xarg), str(yarg)]
                    proc = subprocess.run(cmd, text=True, capture_output=True, timeout=5)
                    if y == 0:
                        assert proc.returncode != 0 and "integer divide by zero" in proc.stderr, (mode, x, y)
                        counters["trap"] += 1
                    else:
                        quotient, rem = divmod(x, y)
                        rotated = ((rem << 17) | (rem >> 47)) & MASK
                        expected = quotient ^ rotated
                        actual = int(proc.stdout.strip().splitlines()[-1]) & MASK
                        assert proc.returncode == 0 and actual == expected, (mode, x, y, actual, expected)
                        counters["value"] += 1
    return {"result": "passed-finite-independent-replay", "compiler_binaries": 2,
            "vectors_per_compiler": len(vectors), "executions": sum(counters.values()),
            **counters, "claim_limit": "adjacent fixture only; no total compiler or machine proof"}


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("archive", help="original CI artifact ZIP #14603308612")
    args = ap.parse_args()
    print(json.dumps(replay(args.archive), indent=2))
