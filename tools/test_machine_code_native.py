"""Actual Linux x86-64 conformance; GNU tools are independent encoding oracles."""

import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import unittest

from machine_code.model import canonical, digest, TARGETS
from machine_code.targets import encode_program, elf_object
from machine_code.verify import check


C_RUNNER = r'''
#include <stdint.h>
#include <inttypes.h>
#include <stdio.h>
extern uint64_t zeno_kernel(uint64_t, uint64_t);
static uint64_t expected(uint64_t x, uint64_t y) { return RETURN_EXPR; }
static int one(uint64_t x, uint64_t y) {
    uint64_t got = zeno_kernel(x,y), want = expected(x,y);
    if (got == want) return 0;
    fprintf(stderr,"mismatch x=%" PRIu64 " y=%" PRIu64 " got=%" PRIu64 " expected=%" PRIu64 "\n",x,y,got,want);
    return 1;
}
int main(void) {
    uint64_t edge[] = {0,1,2,UINT64_MAX,UINT64_C(1)<<63,(UINT64_C(1)<<63)-1,UINT64_MAX-1,195};
    for (unsigned i=0;i<8;i++) for(unsigned j=0;j<8;j++) if(one(edge[i],edge[j])) return 1;
    uint64_t state = UINT64_C(0x9e3779b97f4a7c15);
    for (unsigned i=0;i<2048;i++) {
        state = state * UINT64_C(6364136223846793005) + UINT64_C(1442695040888963407);
        uint64_t x=state;
        state = state * UINT64_C(6364136223846793005) + UINT64_C(1442695040888963407);
        if(one(x,state)) return 1;
    }
    puts("native-conformance: 2112 checked pairs");
    return 0;
}
'''


def asm_instruction(instruction):
    op, *args = instruction
    if op == "lea":
        return f"lea {args[0]}, [{args[1]} + {args[2]}*{args[3]}]"
    if op == "imm":
        return f"movabs {args[0]}, {args[1]}"
    return op + (" " + ", ".join(map(str, args)) if args else "")


@unittest.skipUnless(platform.system() == "Linux" and platform.machine() == "x86_64",
                     "actual Linux x86-64 native profile required")
class NativeMachineTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.commands = {}
        for name in ("gcc", "as", "objcopy", "readelf", "nm"):
            path = shutil.which(name, path="/usr/bin:/bin")
            if path is None:
                raise RuntimeError("required GNU conformance tool is missing: " + name)
            cls.commands[name] = path
        retained = os.environ.get("ZENO_MACHINE_NATIVE_OUT")
        cls.temporary = None if retained else tempfile.TemporaryDirectory(prefix="zeno-machine-native-")
        cls.root = Path(retained) if retained else Path(cls.temporary.name)
        if retained:
            cls.root.mkdir(mode=0o700)
        cls.results = []

    @classmethod
    def tearDownClass(cls):
        (cls.root / "results.json").write_bytes(canonical(cls.results) + b"\n")
        if cls.temporary:
            cls.temporary.cleanup()

    def run_tool(self, arguments, cwd):
        env = {"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C"}
        result = subprocess.run(arguments, cwd=cwd, env=env, capture_output=True, timeout=20, check=False)
        self.assertEqual(result.returncode, 0, result.stderr.decode(errors="replace")[:4096])
        self.assertLess(len(result.stdout) + len(result.stderr), 1 << 20)
        return result.stdout.decode("utf-8")

    def test_checked_x86_bytes_match_gnu_assembly_and_execute(self):
        x, y = ["input", "x"], ["input", "y"]
        kernels = [
            ("add", [["lea", "rax", "rdi", "rsi", 1]], ["add", x, y], "x+y"),
            ("triple", [["lea", "rax", "rdi", "rdi", 2]], ["mul", x, ["const", "3"]], "x*UINT64_C(3)"),
            ("xor", [["mov", "rax", "rdi"], ["xor", "rax", "rsi"]], ["xor", x, y], "x^y"),
            ("mul", [["mov", "rax", "rdi"], ["imul", "rax", "rsi"]], ["mul", x, y], "x*y"),
            ("shift63", [["mov", "rax", "rdi"], ["shr", "rax", 63]], ["lshr", x, 63], "x>>63"),
            ("constant195", [["imm", "rax", "195"]], ["const", "195"], "UINT64_C(195)"),
            ("highreg", [["mov", "r10", "rdi"], ["add", "r10", "rsi"], ["mov", "rax", "r10"]], ["add", x, y], "x+y"),
        ]
        for name, instructions, expression, c_expression in kernels:
            with self.subTest(kernel=name):
                out = self.root / name; out.mkdir()
                problem = {"schema": "zeno-fcis/machine-problem/1", "target": TARGETS[0], "inputs": ["x", "y"],
                           "expression": expression, "grammar": instructions, "max_instructions": len(instructions), "objective": "bytes"}
                code = encode_program(TARGETS[0], instructions)
                verified = check(problem, code)
                self.assertEqual(verified["status"], "equivalent", verified)
                (out / "problem.json").write_bytes(canonical(problem))
                (out / "verification.json").write_bytes(canonical(verified))
                (out / "kernel.bin").write_bytes(code)
                (out / "kernel.o").write_bytes(elf_object(TARGETS[0], code))
                assembly = ".intel_syntax noprefix\n.text\n.globl gold\ngold:\n"
                assembly += "\n".join(asm_instruction(item) for item in instructions) + "\nret\n"
                (out / "gold.S").write_text(assembly)
                self.run_tool([self.commands["as"], "--64", "gold.S", "-o", "gold.o"], out)
                self.run_tool([self.commands["objcopy"], "-O", "binary", "--only-section=.text", "gold.o", "gold.bin"], out)
                self.assertEqual((out / "gold.bin").read_bytes(), code, "independent assembler encoding disagreement")
                (out / "runner.c").write_text(C_RUNNER.replace("RETURN_EXPR", c_expression))
                self.run_tool([self.commands["gcc"], "-std=c11", "-O2", "-fno-lto", "-Wl,-z,noexecstack", "runner.c", "kernel.o", "-o", "runner"], out)
                report = self.run_tool([str(out / "runner")], out)
                self.assertEqual(report.strip(), "native-conformance: 2112 checked pairs")
                (out / "baseline.c").write_text("#include <stdint.h>\nuint64_t baseline(uint64_t x,uint64_t y){return " + c_expression + ";}\n")
                self.run_tool([self.commands["gcc"], "-std=c11", "-O2", "-fno-lto", "-c", "baseline.c", "-o", "baseline.o"], out)
                symbols = self.run_tool([self.commands["nm"], "-S", "--defined-only", "baseline.o"], out)
                size = int(next(line.split()[1] for line in symbols.splitlines() if line.split()[-1] == "baseline"), 16)
                self.run_tool([self.commands["gcc"], "-std=c11", "-O2", "-fno-lto", "-fcf-protection=none", "-c", "baseline.c", "-o", "baseline-profile.o"], out)
                symbols = self.run_tool([self.commands["nm"], "-S", "--defined-only", "baseline-profile.o"], out)
                matched = int(next(line.split()[1] for line in symbols.splitlines() if line.split()[-1] == "baseline"), 16)
                row = {"kernel": name, "code_sha256": digest(code), "bytes": len(code), "gcc_O2_default_bytes": size,
                       "gcc_O2_matching_non_CET_profile_bytes": matched,
                       "baseline_profile": "register kernel only; CET/control-flow hardening is outside profile1",
                       "native_pairs": 2112, "semantic_status": verified["status"], "assembler_bytes_equal": True,
                       "runtime_speedup": "unmeasured", "global_optimality": False}
                self.results.append(row)
                print("MACHINE_NATIVE_RESULT " + json.dumps(row, sort_keys=True), flush=True)

    def test_rv64i_object_has_independent_elf_identification(self):
        out = self.root / "rv64i"; out.mkdir()
        code = encode_program(TARGETS[1], [["add", "a0", "a0", "a1"]])
        (out / "kernel.o").write_bytes(elf_object(TARGETS[1], code))
        text = self.run_tool([self.commands["readelf"], "-h", "-S", "-s", "-x", ".text", "kernel.o"], out)
        self.assertIn("RISC-V", text)
        self.assertIn("ELF64", text)
        self.assertIn("zeno_kernel", text)
        self.assertIn("3305b500", text)
        self.assertIn("67800000", text)
        (out / "readelf.txt").write_text(text)
        self.results.append({"target": TARGETS[1], "elf_identified": True, "hardware_execution": "not run"})


if __name__ == "__main__":
    unittest.main()
