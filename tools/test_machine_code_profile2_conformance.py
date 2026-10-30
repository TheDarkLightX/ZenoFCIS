"""Hardware conformance of the profile-2 x86 instruction and flag model; the physical CPU is the oracle.

Every admitted x86_64-sysv-register/2 instruction form is enumerated:
- MOV, ADD, SUB, AND, OR, XOR, IMUL, ADC, SBB, CMP, TEST, MOVZX and the fourteen CMOVcc over all register pairs;
- SHL/SHR/SAR with every count;
- the fourteen SETcc on every register byte, and NEG on every register;
- MOVABS with edge immediates, LEA over every base, index and scale, and NOP;
- ADD, SUB, AND, OR, XOR and CMP with every 8-bit immediate and edge 32-bit immediates on every register, in the
  canonical encodings (83 /n ib, the RAX short form, 81 /n id).

For each form, three independent checks run:
1. The encoding is compared with GNU `as`.
2. The independent decoder must return the same form.
3. The real instruction runs from many full register and flag states, loaded with POPFQ. Every admitted
   register and every CF/ZF/SF/OF bit read back with PUSHFQ is compared with the concrete model on every state,
   and with the SMT model on a subset. A flag the model leaves undefined is not compared.

Five planted model defects must be detected, so a vacuous comparison cannot pass. Host-independent checks scan
that every short body the decoder accepts is one of the enumerated forms in its unique canonical encoding, and
that the RV64I concrete and SMT models agree with each other on sampled states; that agreement is not execution.
Agreement is evidence for the sampled states on the executing CPU only. It is not a proof of the ISA or of other
processors. The RV64I model is compared under emulation in `tools/test_machine_code_rv64i_conformance.py`.
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

from machine_code import verify2 as v
from machine_code.targets import encode_instruction

REGS = ["rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11"]
BYTE = {"rax": "al", "rcx": "cl", "rdx": "dl", "rsi": "sil", "rdi": "dil",
        "r8": "r8b", "r9": "r9b", "r10": "r10b", "r11": "r11b"}
FLAG_BITS = {"cf": 0, "zf": 6, "sf": 7, "of": 11}
CONDITIONS = list(v.CONDITIONS.values())
MASK = (1 << 64) - 1
EDGE = [0, 1, 2, MASK, 1 << 63, (1 << 63) - 1, MASK - 1, 195, 0x5555555555555555, 0xAAAAAAAAAAAAAAAA]
STATES = int(os.environ.get("ZENO_MACHINE_CONFORMANCE_STATES", "16"))
SMT_STATES = 2
IMM32 = [-(1 << 31), -(1 << 31) + 1, -0x12345678, -256, -129, 128, 255, 65536, 0x12345678, (1 << 31) - 1]
# The SMT comparison samples 8-bit immediates; the concrete comparison covers all of them.
SMT_IMM8 = {-128, -127, -2, -1, 0, 1, 2, 64, 126, 127}
WORDS = len(REGS) + 1  # registers then RFLAGS, for the state before and after


def forms():
    out = []
    pairs = ("mov", "add", "sub", "and", "or", "xor", "imul", "adc", "sbb", "cmp", "test", "movzx")
    for op in pairs + tuple("cmov" + c for c in CONDITIONS):
        out += [[op, d, s] for d, s in itertools.product(REGS, REGS)]
    for op in ("shl", "shr", "sar"):
        out += [[op, d, c] for d in REGS for c in range(64)]
    out += [["set" + c, d] for c in CONDITIONS for d in REGS]
    out += [["neg", d] for d in REGS]
    for d in REGS:
        out += [["imm", d, str(x)] for x in (0, 1, MASK, 1 << 63, (1 << 63) - 1, 195, 0x0123456789ABCDEF)]
    out += [["lea", d, b, i, s] for d, b, i in itertools.product(REGS, REGS, REGS) for s in (1, 2, 4, 8)]
    out += [[op, d, k] for op in v.IMMEDIATE_OPS for d in REGS for k in list(range(-128, 128)) + IMM32]
    return out + [["nop"]]


def intel(form):
    op, *a = form
    if op == "lea":
        return f"lea {a[0]}, [{a[1]} + {a[2]}*{a[3]}]"
    if op == "imm":
        return f"movabs {a[0]}, {a[1]}"
    if op in v.SETCC:
        return f"{op} {BYTE[a[0]]}"
    if op == "movzx":
        return f"movzx {a[0]}, {BYTE[a[1]]}"
    return op + (" " + ", ".join(map(str, a)) if a else "")


def flags_word(flags, rng):
    """RFLAGS image: reserved bit 1 and IF; PF and AF random to show the model never reads them."""
    word = 0x202 | (rng.getrandbits(1) << 2) | (rng.getrandbits(1) << 4)
    return word | sum(flags[name] << bit for name, bit in FLAG_BITS.items())


def cpu_name():
    try:
        rows = Path("/proc/cpuinfo").read_text().splitlines()
        return next(row.split(":", 1)[1].strip() for row in rows if row.startswith("model name"))
    except (OSError, StopIteration):
        return platform.processor() or platform.machine()


class HostIndependentModelChecks(unittest.TestCase):
    """Decoder-language scans and RV64I concrete/SMT agreement; neither executes target code."""

    def test_every_accepted_short_x86_body_is_an_enumerated_canonical_form(self):
        known = {encode_instruction(v.X86, f): f for f in forms()}
        self.assertEqual(len(known), len(forms()), "two enumerated forms share an encoding")
        families = [bytes([b]) for b in range(256)]
        families += [bytes([b0, b1]) for b0 in range(256) for b1 in range(256)]
        bodies = itertools.chain(
            families,
            (bytes([b0, b1, b2]) for b0 in [0x0F, *range(0x40, 0x50)] for b1 in range(256) for b2 in range(256)),
            (bytes([b0, b1, b2, b3]) for b0, b1 in [(0x40, 0x0F), (0x41, 0x0F)]
             + [(r, o) for r in range(0x48, 0x50) for o in (0x0F, 0x8D, 0xC1, 0x83)]
             for b2 in range(256) for b3 in range(256)),
            # 32-bit immediates: every REX.W prefix, ModRM byte and accumulator opcode, with sampled values
            # including some that fit eight bits and so must refuse.
            (bytes([r, 0x81, modrm]) + struct.pack("<i", k) for r in range(0x48, 0x50) for modrm in range(256)
             for k in IMM32 + [-128, -1, 0, 127]),
            (bytes([r, o]) + struct.pack("<i", k) for r in range(0x40, 0x50) for o in range(0x05, 0x40, 8)
             for k in IMM32 + [-128, -1, 0, 127]))
        accepted = 0
        for body in bodies:
            try:
                decoded = v.decode(v.X86, body + b"\xc3")
            except ValueError:
                continue
            accepted += 1
            for ins in decoded[:-1]:
                piece = body[ins.offset:ins.offset + ins.size]
                self.assertEqual(known.get(piece), ins.form(), body.hex())
        self.assertGreater(accepted, 4000)

    def test_every_accepted_rv64i_word_shape_is_a_canonical_form(self):
        registers = list(v._RREG.values())
        ops = {"add", "sub", "xor", "and", "or", "slt", "sltu", "sra", "addi", "slti", "sltiu", "xori", "ori",
               "andi", "slli", "srli", "srai"}
        accepted = 0
        for opcode, funct3, high in itertools.product((0x13, 0x33), range(8), range(128)):
            for field, value in itertools.product((7, 15, 20), range(32)):
                fields = {7: 10, 15: 11, 20: 12}
                fields[field] = value
                word = (high << 25) | (fields[20] << 20) | (fields[15] << 15) | (funct3 << 12) | (fields[7] << 7) | opcode
                code = struct.pack("<I", word) + bytes.fromhex("67800000")
                try:
                    decoded = v.decode(v.RV64, code)
                except ValueError:
                    continue
                accepted += 1
                form = decoded[0].form()
                self.assertIn(form[0], ops | {"nop"})
                self.assertTrue(all(isinstance(x, int) or x in registers for x in form[1:]), form)
                self.assertEqual(encode_instruction(v.RV64, form), code[:4], form)
        self.assertGreater(accepted, 1000)

    def test_rv64i_concrete_and_smt_models_agree_on_sampled_states(self):
        import z3
        context = z3.Context()
        rng = random.Random(20261102)
        registers = v.REGISTERS[v.RV64]
        sources = ["zero", "t0", "a0", "a7"]
        rv = [[op, d, a, b] for op in ("add", "sub", "and", "or", "xor", "slt", "sltu", "sra")
              for d, a, b in itertools.product(sources[1:], sources, sources)]
        rv += [[op, d, a, k] for op in ("addi", "slti", "sltiu", "xori", "ori", "andi")
               for d, a in itertools.product(sources[1:], sources) for k in (-2048, -5, -1, 0, 1, 5, 0x555, 2047)]
        rv += [[op, d, a, k] for op in ("slli", "srli", "srai") for d, a in itertools.product(sources[1:], sources)
               for k in range(64)]
        for form in rv:
            code = encode_instruction(v.RV64, form) + bytes.fromhex("67800000")
            decoded = v.decode(v.RV64, code)
            for _ in range(2):
                state = {r: rng.choice(EDGE) if rng.random() < 0.3 else rng.getrandbits(64) for r in registers}
                concrete, _ = v.execute(v.RV64, code, state)
                symbolic = {r: z3.BitVecVal(value, 64, context) for r, value in state.items()}
                final, _ = v.machine_smt(z3, v.RV64, decoded, symbolic, {}, context)
                self.assertEqual({r: z3.simplify(value).as_long() for r, value in final.items()}, concrete, form)


@unittest.skipUnless(platform.system() == "Linux" and platform.machine() == "x86_64",
                     "actual Linux x86-64 execution required")
class X86FlagModelConformance(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tools = {}
        for name in ("gcc", "as", "objcopy", "nm"):
            path = shutil.which(name, path="/usr/bin:/bin")
            if path is None:
                raise RuntimeError("required GNU conformance tool is missing: " + name)
            cls.tools[name] = path
        cls.temporary = tempfile.TemporaryDirectory(prefix="zeno-machine-conformance2-")
        cls.root = Path(cls.temporary.name)
        cls.forms = forms()
        cls.code = [encode_instruction(v.X86, f) for f in cls.forms]
        rng = random.Random(20261031)
        cls.states = []
        for _ in cls.forms:
            per = [([0] * 9, dict.fromkeys(FLAG_BITS, 0)), ([MASK] * 9, dict.fromkeys(FLAG_BITS, 1))]
            per += [([rng.choice(EDGE) if rng.random() < 0.3 else rng.getrandbits(64) for _ in REGS],
                     {name: rng.getrandbits(1) for name in FLAG_BITS}) for _ in range(STATES - 2)]
            cls.states.append([(registers, flags, flags_word(flags, rng)) for registers, flags in per])
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
        # rbx is callee-saved and outside the profile; it points at {u64 in[9], rflags; u64 out[9], rflags}.
        # Register loads and stores are MOVs, which leave the flags unchanged.
        asm = [".intel_syntax noprefix", ".text"]
        for i, code in enumerate(cls.code):
            asm += [f".globl k{i}", f"k{i}:", "push rbx", "mov rbx, rdi", "push qword ptr [rbx + 72]", "popfq"]
            asm += [f"mov {r}, [rbx + {8 * j}]" for j, r in enumerate(REGS) if r != "rdi"]
            asm += [f"mov rdi, [rbx + {8 * REGS.index('rdi')}]", ".byte " + ",".join(map(str, code))]
            asm += ["pushfq", "pop qword ptr [rbx + 152]"]
            asm += [f"mov [rbx + {80 + 8 * j}], {r}" for j, r in enumerate(REGS)] + ["pop rbx", "ret"]
        (cls.root / "kernels.S").write_text("\n".join(asm) + "\n")
        n = len(cls.code)
        driver = ["#include <stdint.h>", "#include <stdio.h>", "typedef void (*k_t)(uint64_t*);"]
        driver += [f"void k{i}(uint64_t*);" for i in range(n)]
        driver += ["static k_t ks[] = {" + ",".join(f"k{i}" for i in range(n)) + "};",
                   "int main(void){uint64_t s[20];unsigned long long per;FILE*f=fopen(\"states.bin\",\"rb\");",
                   "FILE*o=fopen(\"hardware.bin\",\"wb\");if(!f||!o||fread(&per,8,1,f)!=1)return 2;",
                   "for(size_t i=0;i<sizeof ks/sizeof ks[0];i++)for(unsigned long long k=0;k<per;k++){",
                   "if(fread(s,8,10,f)!=10)return 3;ks[i](s);fwrite(s+10,8,10,o);}fclose(o);return 0;}"]
        (cls.root / "driver.c").write_text("\n".join(driver) + "\n")
        with open(cls.root / "states.bin", "wb") as handle:
            handle.write(struct.pack("<Q", STATES))
            for per in cls.states:
                for registers, _, word in per:
                    handle.write(struct.pack("<10Q", *registers, word))
        cls.run_tool([cls.tools["gcc"], "-O1", "-std=c11", "-Wl,-z,noexecstack", "driver.c", "kernels.S", "-o", "conf"])
        cls.run_tool([str(cls.root / "conf")])
        data = (cls.root / "hardware.bin").read_bytes()
        rows = []
        for i in range(n):
            per = []
            for k in range(STATES):
                *after, word = struct.unpack_from("<10Q", data, 8 * WORDS * (i * STATES + k))
                per.append((dict(zip(REGS, after)), {name: (word >> bit) & 1 for name, bit in FLAG_BITS.items()}))
            rows.append(per)
        return rows

    def mismatches(self, model):
        found = []
        for i, form in enumerate(self.forms):
            probe = self.code[i] + b"\xc3"
            for (registers, flags, _), (hardware, hardware_flags) in zip(self.states[i], self.hardware[i]):
                after, after_flags = model(form, probe, dict(zip(REGS, registers)), dict(flags))
                defined = {name: value for name, value in after_flags.items() if value is not None}
                if after != hardware or defined != {name: hardware_flags[name] for name in defined}:
                    found.append((form, registers, flags))
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
            self.assertEqual([i.form() for i in decoded[:-1]], [form])
            self.assertEqual(decoded[-1].op, "ret")

    def test_hardware_matches_concrete_registers_and_flags(self):
        model = lambda form, probe, registers, flags: v.execute(v.X86, probe, registers, flags)
        self.assertEqual(self.mismatches(model)[:5], [])

    def test_hardware_matches_smt_registers_and_flags(self):
        import z3
        context = z3.Context()
        for i, form in enumerate(self.forms):
            if form[0] in v.IMMEDIATE_OPS and isinstance(form[-1], int) and -128 <= form[-1] <= 127 \
                    and form[-1] not in SMT_IMM8:
                continue
            decoded = v.decode(v.X86, self.code[i] + b"\xc3")
            for (registers, flags, _), (hardware, hardware_flags) in list(zip(self.states[i], self.hardware[i]))[:SMT_STATES]:
                symbolic = {r: z3.BitVecVal(value, 64, context) for r, value in zip(REGS, registers)}
                truth = {name: z3.BoolVal(bool(value), context) for name, value in flags.items()}
                after, after_flags = v.machine_smt(z3, v.X86, decoded, symbolic, truth, context)
                self.assertEqual({r: z3.simplify(value).as_long() for r, value in after.items()}, hardware, form)
                for name, value in after_flags.items():
                    if value is not None:
                        self.assertEqual(int(z3.is_true(z3.simplify(value))), hardware_flags[name], (form, name))

    def test_planted_flag_and_register_defects_are_detected(self):
        exact = lambda form, probe, registers, flags: v.execute(v.X86, probe, registers, flags)

        def sbb_ignores_borrow_in_carry(form, probe, registers, flags):
            after, after_flags = exact(form, probe, registers, flags)
            if form[0] == "sbb":
                after_flags["cf"] = int(registers[form[1]] < registers[form[2]])
            return after, after_flags

        def add_overflow_is_carry(form, probe, registers, flags):
            after, after_flags = exact(form, probe, registers, flags)
            if form[0] == "add":
                after_flags["of"] = after_flags["cf"]
            return after, after_flags

        def setcc_writes_whole_register(form, probe, registers, flags):
            after, after_flags = exact(form, probe, registers, flags)
            if form[0] in v.SETCC:
                after[form[1]] &= 1
            return after, after_flags

        def cmovle_ignores_zero(form, probe, registers, flags):
            if form[0] == "cmovle":
                after, after_flags = exact(form, probe, registers, flags)
                after[form[1]] = registers[form[2]] if flags["sf"] != flags["of"] else registers[form[1]]
                return after, after_flags
            return exact(form, probe, registers, flags)

        def shl_carry_off_by_one(form, probe, registers, flags):
            after, after_flags = exact(form, probe, registers, flags)
            if form[0] == "shl" and form[2] > 0:
                after_flags["cf"] = (registers[form[1]] >> (63 - form[2])) & 1 if form[2] < 63 else 0
            return after, after_flags

        def immediate_zero_extended(form, probe, registers, flags):
            if form[0] in v.IMMEDIATE_OPS and isinstance(form[2], int) and form[2] < 0:
                width = 0xFF if form[2] >= -128 else 0xFFFFFFFF
                after, after_flags = dict(registers), dict(flags)
                v._x86_concrete(v.Instruction(form[0], (form[1], form[2] & width), 0, 0), after, after_flags)
                return after, after_flags
            return exact(form, probe, registers, flags)

        for name, model in (("immediate-zero-extended", immediate_zero_extended),
                            ("sbb-carry", sbb_ignores_borrow_in_carry), ("add-overflow", add_overflow_is_carry),
                            ("setcc-width", setcc_writes_whole_register), ("cmovle-zero", cmovle_ignores_zero),
                            ("shl-carry", shl_carry_off_by_one)):
            self.assertTrue(self.mismatches(model), f"planted defect not detected: {name}")
        out = os.environ.get("ZENO_MACHINE_CONFORMANCE_OUT")
        if out:
            Path(out).write_text(json.dumps({"schema": "zeno-fcis/x86-flag-model-conformance/1",
                                             "forms": len(self.forms), "states_per_form": STATES,
                                             "cpu": cpu_name()}) + "\n")


if __name__ == "__main__":
    unittest.main()
