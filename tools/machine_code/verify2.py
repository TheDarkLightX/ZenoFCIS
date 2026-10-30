"""Independent profile-2 byte decoder, flag model and modular-64 equivalence checker.

Profile 2 adds the x86 CF, ZF, SF and OF flags, carries, comparisons, SETcc,
CMOVcc and arithmetic shifts, and RV64I set-less-than, immediate logic and
arithmetic shifts, with up to six arguments and two results. PF, AF and every
other flag are unmodelled: no admitted instruction reads them, and an
instruction reading a flag that an earlier instruction left undefined refuses.

No encoder or search implementation is imported. SMT checks quantify every
argument, every other admitted register and every entry flag. This is not a
proof of the hardware ISA, native conformance, timing or an unrestricted ABI.
"""
from __future__ import annotations

from .verify import Instruction, _reg, _require

MASK = (1 << 64) - 1
SIGN = 1 << 63
X86 = "x86_64-sysv-register/2"
RV64 = "rv64i-register/2"
_XREG = {0: "rax", 1: "rcx", 2: "rdx", 6: "rsi", 7: "rdi",
         8: "r8", 9: "r9", 10: "r10", 11: "r11"}
_RREG = {0: "zero", 5: "t0", 6: "t1", 7: "t2", **{10 + i: f"a{i}" for i in range(8)}}
REGISTERS = {X86: tuple(_XREG.values()), RV64: tuple(n for n in _RREG.values() if n != "zero")}
ARGUMENTS = {X86: ("rdi", "rsi", "rdx", "rcx", "r8", "r9"), RV64: ("a0", "a1", "a2", "a3", "a4", "a5")}
RESULTS = {X86: ("rax", "rdx"), RV64: ("a0", "a1")}
FLAGS = ("cf", "zf", "sf", "of")
# Condition codes 0x0A/0x0B (parity) read the unmodelled PF and stay outside the profile.
CONDITIONS = {0x0: "o", 0x1: "no", 0x2: "b", 0x3: "ae", 0x4: "e", 0x5: "ne", 0x6: "be", 0x7: "a",
              0x8: "s", 0x9: "ns", 0xC: "l", 0xD: "ge", 0xE: "le", 0xF: "g"}
_READS = {"o": ("of",), "no": ("of",), "b": ("cf",), "ae": ("cf",), "e": ("zf",), "ne": ("zf",),
          "be": ("cf", "zf"), "a": ("cf", "zf"), "s": ("sf",), "ns": ("sf",),
          "l": ("sf", "of"), "ge": ("sf", "of"), "le": ("zf", "sf", "of"), "g": ("zf", "sf", "of")}
SETCC = {"set" + name: name for name in CONDITIONS.values()}
CMOVCC = {"cmov" + name: name for name in CONDITIONS.values()}
_ALU = {0x89: "mov", 0x01: "add", 0x29: "sub", 0x21: "and", 0x09: "or", 0x31: "xor",
        0x11: "adc", 0x19: "sbb", 0x39: "cmp", 0x85: "test"}
_SHIFT = {4: "shl", 5: "shr", 7: "sar"}
_X86_PAIR = set(_ALU.values()) | {"imul", "movzx"} | set(CMOVCC)
_RV_REGISTER = {(0, 0): "add", (32, 0): "sub", (0, 4): "xor", (0, 7): "and", (0, 6): "or",
                (0, 2): "slt", (0, 3): "sltu", (32, 5): "sra"}
_RV_IMMEDIATE = {0: "addi", 2: "slti", 3: "sltiu", 4: "xori", 6: "ori", 7: "andi"}


def _palette(target, grammar):
    """Validate declared forms independently, including unused palette entries."""
    registers = set(_XREG.values()) if target == X86 else set(_RREG.values())
    writable = registers - {"zero"}
    names = lambda args: all(type(x) is str and x in registers for x in args)
    for form in grammar:
        op, args = form[0], form[1:]
        if op == "nop":
            valid = not args
        elif target == X86 and op in _X86_PAIR:
            valid = len(args) == 2 and names(args)
        elif target == X86 and (op in SETCC or op == "neg"):
            valid = len(args) == 1 and names(args)
        elif target == X86 and op == "imm":
            valid = len(args) == 2 and names(args[:1])
            if valid:
                text = args[1]
                valid = (type(text) is str and 1 <= len(text) <= 20 and text.isascii()
                         and text.isdecimal() and (text == "0" or not text.startswith("0"))
                         and int(text) <= MASK)
        elif target == X86 and op in _SHIFT.values():
            valid = len(args) == 2 and names(args[:1]) and type(args[1]) is int and 0 <= args[1] <= 63
        elif target == X86 and op == "lea":
            valid = len(args) == 4 and names(args[:3]) and type(args[3]) is int and args[3] in (1, 2, 4, 8)
        elif target == RV64 and op in _RV_REGISTER.values():
            valid = len(args) == 3 and type(args[0]) is str and args[0] in writable and names(args[1:])
        elif target == RV64 and op in _RV_IMMEDIATE.values():
            valid = (len(args) == 3 and type(args[0]) is str and args[0] in writable and names(args[1:2])
                     and type(args[2]) is int and -2048 <= args[2] <= 2047)
        elif target == RV64 and op in ("slli", "srli", "srai"):
            valid = (len(args) == 3 and type(args[0]) is str and args[0] in writable and names(args[1:2])
                     and type(args[2]) is int and 0 <= args[2] <= 63)
        else:
            valid = False
        _require(valid, "InvalidDeclaredInstruction")


def _decode_x86(code):
    result = []
    at = 0

    def take(count):
        nonlocal at
        _require(at + count <= len(code), "TruncatedInstruction")
        data = code[at:at + count]
        at += count
        return data

    def register_modrm():
        modrm = take(1)[0]
        _require(modrm >> 6 == 3, "MemoryOperandRefused")
        return (modrm >> 3) & 7, modrm & 7

    while at < len(code):
        begin = at
        first = take(1)[0]
        if first == 0xC3:
            _require(at == len(code), "ReturnMustBeTerminal")
            result.append(Instruction("ret", (), begin, 1))
            return result
        if first == 0x90:
            result.append(Instruction("nop", (), begin, 1))
            _require(len(result) <= 16, "TooManyBodyInstructions")
            continue
        if first in (0x0F, 0x40, 0x41):
            # SETcc r/m8: no prefix for AL/CL/DL, exactly 0x40 for SIL/DIL, exactly 0x41 for R8B-R11B.
            if first != 0x0F:
                _require(take(1)[0] == 0x0F, "UnsupportedPrefixOrOpcode")
            opcode = take(1)[0]
            _require(0x90 <= opcode <= 0x9F and opcode - 0x90 in CONDITIONS, "UnsupportedByteOpcode")
            field, low = register_modrm()
            _require(field == 0, "SetccRegFieldMustBeZero")
            allowed = {0x0F: (0, 1, 2), 0x40: (6, 7), 0x41: (0, 1, 2, 3)}[first]
            _require(low in allowed, "ByteRegisterOutsideProfile")
            dst = _reg(_XREG, low + (8 if first == 0x41 else 0))
            op, operands = "set" + CONDITIONS[opcode - 0x90], (dst,)
        else:
            _require(0x48 <= first <= 0x4F, "UnsupportedPrefixOrOpcode")
            rex_r, rex_x, rex_b = (first >> 2) & 1, (first >> 1) & 1, first & 1
            opcode = take(1)[0]
            if 0xB8 <= opcode <= 0xBF:
                _require(not rex_r and not rex_x, "UnusedRexBits")
                dst = _reg(_XREG, opcode - 0xB8 + 8 * rex_b)
                op, operands = "imm", (dst, int.from_bytes(take(8), "little"))
            elif opcode in _ALU:
                _require(not rex_x, "UnusedRexBits")
                field, low = register_modrm()
                op = _ALU[opcode]
                operands = (_reg(_XREG, low + 8 * rex_b), _reg(_XREG, field + 8 * rex_r))
            elif opcode == 0x0F:
                _require(not rex_x, "UnusedRexBits")
                second = take(1)[0]
                if second == 0xAF:
                    op = "imul"
                elif second == 0xB6:
                    op = "movzx"
                elif 0x40 <= second <= 0x4F and second - 0x40 in CONDITIONS:
                    op = "cmov" + CONDITIONS[second - 0x40]
                else:
                    raise ValueError("UnsupportedTwoByteOpcode")
                field, low = register_modrm()
                operands = (_reg(_XREG, field + 8 * rex_r), _reg(_XREG, low + 8 * rex_b))
            elif opcode in (0xC1, 0xD1):
                _require(not rex_r and not rex_x, "UnusedRexBits")
                field, low = register_modrm()
                _require(field in _SHIFT, "UnsupportedShift")
                dst = _reg(_XREG, low + 8 * rex_b)
                amount = 1 if opcode == 0xD1 else take(1)[0]
                _require(0 <= amount <= 63, "ShiftOutsideProfile")
                _require(opcode != 0xC1 or amount != 1, "OneBitShiftRequiresD1")
                op, operands = _SHIFT[field], (dst, amount)
            elif opcode == 0xF7:
                _require(not rex_r and not rex_x, "UnusedRexBits")
                field, low = register_modrm()
                _require(field == 3, "UnsupportedGroup3")
                op, operands = "neg", (_reg(_XREG, low + 8 * rex_b),)
            elif opcode == 0x8D:
                modrm = take(1)[0]
                _require(modrm >> 6 == 0 and (modrm & 7) == 4, "LeaRequiresDisplacementFreeSib")
                sib = take(1)[0]
                dst = _reg(_XREG, ((modrm >> 3) & 7) + 8 * rex_r)
                base = _reg(_XREG, (sib & 7) + 8 * rex_b)
                index = _reg(_XREG, ((sib >> 3) & 7) + 8 * rex_x)
                op, operands = "lea", (dst, base, index, 1 << (sib >> 6))
            else:
                raise ValueError("UnsupportedOpcode")
        result.append(Instruction(op, operands, begin, at - begin))
        _require(len(result) <= 16, "TooManyBodyInstructions")
    raise ValueError("MissingTerminalReturn")


def _decode_rv64(code):
    _require(len(code) % 4 == 0, "UnalignedOrCompressedCode")
    result = []
    for at in range(0, len(code), 4):
        word = int.from_bytes(code[at:at + 4], "little")
        if word == 0x00008067:
            _require(at + 4 == len(code), "ReturnMustBeTerminal")
            result.append(Instruction("ret", (), at, 4))
            return result
        if word == 0x00000013:
            result.append(Instruction("nop", (), at, 4))
            _require(len(result) <= 16, "TooManyBodyInstructions")
            continue
        opcode = word & 0x7F
        _require(opcode in (0x33, 0x13), "UnsupportedOpcode")
        rd, funct3, rs1 = (word >> 7) & 31, (word >> 12) & 7, (word >> 15) & 31
        _require(rd != 0, "ZeroWriteHintRefused")
        dst, a = _reg(_RREG, rd), _reg(_RREG, rs1)
        if opcode == 0x33:
            signature = ((word >> 25) & 127, funct3)
            _require(signature in _RV_REGISTER, "UnsupportedRegisterInstruction")
            op, operands = _RV_REGISTER[signature], (dst, a, _reg(_RREG, (word >> 20) & 31))
        elif funct3 in _RV_IMMEDIATE:
            immediate = word >> 20
            if immediate & 0x800:
                immediate -= 0x1000
            op, operands = _RV_IMMEDIATE[funct3], (dst, a, immediate)
        else:
            # funct3 1 is SLLI with imm[11:6]=0; funct3 5 is SRLI (0) or SRAI (0b010000).
            shapes = {(1, 0): "slli", (5, 0): "srli", (5, 16): "srai"}
            _require((funct3, word >> 26) in shapes, "UnsupportedShiftEncoding")
            op, operands = shapes[(funct3, word >> 26)], (dst, a, (word >> 20) & 63)
        result.append(Instruction(op, operands, at, 4))
        _require(len(result) <= 16, "TooManyBodyInstructions")
    raise ValueError("MissingTerminalReturn")


def decode(target, code):
    """Consume an entire admitted function, including its unique exact return."""
    _require(type(target) is str and target in REGISTERS, "UnsupportedTarget")
    _require(type(code) is bytes and 0 < len(code) <= 256, "CodeMustBe1To256Bytes")
    return _decode_x86(code) if target == X86 else _decode_rv64(code)


def flag_effects(ins):
    """(flags read, flags defined, flags left undefined) by one decoded x86 instruction."""
    op = ins.op
    if op in ("add", "sub", "cmp", "neg", "and", "or", "xor", "test"):
        return (), FLAGS, ()
    if op in ("adc", "sbb"):
        return ("cf",), FLAGS, ()
    if op == "imul":
        return (), ("cf", "of"), ("zf", "sf")
    if op in ("shl", "shr", "sar"):
        count = ins.operands[1]
        return (), (() if count == 0 else FLAGS if count == 1 else ("cf", "zf", "sf")), (("of",) if count > 1 else ())
    if op in SETCC:
        return _READS[SETCC[op]], (), ()
    if op in CMOVCC:
        return _READS[CMOVCC[op]], (), ()
    return (), (), ()


def require_defined_flag_reads(target, instructions):
    """Entry flags are arbitrary but defined; a read of a flag left undefined refuses."""
    if target != X86:
        return
    defined = set(FLAGS)
    for ins in instructions:
        reads, written, undefined = flag_effects(ins)
        _require(set(reads) <= defined, "ReadsUndefinedFlag")
        defined = (defined | set(written)) - set(undefined)


def _signed(value):
    return value - (1 << 64) if value & SIGN else value


def _condition(name, f):
    for flag in _READS[name]:
        _require(f[flag] is not None, "ReadsUndefinedFlag")
    cf, zf, sf, of = (f[n] for n in FLAGS)
    return {"o": of, "no": not of, "b": cf, "ae": not cf, "e": zf, "ne": not zf,
            "be": cf or zf, "a": not cf and not zf, "s": sf, "ns": not sf,
            "l": sf != of, "ge": sf == of, "le": zf or sf != of, "g": not zf and sf == of}[name]


def _x86_concrete(ins, r, f):
    op, a = ins.op, ins.operands
    if op == "nop":
        return
    dst = a[0]
    if op == "imm":
        r[dst] = a[1]
    elif op == "mov":
        r[dst] = r[a[1]]
    elif op == "lea":
        r[dst] = (r[a[1]] + r[a[2]] * a[3]) & MASK
    elif op == "movzx":
        r[dst] = r[a[1]] & 0xFF
    elif op in CMOVCC:
        if _condition(CMOVCC[op], f):
            r[dst] = r[a[1]]
    elif op in SETCC:
        r[dst] = (r[dst] & ~0xFF & MASK) | int(_condition(SETCC[op], f))
    elif op == "neg":
        value = r[dst]
        result = -value & MASK
        f.update(cf=int(value != 0), of=int(value == SIGN))
        r[dst] = result
    elif op == "imul":
        left, right = r[dst], r[a[1]]
        full = _signed(left) * _signed(right)
        result = full & MASK
        f.update(cf=int(full != _signed(result)), of=int(full != _signed(result)), zf=None, sf=None)
        r[dst] = result
        return
    elif op in ("shl", "shr", "sar"):
        value, count = r[dst], a[1]
        if count == 0:
            return
        if op == "shl":
            result, carry = (value << count) & MASK, (value >> (64 - count)) & 1
            overflow = (result >> 63) ^ carry
        else:
            result = value >> count if op == "shr" else (_signed(value) >> count) & MASK
            carry = (value >> (count - 1)) & 1
            overflow = value >> 63 if op == "shr" else 0
        f.update(cf=carry, of=overflow if count == 1 else None)
        r[dst] = result
    else:
        left, right = r[dst], r[a[1]]
        if op in ("add", "adc"):
            carry_in = f["cf"] if op == "adc" else 0
            _require(carry_in is not None, "ReadsUndefinedFlag")
            total = left + right + carry_in
            result = total & MASK
            f.update(cf=int(total > MASK), of=((left ^ result) & (right ^ result)) >> 63)
        elif op in ("sub", "sbb", "cmp"):
            borrow = f["cf"] if op == "sbb" else 0
            _require(borrow is not None, "ReadsUndefinedFlag")
            result = (left - right - borrow) & MASK
            f.update(cf=int(left < right + borrow), of=((left ^ right) & (left ^ result)) >> 63)
        elif op in ("and", "test"):
            result = left & right
            f.update(cf=0, of=0)
        elif op == "or":
            result = left | right
            f.update(cf=0, of=0)
        elif op == "xor":
            result = left ^ right
            f.update(cf=0, of=0)
        else:
            raise ValueError("InternalConcreteOpcode")
        if op not in ("cmp", "test"):
            r[dst] = result
    if op in ("neg", "shl", "shr", "sar", "add", "adc", "sub", "sbb", "cmp", "and", "test", "or", "xor"):
        f.update(zf=int(result == 0), sf=result >> 63)


def _rv_concrete(ins, r):
    op, a = ins.op, ins.operands
    if op == "nop":
        return
    left = r[a[1]]
    if op in ("slli", "srli", "srai"):
        value = left << a[2] if op == "slli" else left >> a[2] if op == "srli" else _signed(left) >> a[2]
    elif op in _RV_IMMEDIATE.values():
        right = a[2] & MASK
        if op == "addi": value = left + right
        elif op == "slti": value = int(_signed(left) < a[2])
        elif op == "sltiu": value = int(left < right)
        elif op == "xori": value = left ^ right
        elif op == "ori": value = left | right
        elif op == "andi": value = left & right
        else: raise ValueError("InternalConcreteOpcode")
    else:
        right = r[a[2]]
        if op == "add": value = left + right
        elif op == "sub": value = left - right
        elif op == "and": value = left & right
        elif op == "or": value = left | right
        elif op == "xor": value = left ^ right
        elif op == "slt": value = int(_signed(left) < _signed(right))
        elif op == "sltu": value = int(left < right)
        elif op == "sra": value = _signed(left) >> (right & 63)
        else: raise ValueError("InternalConcreteOpcode")
    r[a[0]] = value & MASK


def execute(target, code, registers, flags=None):
    """Concrete whole-state step: every admitted register and, for x86, every modelled entry flag.

    Returns the final registers and flags; a flag an instruction left undefined is None.
    """
    instructions = decode(target, code)
    require_defined_flag_reads(target, instructions)
    _require(type(registers) is dict and set(registers) == set(REGISTERS[target]), "ExactRegisterStateRequired")
    for value in registers.values():
        _require(type(value) is int and 0 <= value <= MASK, "RegisterMustBeU64")
    if target == X86:
        _require(type(flags) is dict and set(flags) == set(FLAGS), "ExactFlagStateRequired")
        _require(all(type(value) is int and value in (0, 1) for value in flags.values()), "FlagMustBe0Or1")
        f = dict(flags)
    else:
        _require(flags is None, "RiscvHasNoFlags")
        f = {}
    r = dict(registers)
    if target == RV64:
        r["zero"] = 0
    for ins in instructions[:-1]:
        if target == X86:
            _x86_concrete(ins, r, f)
        else:
            _rv_concrete(ins, r)
    r.pop("zero", None)
    return r, f


def entry_state(target, arguments, initial=None, flags=None):
    """Whole entry state from positional arguments; omitted scratch registers and flags are zero."""
    _require(type(arguments) is list and 1 <= len(arguments) <= len(ARGUMENTS[target]), "ArgumentsMustBe1To6Words")
    for value in arguments:
        _require(type(value) is int and 0 <= value <= MASK, "InputMustBeU64")
    scratch = [name for name in REGISTERS[target] if name not in ARGUMENTS[target][:len(arguments)]]
    initial = {} if initial is None else initial
    _require(type(initial) is dict and set(initial) <= set(scratch), "InitialScratchFields")
    for value in initial.values():
        _require(type(value) is int and 0 <= value <= MASK, "InitialScratchMustBeU64")
    registers = {name: initial.get(name, 0) for name in scratch}
    registers.update(zip(ARGUMENTS[target], arguments))
    if target != X86:
        _require(flags is None, "RiscvHasNoFlags")
        return registers, None
    flags = {} if flags is None else flags
    _require(type(flags) is dict and set(flags) <= set(FLAGS), "InitialFlagFields")
    return registers, {name: flags.get(name, 0) for name in FLAGS}


def run_concrete(target, code, arguments, initial=None, flags=None, results=1):
    """Concrete sample; omitted scratch registers and flags default to zero ONLY for this sample."""
    _require(type(results) is int and results in (1, 2), "ResultsMustBe1Or2")
    _require(type(target) is str and target in REGISTERS, "UnsupportedTarget")
    registers, entry_flags = entry_state(target, arguments, initial, flags)
    final, _ = execute(target, code, registers, entry_flags)
    return [final[name] for name in RESULTS[target][:results]]


def _ast_concrete(node, inputs):
    op = node[0]
    if op == "input": return inputs[node[1]]
    if op == "const": return int(node[1])
    if op == "ite":
        return _ast_concrete(node[2], inputs) if _ast_concrete(node[1], inputs) != 0 else _ast_concrete(node[3], inputs)
    a = _ast_concrete(node[1], inputs)
    if op == "not": return (~a) & MASK
    if op == "shl": return (a << node[2]) & MASK
    if op == "lshr": return a >> node[2]
    if op == "ashr": return (_signed(a) >> node[2]) & MASK
    b = _ast_concrete(node[2], inputs)
    if op == "add": return (a + b) & MASK
    if op == "sub": return (a - b) & MASK
    if op == "mul": return (a * b) & MASK
    if op == "xor": return a ^ b
    if op == "and": return a & b
    if op == "or": return a | b
    if op == "ult": return int(a < b)
    if op == "ule": return int(a <= b)
    if op == "slt": return int(_signed(a) < _signed(b))
    if op == "sle": return int(_signed(a) <= _signed(b))
    if op == "eq": return int(a == b)
    raise ValueError("UnsupportedExpression")


def _ast_smt(z, node, inputs, context):
    word = lambda value: z.BitVecVal(value, 64, ctx=context)
    flag = lambda condition: z.If(condition, word(1), word(0))
    op = node[0]
    if op == "input": return inputs[node[1]]
    if op == "const": return word(int(node[1]))
    if op == "ite":
        return z.If(_ast_smt(z, node[1], inputs, context) != word(0),
                    _ast_smt(z, node[2], inputs, context), _ast_smt(z, node[3], inputs, context))
    a = _ast_smt(z, node[1], inputs, context)
    if op == "not": return ~a
    if op == "shl": return a << node[2]
    if op == "lshr": return z.LShR(a, node[2])
    if op == "ashr": return a >> node[2]
    b = _ast_smt(z, node[2], inputs, context)
    if op == "add": return a + b
    if op == "sub": return a - b
    if op == "mul": return a * b
    if op == "xor": return a ^ b
    if op == "and": return a & b
    if op == "or": return a | b
    if op == "ult": return flag(z.ULT(a, b))
    if op == "ule": return flag(z.ULE(a, b))
    if op == "slt": return flag(a < b)
    if op == "sle": return flag(a <= b)
    if op == "eq": return flag(a == b)
    raise ValueError("UnsupportedExpression")


def _smt_condition(z, name, f):
    for flag in _READS[name]:
        _require(f[flag] is not None, "ReadsUndefinedFlag")
    cf, zf, sf, of = (f[n] for n in FLAGS)
    return {"o": lambda: of, "no": lambda: z.Not(of), "b": lambda: cf, "ae": lambda: z.Not(cf),
            "e": lambda: zf, "ne": lambda: z.Not(zf), "be": lambda: z.Or(cf, zf),
            "a": lambda: z.And(z.Not(cf), z.Not(zf)), "s": lambda: sf, "ns": lambda: z.Not(sf),
            "l": lambda: sf != of, "ge": lambda: sf == of, "le": lambda: z.Or(zf, sf != of),
            "g": lambda: z.And(z.Not(zf), sf == of)}[name]()


def _x86_smt(z, ins, r, f, context):
    op, a = ins.op, ins.operands
    if op == "nop":
        return
    word = lambda value: z.BitVecVal(value, 64, ctx=context)
    bit = lambda value, index: z.Extract(index, index, value) == z.BitVecVal(1, 1, ctx=context)
    dst = a[0]
    if op in ("imm", "mov", "lea", "movzx") or op in CMOVCC or op in SETCC:
        if op == "imm":
            r[dst] = word(a[1])
        elif op == "mov":
            r[dst] = r[a[1]]
        elif op == "lea":
            r[dst] = r[a[1]] + r[a[2]] * a[3]
        elif op == "movzx":
            r[dst] = r[a[1]] & word(0xFF)
        elif op in CMOVCC:
            r[dst] = z.If(_smt_condition(z, CMOVCC[op], f), r[a[1]], r[dst])
        else:
            r[dst] = (r[dst] & word(MASK ^ 0xFF)) | z.If(_smt_condition(z, SETCC[op], f), word(1), word(0))
        return
    if op == "neg":
        value = r[dst]
        result = -value
        f.update(cf=value != word(0), of=value == word(SIGN))
        r[dst] = result
    elif op == "imul":
        left, right = r[dst], r[a[1]]
        result = left * right
        overflow = z.SignExt(64, left) * z.SignExt(64, right) != z.SignExt(64, result)
        f.update(cf=overflow, of=overflow, zf=None, sf=None)
        r[dst] = result
        return
    elif op in ("shl", "shr", "sar"):
        value, count = r[dst], a[1]
        if count == 0:
            return
        if op == "shl":
            result, carry = value << count, bit(value, 64 - count)
            overflow = z.Xor(bit(result, 63), carry)
        else:
            result = z.LShR(value, count) if op == "shr" else value >> count
            carry = bit(value, count - 1)
            overflow = bit(value, 63) if op == "shr" else z.BoolVal(False, ctx=context)
        f.update(cf=carry, of=overflow if count == 1 else None)
        r[dst] = result
    else:
        left, right = r[dst], r[a[1]]
        if op in ("add", "adc", "sub", "sbb", "cmp"):
            carry_in = f["cf"] if op in ("adc", "sbb") else z.BoolVal(False, ctx=context)
            _require(carry_in is not None, "ReadsUndefinedFlag")
            extend = lambda value: z.ZeroExt(1, value)
            one = z.If(carry_in, z.BitVecVal(1, 65, ctx=context), z.BitVecVal(0, 65, ctx=context))
            if op in ("add", "adc"):
                total = extend(left) + extend(right) + one
                result = z.Extract(63, 0, total)
                f.update(cf=bit(total, 64), of=bit((left ^ result) & (right ^ result), 63))
            else:
                result = left - right - z.Extract(63, 0, one)
                f.update(cf=z.ULT(extend(left), extend(right) + one), of=bit((left ^ right) & (left ^ result), 63))
        elif op in ("and", "test", "or", "xor"):
            result = left | right if op == "or" else left ^ right if op == "xor" else left & right
            f.update(cf=z.BoolVal(False, ctx=context), of=z.BoolVal(False, ctx=context))
        else:
            raise ValueError("InternalSmtOpcode")
        if op not in ("cmp", "test"):
            r[dst] = result
    f.update(zf=result == word(0), sf=bit(result, 63))


def _rv_smt(z, ins, r, context):
    op, a = ins.op, ins.operands
    if op == "nop":
        return
    word = lambda value: z.BitVecVal(value & MASK, 64, ctx=context)
    flag = lambda condition: z.If(condition, word(1), word(0))
    left = r[a[1]]
    if op == "slli": value = left << a[2]
    elif op == "srli": value = z.LShR(left, a[2])
    elif op == "srai": value = left >> a[2]
    elif op == "addi": value = left + word(a[2])
    elif op == "slti": value = flag(left < word(a[2]))
    elif op == "sltiu": value = flag(z.ULT(left, word(a[2])))
    elif op == "xori": value = left ^ word(a[2])
    elif op == "ori": value = left | word(a[2])
    elif op == "andi": value = left & word(a[2])
    else:
        right = r[a[2]]
        if op == "add": value = left + right
        elif op == "sub": value = left - right
        elif op == "and": value = left & right
        elif op == "or": value = left | right
        elif op == "xor": value = left ^ right
        elif op == "slt": value = flag(left < right)
        elif op == "sltu": value = flag(z.ULT(left, right))
        elif op == "sra": value = left >> (right & word(63))
        else: raise ValueError("InternalSmtOpcode")
    r[a[0]] = value


def machine_smt(z, target, instructions, registers, flags, context):
    """SMT whole-state step over the same decoded instructions; undefined flags are None."""
    r = dict(registers)
    f = dict(flags) if target == X86 else {}
    if target == RV64:
        r["zero"] = z.BitVecVal(0, 64, ctx=context)
    for ins in instructions[:-1]:
        if target == X86:
            _x86_smt(z, ins, r, f, context)
        else:
            _rv_smt(z, ins, r, context)
    r.pop("zero", None)
    return r, f


def check(problem, code, timeout_ms=1000):
    """Refute a full-64 mismatch on any result, including arbitrary entry registers and flags."""
    try:
        from .profile2 import validate_problem
    except ImportError:
        return {"status": "inconclusive", "reason": "ProblemValidatorUnavailable"}
    try:
        p = validate_problem(problem)
        _require(type(timeout_ms) is int and 1 <= timeout_ms <= 60000, "TimeoutMustBe1To60000Ms")
        target = p["target"]
        _palette(target, p["grammar"])
        instructions = decode(target, code)
        _require(len(instructions) - 1 <= p["max_instructions"], "ProblemInstructionBound")
        grammar = p["grammar"]
        _require(all(i.form() in grammar for i in instructions[:-1]), "InstructionOutsideGrammar")
        require_defined_flag_reads(target, instructions)
    except (ValueError, TypeError, KeyError, IndexError, OverflowError, RecursionError) as error:
        return {"status": "refused", "reason": str(error) or type(error).__name__}
    names = p["inputs"]
    arguments = ARGUMENTS[target][:len(names)]
    scratch = [name for name in REGISTERS[target] if name not in arguments]
    results = RESULTS[target][:len(p["outputs"])]
    cost = {"bytes": len(code), "instructions": len(instructions) - 1}
    scope = {"word_bits": 64, "inputs": f"all 2^{64 * len(names)} unsigned assignments to " + ",".join(names),
             "arguments": ",".join(f"{n}={r}" for n, r in zip(names, arguments)),
             "results": ",".join(results),
             "initial_registers": "all 64-bit assignments to " + ",".join(scratch),
             "initial_flags": "all assignments to CF,ZF,SF,OF; PF, AF and other flags unmodelled and never read"
             if target == X86 else "none",
             "target": target,
             "nonclaims": "Not full ISA, memory, other flags, native conformance, timing or global optimality."}
    details = {"cost": cost, "decoded": [i.text() for i in instructions], "scope": scope}
    try:
        import z3 as z
    except ImportError:
        return {"status": "inconclusive", "reason": "Z3Unavailable", **details}
    try:
        details["z3_version"] = z.get_version_string()
        context = z.Context()
        inputs = {n: z.BitVec("mc_input_" + n, 64, ctx=context) for n in names}
        initial = {n: z.BitVec("mc_initial_" + n, 64, ctx=context) for n in scratch}
        flags = {n: z.Bool("mc_flag_" + n, ctx=context) for n in FLAGS} if target == X86 else {}
        registers = {**initial, **{r: inputs[n] for n, r in zip(names, arguments)}}
        expected = [_ast_smt(z, output, inputs, context) for output in p["outputs"]]
        final, _ = machine_smt(z, target, instructions, registers, flags, context)
        actual = [final[name] for name in results]
        solver = z.Solver(ctx=context)
        solver.set(timeout=timeout_ms)
        solver.add(z.Or([e != a for e, a in zip(expected, actual)]))
        answer = solver.check()
        if answer == z.unsat:
            return {"status": "equivalent", **details}
        if answer != z.sat:
            return {"status": "inconclusive", "reason": "SolverUnknown:" + solver.reason_unknown(), **details}
        model = solver.model()
        value = lambda term: model.eval(term, model_completion=True)
        values = {n: value(v).as_long() for n, v in inputs.items()}
        entry = {n: value(v).as_long() for n, v in initial.items()}
        entry_flags = {n: int(z.is_true(value(v))) for n, v in flags.items()}
        e = [value(term).as_long() for term in expected]
        a = [value(term).as_long() for term in actual]
        replayed = run_concrete(target, code, [values[n] for n in names], entry,
                                entry_flags if target == X86 else None, len(results))
        if e != [_ast_concrete(output, values) for output in p["outputs"]] or a != replayed or e == a:
            return {"status": "inconclusive", "reason": "CounterexampleReplayMismatch", **details}
        witness = {"inputs": {k: str(v) for k, v in values.items()},
                   "initial_registers": {k: str(v) for k, v in entry.items()},
                   "expected": [str(v) for v in e], "actual": [str(v) for v in a]}
        if target == X86:
            witness["initial_flags"] = entry_flags
        return {"status": "counterexample", "reason": "Full64Mismatch", **witness, **details}
    except Exception as error:
        return {"status": "inconclusive", "reason": "SolverOrReplayError:" + str(error), **details}
