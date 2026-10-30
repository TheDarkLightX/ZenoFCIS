"""Actual Linux x86-64 execution of emitted profile-2 kernels; GNU tools are independent encoding oracles.

Each kernel goes through the session loop (open, proposal or search, emit, replay). The emitted relocatable
object is linked with a C runner whose expected results use GCC builtins and 128-bit arithmetic, not the
problem's expression. Two-result kernels use the SysV return of a two-word structure in rax:rdx.
"""

import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import unittest

from machine_code import model, profile2, session, targets
from machine_code import verify2

X86 = profile2.TARGETS[0]
MASK = str((1 << 64) - 1)

RUNNER = r'''
#include <stdint.h>
#include <inttypes.h>
#include <stdio.h>
typedef struct { uint64_t r0, r1; } pair;
extern RESULT zeno_kernel(PARAMETERS);
static void expected(const uint64_t *a, uint64_t *out) { EXPECTED }
static int one(const uint64_t *a) {
    uint64_t want[2] = {0, 0}, got[2] = {0, 0};
    expected(a, want);
    CALL
    for (unsigned k = 0; k < OUTPUTS; k++) if (got[k] != want[k]) {
        fprintf(stderr, "mismatch result %u got=%" PRIu64 " expected=%" PRIu64 "\n", k, got[k], want[k]);
        return 1;
    }
    return 0;
}
int main(void) {
    uint64_t edge[] = {0, 1, 2, UINT64_MAX, UINT64_C(1) << 63, (UINT64_C(1) << 63) - 1, UINT64_MAX - 1, 195};
    uint64_t a[INPUTS];
    unsigned long checked = 0, total = 1;
    for (unsigned i = 0; i < INPUTS; i++) total *= 8;
    for (unsigned long n = 0; n < total; n++) {
        unsigned long m = n;
        for (unsigned i = 0; i < INPUTS; i++) { a[i] = edge[m % 8]; m /= 8; }
        if (one(a)) return 1;
        checked++;
    }
    uint64_t state = UINT64_C(0x9e3779b97f4a7c15);
    for (unsigned n = 0; n < 4096; n++) {
        for (unsigned i = 0; i < INPUTS; i++) {
            state = state * UINT64_C(6364136223846793005) + UINT64_C(1442695040888963407);
            a[i] = state;
        }
        if (one(a)) return 1;
        checked++;
    }
    printf("native-conformance: %lu checked argument vectors\n", checked);
    return 0;
}
'''


def i(name):
    return ["input", name]


ADD = ["add", i("x"), i("y")]
SIGNED_TOTAL = ["slt", ["and", ["xor", i("x"), ADD], ["xor", i("y"), ADD]], ["const", "0"]]
KERNELS = [
    ("add128", ["a_lo", "a_hi", "b_lo", "b_hi"],
     [["add", i("a_lo"), i("b_lo")],
      ["add", ["add", i("a_hi"), i("b_hi")], ["ult", ["add", i("a_lo"), i("b_lo")], i("a_lo")]]],
     [["mov", "rax", "rdi"], ["add", "rax", "rdx"], ["mov", "rdx", "rsi"], ["adc", "rdx", "rcx"]],
     "unsigned __int128 x = ((unsigned __int128)a[1] << 64) | a[0], y = ((unsigned __int128)a[3] << 64) | a[2];"
     "unsigned __int128 s = x + y; out[0] = (uint64_t)s; out[1] = (uint64_t)(s >> 64);"),
    ("sub128", ["a_lo", "a_hi", "b_lo", "b_hi"],
     [["sub", i("a_lo"), i("b_lo")], ["sub", ["sub", i("a_hi"), i("b_hi")], ["ult", i("a_lo"), i("b_lo")]]],
     [["mov", "rax", "rdi"], ["sub", "rax", "rdx"], ["mov", "rdx", "rsi"], ["sbb", "rdx", "rcx"]],
     "unsigned __int128 x = ((unsigned __int128)a[1] << 64) | a[0], y = ((unsigned __int128)a[3] << 64) | a[2];"
     "unsigned __int128 s = x - y; out[0] = (uint64_t)s; out[1] = (uint64_t)(s >> 64);"),
    ("add_carry_out", ["x", "y"], [ADD, ["ult", ADD, i("x")]],
     [["xor", "rdx", "rdx"], ["mov", "rax", "rdi"], ["add", "rax", "rsi"], ["setb", "rdx"]],
     "out[1] = __builtin_add_overflow(a[0], a[1], &out[0]);"),
    ("umin", ["x", "y"], [["ite", ["ule", i("x"), i("y")], i("x"), i("y")]], "search",
     "out[0] = a[0] < a[1] ? a[0] : a[1];"),
    ("smax", ["x", "y"], [["ite", ["slt", i("x"), i("y")], i("y"), i("x")]],
     [["mov", "rax", "rdi"], ["cmp", "rdi", "rsi"], ["cmovl", "rax", "rsi"]],
     "int64_t x = (int64_t)a[0], y = (int64_t)a[1]; out[0] = (uint64_t)(x > y ? x : y);"),
    ("saturating_add", ["x", "y"], [["ite", ["ult", ADD, i("x")], ["const", MASK], ADD]],
     [["mov", "rax", "rdi"], ["add", "rax", "rsi"], ["sbb", "rdx", "rdx"], ["or", "rax", "rdx"]],
     "uint64_t s; out[0] = __builtin_add_overflow(a[0], a[1], &s) ? UINT64_MAX : s;"),
    ("signed_saturating_add", ["x", "y"],
     [["ite", SIGNED_TOTAL, ["ite", ["slt", i("x"), ["const", "0"]], ["const", str(1 << 63)],
                             ["const", str((1 << 63) - 1)]], ADD]],
     [["mov", "rdx", "rdi"], ["sar", "rdx", 63], ["imm", "rcx", str((1 << 63) - 1)], ["xor", "rdx", "rcx"],
      ["mov", "rax", "rdi"], ["add", "rax", "rsi"], ["cmovo", "rax", "rdx"]],
     "int64_t s; out[0] = (uint64_t)(__builtin_add_overflow((int64_t)a[0], (int64_t)a[1], &s)"
     " ? ((int64_t)a[0] < 0 ? INT64_MIN : INT64_MAX) : s);"),
    ("abs", ["x"], [["ite", ["slt", i("x"), ["const", "0"]], ["sub", ["const", "0"], i("x")], i("x")]],
     [["mov", "rax", "rdi"], ["neg", "rax"], ["cmovs", "rax", "rdi"]],
     "out[0] = (int64_t)a[0] < 0 ? UINT64_C(0) - a[0] : a[0];"),
]
SEARCH_GRAMMAR = [["mov", "rax", "rdi"], ["mov", "rax", "rsi"], ["cmp", "rdi", "rsi"], ["cmova", "rax", "rsi"],
                  ["cmovb", "rax", "rdi"], ["nop"]]
BYTE = {"rax": "al", "rcx": "cl", "rdx": "dl", "rsi": "sil", "rdi": "dil",
        "r8": "r8b", "r9": "r9b", "r10": "r10b", "r11": "r11b"}


def asm_instruction(form):
    op, *args = form
    if op == "lea":
        return f"lea {args[0]}, [{args[1]} + {args[2]}*{args[3]}]"
    if op == "imm":
        return f"movabs {args[0]}, {args[1]}"
    if op in verify2.SETCC:
        return f"{op} {BYTE[args[0]]}"
    if op == "movzx":
        return f"movzx {args[0]}, {BYTE[args[1]]}"
    return op + (" " + ", ".join(map(str, args)) if args else "")


@unittest.skipUnless(platform.system() == "Linux" and platform.machine() == "x86_64",
                     "actual Linux x86-64 native profile required")
class NativeProfile2Tests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.commands = {}
        for name in ("gcc", "as", "objcopy", "readelf", "nm"):
            path = shutil.which(name, path="/usr/bin:/bin")
            if path is None:
                raise RuntimeError("required GNU conformance tool is missing: " + name)
            cls.commands[name] = path
        retained = os.environ.get("ZENO_MACHINE_NATIVE_OUT")
        cls.temporary = None if retained else tempfile.TemporaryDirectory(prefix="zeno-machine-native2-")
        cls.root = Path(retained) if retained else Path(cls.temporary.name)
        if retained:
            cls.root.mkdir(mode=0o700)
        cls.results = []

    @classmethod
    def tearDownClass(cls):
        (cls.root / "results.json").write_bytes(model.canonical(cls.results) + b"\n")
        if cls.temporary:
            cls.temporary.cleanup()

    def run_tool(self, arguments, cwd):
        env = {"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C"}
        result = subprocess.run(arguments, cwd=cwd, env=env, capture_output=True, timeout=60, check=False)
        self.assertEqual(result.returncode, 0, result.stderr.decode(errors="replace")[:4096])
        self.assertLess(len(result.stdout) + len(result.stderr), 1 << 20)
        return result.stdout.decode("utf-8")

    def emitted(self, out, inputs, outputs, body):
        grammar = SEARCH_GRAMMAR if body == "search" else body
        problem = {"schema": profile2.SCHEMA, "target": X86, "inputs": inputs, "outputs": outputs,
                   "grammar": grammar, "max_instructions": 3 if body == "search" else len(body), "objective": "bytes"}
        (out / "problem.json").write_bytes(model.canonical(problem))
        directory = out / "session"
        session.open_session(out / "problem.json", directory)
        if body == "search":
            found = session.search(directory, max_candidates=5000, max_checks=16, deadline_ms=30000)
            self.assertEqual(found["status"], "checked-candidate", found)
        else:
            code = targets.encode_program(X86, body)
            (out / "proposal.json").write_bytes(model.canonical({
                "schema": "zeno-fcis/machine-proposal/1", "code_hex": code.hex(),
                "provenance": {"kind": "human", "name": "native-test", "note": "known flag-based kernel"}}))
            self.assertEqual(session.submit(directory, out / "proposal.json")["status"], "equivalent")
        artifact = out / "artifact"
        self.assertEqual(session.emit(directory, artifact)["status"], "emitted")
        self.assertEqual(session.replay_artifact(artifact)["status"], "replayed")
        return artifact

    def test_emitted_flag_kernels_match_gnu_assembly_and_execute(self):
        for name, inputs, outputs, body, expected in KERNELS:
            with self.subTest(kernel=name):
                out = self.root / name; out.mkdir()
                artifact = self.emitted(out, inputs, outputs, body)
                code = (artifact / "kernel.bin").read_bytes()
                forms = [ins.form() for ins in verify2.decode(X86, code)[:-1]]
                (out / "gold.S").write_text(".intel_syntax noprefix\n.text\n"
                                            + "\n".join(asm_instruction(form) for form in forms) + "\nret\n")
                self.run_tool([self.commands["as"], "--64", "gold.S", "-o", "gold.o"], out)
                self.run_tool([self.commands["objcopy"], "-O", "binary", "--only-section=.text", "gold.o", "gold.bin"], out)
                self.assertEqual((out / "gold.bin").read_bytes(), code, "independent assembler encoding disagreement")
                pair = len(outputs) == 2
                parameters = ", ".join(f"uint64_t a{k}" for k in range(len(inputs)))
                arguments = ", ".join(f"a[{k}]" for k in range(len(inputs)))
                call = (f"pair p = zeno_kernel({arguments}); got[0] = p.r0; got[1] = p.r1;" if pair
                        else f"got[0] = zeno_kernel({arguments});")
                runner = (RUNNER.replace("RESULT", "pair" if pair else "uint64_t").replace("PARAMETERS", parameters)
                          .replace("EXPECTED", expected).replace("CALL", call)
                          .replace("OUTPUTS", str(len(outputs))).replace("INPUTS", str(len(inputs))))
                (out / "runner.c").write_text(runner)
                self.run_tool([self.commands["gcc"], "-std=gnu11", "-O2", "-fno-lto", "-Wl,-z,noexecstack",
                               "runner.c", str(artifact / "kernel.o"), "-o", "runner"], out)
                report = self.run_tool([str(out / "runner")], out).strip()
                vectors = 8 ** len(inputs) + 4096
                self.assertEqual(report, f"native-conformance: {vectors} checked argument vectors")
                baseline = ("#include <stdint.h>\ntypedef struct { uint64_t r0, r1; } pair;\n"
                            + ("pair" if pair else "uint64_t") + f" baseline({parameters}){{uint64_t a[] = {{"
                            + ", ".join(f"a{k}" for k in range(len(inputs))) + "}; uint64_t out[2] = {0, 0}; "
                            + expected + (" return (pair){out[0], out[1]};}\n" if pair else " return out[0];}\n"))
                (out / "baseline.c").write_text(baseline)
                self.run_tool([self.commands["gcc"], "-std=gnu11", "-O2", "-fno-lto", "-fcf-protection=none",
                               "-c", "baseline.c", "-o", "baseline.o"], out)
                symbols = self.run_tool([self.commands["nm"], "-S", "--defined-only", "baseline.o"], out)
                size = int(next(line.split()[1] for line in symbols.splitlines() if line.split()[-1] == "baseline"), 16)
                row = {"kernel": name, "code_sha256": model.digest(code), "bytes": len(code),
                       "instructions": len(forms), "decoded": [asm_instruction(form) for form in forms],
                       "route": "search" if body == "search" else "proposal",
                       "gcc_O2_non_CET_bytes": size, "native_vectors": vectors, "results": len(outputs),
                       "assembler_bytes_equal": True, "replayed": True,
                       "runtime_speedup": "unmeasured", "global_optimality": False}
                self.results.append(row)
                print("MACHINE_NATIVE2_RESULT " + json.dumps(row, sort_keys=True), flush=True)

    def test_paired_timing_of_a_two_result_kernel_against_gcc(self):
        from machine_code.bench import bench
        out = self.root / "bench"; out.mkdir()
        name, inputs, outputs, body, _ = KERNELS[0]
        artifact = self.emitted(out, inputs, outputs, body)
        report = bench(artifact, out / "timing", iterations=20_000, repeats=5)
        self.assertEqual(report["status"], "measured", report)
        self.assertTrue(report["baseline_c"].startswith("(pair){(in_a_lo + in_b_lo), "), report["baseline_c"])
        self.assertTrue(all(value > 0 for value in report["ticks_per_call"].values()))
        self.assertIn("Not a portable speedup claim", report["scope"])
        rv = self.root / "bench-rv"; rv.mkdir()
        problem = {"schema": profile2.SCHEMA, "target": profile2.TARGETS[1], "inputs": ["x", "y"],
                   "outputs": [["add", i("x"), i("y")]], "grammar": [["add", "a0", "a0", "a1"]],
                   "max_instructions": 1, "objective": "bytes"}
        (rv / "problem.json").write_bytes(model.canonical(problem))
        session.open_session(rv / "problem.json", rv / "session")
        (rv / "proposal.json").write_bytes(model.canonical({
            "schema": "zeno-fcis/machine-proposal/1", "code_hex": "3305b50067800000",
            "provenance": {"kind": "human", "name": "native-test", "note": "RV64I sum"}}))
        self.assertEqual(session.submit(rv / "session", rv / "proposal.json")["status"], "equivalent")
        session.emit(rv / "session", rv / "artifact")
        refused = bench(rv / "artifact", rv / "timing", iterations=20_000, repeats=5)
        self.assertEqual((refused["status"], refused["reason"]),
                         ("inconclusive", "only x86-64 kernels can be executed on this host"))
        self.assertFalse((rv / "timing").exists())

    def test_rv64i_two_result_object_has_independent_elf_identification(self):
        out = self.root / "rv64i"; out.mkdir()
        code = targets.encode_program(profile2.TARGETS[1], [["add", "a0", "a0", "a2"], ["sltu", "t0", "a0", "a2"],
                                                            ["add", "a1", "a1", "a3"], ["add", "a1", "a1", "t0"]])
        (out / "kernel.o").write_bytes(targets.elf_object(profile2.TARGETS[1], code))
        text = self.run_tool([self.commands["readelf"], "-h", "-S", "-s", "-x", ".text", "kernel.o"], out)
        for needle in ("RISC-V", "ELF64", "zeno_kernel", "3305c500", "b332c500", "67800000"):
            self.assertIn(needle, text)
        self.results.append({"target": profile2.TARGETS[1], "elf_identified": True, "hardware_execution": "not run"})


if __name__ == "__main__":
    unittest.main()
