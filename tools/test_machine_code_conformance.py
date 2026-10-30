"""Hardware conformance of the x86 word-kernel instruction model; the physical CPU is the oracle.

Every admitted x86_64-sysv-register/1 instruction form is enumerated:
- the seven register ALU operations over all register pairs;
- SHL/SHR with every count;
- MOVABS with edge immediates;
- LEA over every base, index and scale;
- NOP.

For each form, three independent checks run:
1. The encoding is compared with GNU `as`.
2. The independent decoder must return the same operation.
3. The real instruction runs from many full register states. The destination value is compared with the concrete
   model on every state, and with the SMT model used by equivalence checks on a subset. Every other admitted
   register must be unchanged.

Two planted model defects must be detected, so a vacuous comparison cannot pass. Agreement is evidence for the
sampled states on the executing CPU only. It is not a proof of the ISA or of other processors. RV64I needs a
separate execution environment.
"""

import itertools
import json
import os
from pathlib import Path
import platform
import random
import shutil
import struct
import subprocess
import tempfile
import unittest

from machine_code import verify as v
from machine_code.targets import encode_instruction

REGS = ["rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11"]
SCRATCH = ("rax", "rcx", "rdx", "r8", "r9", "r10", "r11")
MASK = (1 << 64) - 1
EDGE = [0, 1, 2, MASK, 1 << 63, (1 << 63) - 1, MASK - 1, 195, 0x5555555555555555, 0xAAAAAAAAAAAAAAAA]
STATES = int(os.environ.get("ZENO_MACHINE_CONFORMANCE_STATES", "16"))
SMT_STATES = 2


def forms():
    out = []
    for op in ("mov", "add", "sub", "and", "or", "xor", "imul"):
        out += [[op, d, s] for d, s in itertools.product(REGS, REGS)]
    for op in ("shl", "shr"):
        out += [[op, d, c] for d in REGS for c in range(64)]
    for d in REGS:
        out += [["imm", d, str(x)] for x in (0, 1, MASK, 1 << 63, (1 << 63) - 1, 195, 0x0123456789ABCDEF)]
    out += [["lea", d, b, i, s] for d, b, i in itertools.product(REGS, REGS, REGS) for s in (1, 2, 4, 8)]
    return out + [["nop"]]


def intel(form):
    op, *a = form
    if op == "lea":
        return f"lea {a[0]}, [{a[1]} + {a[2]}*{a[3]}]"
    if op == "imm":
        return f"movabs {a[0]}, {a[1]}"
    return op + (" " + ", ".join(map(str, a)) if a else "")


@unittest.skipUnless(platform.system() == "Linux" and platform.machine() == "x86_64",
                     "actual Linux x86-64 execution required")
class X86ModelConformance(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tools = {}
        for name in ("gcc", "as", "objcopy", "nm"):
            path = shutil.which(name, path="/usr/bin:/bin")
            if path is None:
                raise RuntimeError("required GNU conformance tool is missing: " + name)
            cls.tools[name] = path
        cls.temporary = tempfile.TemporaryDirectory(prefix="zeno-machine-conformance-")
        cls.root = Path(cls.temporary.name)
        cls.forms = forms()
        cls.code = [encode_instruction(v.X86, f) for f in cls.forms]
        rng = random.Random(20261030)
        cls.states = []
        for _ in cls.forms:
            per = [[0] * 9, [MASK] * 9]
            per += [[rng.choice(EDGE) if rng.random() < 0.3 else rng.getrandbits(64) for _ in REGS]
                    for _ in range(STATES - 2)]
            cls.states.append(per)
        cls.hardware = cls.execute()

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    @classmethod
    def run_tool(cls, arguments):
        result = subprocess.run(arguments, cwd=cls.root, capture_output=True, timeout=600, check=False,
                                env={"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C"})
        if result.returncode:
            raise RuntimeError(result.stderr.decode(errors="replace")[:4096])
        return result.stdout.decode()

    @classmethod
    def execute(cls):
        # rbx is callee-saved and outside the profile; it points at {u64 in[9]; u64 out[9]}.
        asm = [".intel_syntax noprefix", ".text"]
        for i, code in enumerate(cls.code):
            asm += [f".globl k{i}", f"k{i}:", "push rbx", "mov rbx, rdi"]
            asm += [f"mov {r}, [rbx + {8 * j}]" for j, r in enumerate(REGS) if r != "rdi"]
            asm += [f"mov rdi, [rbx + {8 * REGS.index('rdi')}]", ".byte " + ",".join(map(str, code))]
            asm += [f"mov [rbx + {72 + 8 * j}], {r}" for j, r in enumerate(REGS)] + ["pop rbx", "ret"]
        (cls.root / "kernels.S").write_text("\n".join(asm) + "\n")
        n = len(cls.code)
        driver = ["#include <stdint.h>", "#include <stdio.h>", "typedef void (*k_t)(uint64_t*);"]
        driver += [f"void k{i}(uint64_t*);" for i in range(n)]
        driver += ["static k_t ks[] = {" + ",".join(f"k{i}" for i in range(n)) + "};",
                   "int main(void){uint64_t s[18];unsigned long long per;FILE*f=fopen(\"states.bin\",\"rb\");",
                   "FILE*o=fopen(\"hardware.bin\",\"wb\");if(!f||!o||fread(&per,8,1,f)!=1)return 2;",
                   "for(size_t i=0;i<sizeof ks/sizeof ks[0];i++)for(unsigned long long k=0;k<per;k++){",
                   "if(fread(s,8,9,f)!=9)return 3;ks[i](s);fwrite(s+9,8,9,o);}fclose(o);return 0;}"]
        (cls.root / "driver.c").write_text("\n".join(driver) + "\n")
        with open(cls.root / "states.bin", "wb") as handle:
            handle.write(struct.pack("<Q", STATES))
            for per in cls.states:
                for state in per:
                    handle.write(struct.pack("<9Q", *state))
        cls.run_tool([cls.tools["gcc"], "-O1", "-std=c11", "-Wl,-z,noexecstack", "driver.c", "kernels.S", "-o", "conf"])
        cls.run_tool([str(cls.root / "conf")])
        data = (cls.root / "hardware.bin").read_bytes()
        return [[struct.unpack_from("<9Q", data, 72 * (i * STATES + k)) for k in range(STATES)]
                for i in range(n)]

    def probe(self, i):
        form = self.forms[i]
        dst = form[1] if form[0] != "nop" else "rax"
        tail = encode_instruction(v.X86, ["mov", "rax", dst]) if dst != "rax" else b""
        return dst, self.code[i] + tail + b"\xc3"

    def mismatches(self, model):
        found = []
        for i in range(len(self.forms)):
            dst, probe = self.probe(i)
            for state, after in zip(self.states[i], self.hardware[i]):
                before = dict(zip(REGS, state))
                inputs = {"x": before["rdi"], "y": before["rsi"]}
                initial = {r: before[r] for r in SCRATCH}
                if model(self.forms[i], probe, inputs, initial) != dict(zip(REGS, after))[dst]:
                    found.append((self.forms[i], state))
        return found

    def test_encoder_matches_gnu_assembler_for_every_admitted_form(self):
        source = ".intel_syntax noprefix\n.text\n" + "".join(
            f"t{i}:\n{intel(f)}\n" for i, f in enumerate(self.forms)) + "tend:\n"
        (self.root / "gnu.S").write_text(source)
        self.run_tool([self.tools["as"], "--64", "gnu.S", "-o", "gnu.o"])
        self.run_tool([self.tools["objcopy"], "-O", "binary", "--only-section=.text", "gnu.o", "gnu.bin"])
        gnu = (self.root / "gnu.bin").read_bytes()
        symbols = {name: int(address, 16) for address, _, name in
                   (line.split() for line in self.run_tool([self.tools["nm"], "gnu.o"]).splitlines())}
        ends = [symbols[f"t{i + 1}"] for i in range(len(self.forms) - 1)] + [symbols["tend"]]
        differing = [f for i, f in enumerate(self.forms) if gnu[symbols[f"t{i}"]:ends[i]] != self.code[i]]
        self.assertEqual(differing, [])

    def test_decoder_round_trips_every_admitted_form(self):
        for form, code in zip(self.forms, self.code):
            decoded = v.decode(v.X86, code + b"\xc3")
            self.assertEqual([i.op for i in decoded], [form[0], "ret"], form)

    def test_hardware_matches_concrete_model_and_leaves_other_registers(self):
        model = lambda form, probe, inputs, initial: v.run_concrete(v.X86, probe, inputs, initial)
        self.assertEqual(self.mismatches(model)[:5], [])
        for i, form in enumerate(self.forms):
            if form[0] == "nop":
                continue
            for state, after in zip(self.states[i], self.hardware[i]):
                changed = [r for r, a, b in zip(REGS, state, after) if a != b and r != form[1]]
                self.assertEqual(changed, [], form)

    def test_hardware_matches_smt_model(self):
        import z3
        context = z3.Context()
        for i in range(len(self.forms)):
            dst, probe = self.probe(i)
            decoded = v.decode(v.X86, probe)
            for state, after in list(zip(self.states[i], self.hardware[i]))[:SMT_STATES]:
                before = dict(zip(REGS, state))
                inputs = {k: z3.BitVecVal(before[r], 64, context) for k, r in (("x", "rdi"), ("y", "rsi"))}
                scratch = {r: z3.BitVecVal(before[r], 64, context) for r in SCRATCH}
                value = z3.simplify(v._machine_smt(z3, v.X86, decoded, inputs, scratch)).as_long()
                self.assertEqual(value, dict(zip(REGS, after))[dst], (self.forms[i], state))

    def test_planted_model_defects_are_detected(self):
        def wrong_lea_scale(form, probe, inputs, initial):
            if form[0] == "lea" and form[4] == 8:
                return v.run_concrete(v.X86, encode_instruction(v.X86, form[:4] + [4]) + probe[len(encode_instruction(v.X86, form)):], inputs, initial)
            return v.run_concrete(v.X86, probe, inputs, initial)

        def arithmetic_shift(form, probe, inputs, initial):
            value = v.run_concrete(v.X86, probe, inputs, initial)
            if form[0] == "shr" and form[2] > 0:
                source = {**initial, "rdi": inputs["x"], "rsi": inputs["y"]}[form[1]]
                if source >> 63:
                    value |= (MASK << (64 - form[2])) & MASK
            return value

        for name, model in (("lea-scale", wrong_lea_scale), ("arithmetic-shr", arithmetic_shift)):
            self.assertTrue(self.mismatches(model), f"planted defect not detected: {name}")
        out = os.environ.get("ZENO_MACHINE_CONFORMANCE_OUT")
        if out:
            Path(out).write_text(json.dumps({"schema": "zeno-fcis/x86-model-conformance/1",
                                             "forms": len(self.forms), "states_per_form": STATES,
                                             "cpu": platform.processor() or platform.machine()}) + "\n")


if __name__ == "__main__":
    unittest.main()
