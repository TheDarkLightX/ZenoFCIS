#!/usr/bin/env python3
"""Independently replay the pinned Tier-1 umin CI artifact (x86-64 Linux).

Verifies original ZIP digest, source pins, ELF .text and equal-size kernels,
then executes actual linked full Cranelift object, extracted compiler core,
and Zeno kernel on independently generated unsigned input pairs.
This is a finite runtime audit, NOT a compiler/state/flags proof or new saving.
"""
import argparse
import ctypes
import hashlib
import json
import platform
from pathlib import Path
import random
import subprocess
import sys
import tempfile
import zipfile

WASMTIME = "e355dc640d047ab76dbf4402567d0c124bfdecd6"
ZENOF = "82f1bfd9559136f73bb2127250abeba98a13fc44"
FULL = bytes.fromhex("554889e5483bf74889f8480f42c64889ec5dc3")
CLIF = bytes.fromhex("483bf74889f8480f42c6c3")
ZENOK = bytes.fromhex("4839f74889f0480f42c7c3")


def run(args):
    if sys.flags.optimize:
        raise RuntimeError("Replay forbids Python -O: required assertions would be stripped")
    assert platform.machine().lower() in {"x86_64", "amd64"}
    assert 0 <= args.random_pairs <= 1000000
    archive = args.artifact.read_bytes()
    assert hashlib.sha256(archive).hexdigest() == args.zip_sha256, "artifact digest differs from GitHub"
    with tempfile.TemporaryDirectory(prefix="tier1-full-compiler-") as directory:
        root = Path(directory)
        with zipfile.ZipFile(args.artifact) as z:
            names = z.namelist()
            assert len(names) == len(set(names))
            assert all(not Path(n).is_absolute() and ".." not in Path(n).parts for n in names)
            z.extractall(root)
        catalog = json.loads((root / "catalog.json").read_text())
        assert catalog["upstream"] == WASMTIME == (root / "wasmtime-source-commit.txt").read_text().strip()
        assert catalog["experiment"] == ZENOF == (root / "zenofcis-source-commit.txt").read_text().strip()
        assert catalog["search_class"] == "rediscovery"
        assert catalog["compiled_body_refinement"]["exact_compiled_model"]["status"] == "unsat"
        assert catalog["costs"]["compiler_core_vs_zeno_core_bytes_saved"] == 0
        assert (root / "artifact/kernel.bin").read_bytes() == ZENOK
        for key, path in {"clif_sha256": "umin.clif", "clif_object_sha256": "umin-clif.o",
                          "machine_code_sha256": "artifact/kernel.bin"}.items():
            assert hashlib.sha256((root / path).read_bytes()).hexdigest() == catalog["source_receipts"][key]
        subprocess.run(["objcopy", "--dump-section", ".text=" + str(root / "text.bin"),
                        str(root / "umin-clif.o")], check=True)
        actual = (root / "text.bin").read_bytes()
        assert actual == FULL, "compiled machine bytes changed; refuse to replay different semantics"
        assert actual[4:-5] + b"\xc3" == CLIF
        assert len(CLIF) == len(ZENOK) == 11
        assert hashlib.sha256(CLIF).hexdigest() == catalog["compiled_body_refinement"]["core_sha256"]

        subjects = [("full-cranelift", "umin-clif.o", "%tier1_umin"),
                    ("zenofcis-kernel", "artifact/kernel.S", "zeno_kernel"),
                    ("cranelift-core", "compiled-core/kernel.S", "zeno_kernel")]
        handles, functions = [], []
        for index, (_, source, symbol) in enumerate(subjects):
            output = root / ("recompiled-" + str(index) + ".so")
            subprocess.run(["gcc", "-shared", "-fPIC", "-o", str(output), str(root / source)], check=True)
            handle = ctypes.CDLL(str(output))
            fn = getattr(handle, symbol)
            fn.argtypes = [ctypes.c_uint64, ctypes.c_uint64]
            fn.restype = ctypes.c_uint64
            handles.append(handle)
            functions.append(fn)
        edges = [0, 1, 2, 3, 255, 256, 65535, 65536, (1 << 32)-1,
                 1 << 32, (1 << 63)-1, 1 << 63, (1 << 63)+1,
                 (1 << 64)-2, (1 << 64)-1]
        rng = random.Random(20261120)
        pairs = [(x, y) for x in edges for y in edges]
        pairs.extend((rng.getrandbits(64), rng.getrandbits(64)) for _ in range(args.random_pairs))
        for x, y in pairs:
            expected = min(x, y)
            for label, fn in zip((s[0] for s in subjects), functions):
                assert fn(x, y) == expected, (label, x, y)
        print(json.dumps({"status": "native-finite-replay-passed", "source_pins": [ZENOF, WASMTIME],
                          "original_zip_sha256": args.zip_sha256,
                          "cases_per_subject": len(pairs), "native_calls": 3 * len(pairs),
                          "subjects": [s[0] for s in subjects],
                          "decoded_full_function_bytes": len(actual),
                          "compiler_arithmetic_core_bytes_including_ret": len(CLIF),
                          "zenofcis_kernel_bytes_including_ret": len(ZENOK),
                          "established_core_byte_saving": 0,
                          "novelty": "rediscovery",
                          "limitations": "finite SysV return-value replay, not proof of complete flags/ABI/unwind/poison or throughput"},
                         sort_keys=True, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("artifact", type=Path)
    parser.add_argument("--zip-sha256", required=True)
    parser.add_argument("--random-pairs", type=int, default=100000)
    run(parser.parse_args())
