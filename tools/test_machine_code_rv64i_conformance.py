"""RV64I instruction-model conformance under user-mode QEMU, which serves as the oracle.

This runs in the machine-code workflow's `rv64i-execution` job, which installs `qemu-user` and the
`riscv64-linux-gnu` GNU toolchain. Without them it is skipped and claims nothing; with
ZENO_MACHINE_REQUIRE_RV64I=1 a missing tool fails instead.

The admitted rv64i-register/2 forms are enumerated:
- ADD, SUB, AND, OR, XOR, SLT, SLTU and SRA over every destination and source register;
- ADDI, SLTI, SLTIU, XORI, ORI and ANDI over every register pair with edge immediates;
- SLLI, SRLI and SRAI over every register pair with edge counts, and on one pair with every count;
- NOP.

For each form, the encoding is compared with GNU `as -march=rv64i`, the independent decoder must return the same
form, and the instruction runs under QEMU from full register states. Every admitted register is compared with
the profile-2 concrete model on every state and with its SMT model on a subset. Forms inside rv64i-register/1 are
also compared with the profile-1 concrete model through an `addi a0, rd, 0` probe. Four planted model defects
must be detected. Agreement is evidence for the sampled states under the emulator that ran, not for RISC-V
hardware or a proof of the ISA.
"""

import itertools
import json
import os
from pathlib import Path
import random
import shutil
import struct
import subprocess
import tempfile
import unittest

from machine_code import verify as v1
from machine_code import verify2 as v
from machine_code.targets import encode_instruction

REGS = list(v.REGISTERS[v.RV64])
SOURCES = ["zero"] + REGS
PROFILE1 = {"ops": {"add", "sub", "xor", "and", "or", "sltu", "addi", "slli", "srli"},
            "registers": {"zero", "t0", "t1", "a0", "a1"}}
MASK = (1 << 64) - 1
EDGE = [0, 1, 2, MASK, 1 << 63, (1 << 63) - 1, MASK - 1, 63, 64, 0x5555555555555555, 0xAAAAAAAAAAAAAAAA]
STATES = int(os.environ.get("ZENO_MACHINE_RV64I_STATES", "8"))
SMT_STATES = 1
TOOLS = {"gcc": "riscv64-linux-gnu-gcc", "as": "riscv64-linux-gnu-as", "objcopy": "riscv64-linux-gnu-objcopy",
         "nm": "riscv64-linux-gnu-nm", "qemu": "qemu-riscv64"}
FOUND = {name: shutil.which(tool, path="/usr/bin:/bin") for name, tool in TOOLS.items()}
MISSING = [TOOLS[name] for name, path in FOUND.items() if path is None]
REQUIRED = os.environ.get("ZENO_MACHINE_REQUIRE_RV64I") == "1"
RET = bytes.fromhex("67800000")


def forms():
    out = []
    for op in ("add", "sub", "and", "or", "xor", "slt", "sltu", "sra"):
        out += [[op, d, a, b] for d, a, b in itertools.product(REGS, SOURCES, SOURCES)]
    for op in ("addi", "slti", "sltiu", "xori", "ori", "andi"):
        out += [[op, d, a, k] for d, a in itertools.product(REGS, SOURCES) for k in (-2048, -5, -1, 0, 1, 5, 0x555, 2047)]
    for op in ("slli", "srli", "srai"):
        out += [[op, d, a, k] for d, a in itertools.product(REGS, SOURCES) for k in (0, 1, 31, 32, 63)]
        out += [[op, "a0", "a1", k] for k in range(64) if k not in (0, 1, 31, 32, 63)]
    return out + [["nop"]]


def gnu(form):
    return "nop" if form == ["nop"] else f"{form[0]} " + ", ".join(map(str, form[1:]))


def in_profile1(form):
    return form == ["nop"] or (form[0] in PROFILE1["ops"] and all(
        x in PROFILE1["registers"] for x in form[1:] if isinstance(x, str)))


@unittest.skipUnless(not MISSING or REQUIRED,
                     "RV64I execution needs qemu-riscv64 and the riscv64-linux-gnu GNU toolchain (missing: "
                     + ", ".join(MISSING) + "); the machine-code workflow's rv64i-execution job installs them, "
                     "so this host provides no RV64I execution evidence")
class Rv64iModelConformance(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if MISSING:
            raise RuntimeError("ZENO_MACHINE_REQUIRE_RV64I=1 but tools are missing: " + ", ".join(MISSING))
        cls.temporary = tempfile.TemporaryDirectory(prefix="zeno-machine-rv64i-")
        cls.root = Path(cls.temporary.name)
        cls.forms = forms()
        cls.code = [encode_instruction(v.RV64, f) for f in cls.forms]
        rng = random.Random(20261101)
        cls.states = []
        for _ in cls.forms:
            per = [[0] * len(REGS), [MASK] * len(REGS)]
            per += [[rng.choice(EDGE) if rng.random() < 0.3 else rng.getrandbits(64) for _ in REGS]
                    for _ in range(STATES - 2)]
            cls.states.append(per)
        cls.hardware = cls.execute()

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    @classmethod
    def run_tool(cls, arguments):
        result = subprocess.run(arguments, cwd=cls.root, capture_output=True, timeout=1200, check=False,
                                env={"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C"})
        if result.returncode:
            raise RuntimeError(result.stderr.decode(errors="replace")[:4096])
        return result.stdout.decode()

    @classmethod
    def execute(cls):
        # s1 is callee-saved and outside the profile; it points at {u64 in[11]; u64 out[11]}.
        words = len(REGS)
        asm = [".option norvc", ".text"]
        for i, code in enumerate(cls.code):
            asm += [f".globl k{i}", f"k{i}:", "addi sp, sp, -16", "sd s1, 0(sp)", "mv s1, a0"]
            asm += [f"ld {r}, {8 * j}(s1)" for j, r in enumerate(REGS) if r != "a0"]
            asm += [f"ld a0, {8 * REGS.index('a0')}(s1)", f".4byte 0x{struct.unpack('<I', code)[0]:08x}"]
            asm += [f"sd {r}, {8 * (words + j)}(s1)" for j, r in enumerate(REGS)]
            asm += ["ld s1, 0(sp)", "addi sp, sp, 16", "ret"]
        (cls.root / "kernels.S").write_text("\n".join(asm) + "\n")
        n = len(cls.code)
        driver = ["#include <stdint.h>", "#include <stdio.h>", "typedef void (*k_t)(uint64_t*);"]
        driver += [f"void k{i}(uint64_t*);" for i in range(n)]
        driver += ["static k_t ks[] = {" + ",".join(f"k{i}" for i in range(n)) + "};",
                   f"int main(void){{uint64_t s[{2 * words}];unsigned long long per;FILE*f=fopen(\"states.bin\",\"rb\");",
                   "FILE*o=fopen(\"hardware.bin\",\"wb\");if(!f||!o||fread(&per,8,1,f)!=1)return 2;",
                   "for(size_t i=0;i<sizeof ks/sizeof ks[0];i++)for(unsigned long long k=0;k<per;k++){",
                   f"if(fread(s,8,{words},f)!={words})return 3;ks[i](s);fwrite(s+{words},8,{words},o);}}"
                   "fclose(o);return 0;}"]
        (cls.root / "driver.c").write_text("\n".join(driver) + "\n")
        with open(cls.root / "states.bin", "wb") as handle:
            handle.write(struct.pack("<Q", STATES))
            for per in cls.states:
                for state in per:
                    handle.write(struct.pack(f"<{words}Q", *state))
        cls.run_tool([FOUND["gcc"], "-O1", "-static", "-std=c11", "driver.c", "kernels.S", "-o", "conf"])
        cls.run_tool([FOUND["qemu"], str(cls.root / "conf")])
        data = (cls.root / "hardware.bin").read_bytes()
        return [[dict(zip(REGS, struct.unpack_from(f"<{words}Q", data, 8 * words * (i * STATES + k))))
                 for k in range(STATES)] for i in range(n)]

    def mismatches(self, model):
        found = []
        for i, form in enumerate(self.forms):
            for state, after in zip(self.states[i], self.hardware[i]):
                if model(form, self.code[i] + RET, dict(zip(REGS, state))) != after:
                    found.append((form, state))
        return found

    def test_encoder_matches_gnu_assembler_for_every_admitted_form(self):
        source = ".option norvc\n.option norelax\n.text\n" + "".join(
            f"t{i}:\n{gnu(f)}\n" for i, f in enumerate(self.forms)) + "tend:\n"
        (self.root / "gnu.S").write_text(source)
        self.run_tool([FOUND["as"], "-march=rv64i", "-mabi=lp64", "gnu.S", "-o", "gnu.o"])
        self.run_tool([FOUND["objcopy"], "-O", "binary", "--only-section=.text", "gnu.o", "gnu.bin"])
        gnu_bytes = (self.root / "gnu.bin").read_bytes()
        symbols = {line.split()[2]: int(line.split()[0], 16) for line in
                   self.run_tool([FOUND["nm"], "gnu.o"]).splitlines() if len(line.split()) == 3}
        ends = [symbols[f"t{i + 1}"] for i in range(len(self.forms) - 1)] + [symbols["tend"]]
        differing = [f for i, f in enumerate(self.forms) if gnu_bytes[symbols[f"t{i}"]:ends[i]] != self.code[i]]
        self.assertEqual(differing[:5], [])

    def test_decoder_round_trips_every_admitted_form(self):
        for form, code in zip(self.forms, self.code):
            self.assertEqual([i.form() for i in v.decode(v.RV64, code + RET)[:-1]], [form])

    def test_emulator_matches_profile2_concrete_model(self):
        model = lambda form, probe, registers: v.execute(v.RV64, probe, registers)[0]
        self.assertEqual(self.mismatches(model)[:5], [])

    def test_emulator_matches_profile2_smt_model(self):
        import z3
        context = z3.Context()
        for i, form in enumerate(self.forms):
            decoded = v.decode(v.RV64, self.code[i] + RET)
            for state, after in list(zip(self.states[i], self.hardware[i]))[:SMT_STATES]:
                symbolic = {r: z3.BitVecVal(value, 64, context) for r, value in zip(REGS, state)}
                final, _ = v.machine_smt(z3, v.RV64, decoded, symbolic, {}, context)
                self.assertEqual({r: z3.simplify(value).as_long() for r, value in final.items()}, after, form)

    def test_emulator_matches_profile1_concrete_model_on_its_forms(self):
        compared = 0
        for i, form in enumerate(self.forms):
            if not in_profile1(form):
                continue
            dst = "a0" if form == ["nop"] else form[1]
            probe = self.code[i] + (encode_instruction(v1.RV64, ["addi", "a0", dst, 0]) if dst != "a0" else b"") + RET
            for state, after in zip(self.states[i], self.hardware[i]):
                before = dict(zip(REGS, state))
                value = v1.run_concrete(v1.RV64, probe, {"x": before["a0"], "y": before["a1"]},
                                        {"t0": before["t0"], "t1": before["t1"]})
                self.assertEqual(value, after[dst], form)
                compared += 1
        self.assertGreater(compared, 1000)

    def test_planted_model_defects_are_detected(self):
        exact = lambda form, probe, registers: v.execute(v.RV64, probe, registers)[0]
        signed = lambda value: value - (1 << 64) if value >> 63 else value

        def defect(op, compute):
            def model(form, probe, registers):
                after = exact(form, probe, registers)
                if form[0] == op:
                    left = 0 if form[2] == "zero" else registers[form[2]]
                    right = form[3] if isinstance(form[3], int) else 0 if form[3] == "zero" else registers[form[3]]
                    after[form[1]] = compute(left, right) & MASK
                return after
            return model

        controls = {"sra-logical": defect("sra", lambda a, b: a >> (b & 63)),
                    "sra-five-bit-amount": defect("sra", lambda a, b: signed(a) >> (b & 31)),
                    "slt-unsigned": defect("slt", lambda a, b: int(a < b)),
                    "sltiu-zero-extended": defect("sltiu", lambda a, b: int(a < (b & 0xFFF)))}
        for name, model in controls.items():
            self.assertTrue(self.mismatches(model), f"planted defect not detected: {name}")
        out = os.environ.get("ZENO_MACHINE_CONFORMANCE_OUT")
        if out:
            qemu = self.run_tool([FOUND["qemu"], "--version"]).splitlines()[0]
            Path(out).write_text(json.dumps({"schema": "zeno-fcis/rv64i-model-conformance/1",
                                             "forms": len(self.forms), "states_per_form": STATES,
                                             "emulator": qemu}) + "\n")


if __name__ == "__main__":
    unittest.main()
