"""Untrusted instruction proposals and deterministic native object emission.

The independent verifier decodes actual bytes without importing this module.
"""

from __future__ import annotations

import struct

from .model import TARGETS, integer, word
from .profile2 import TARGETS as TARGETS2
from .profile3 import TARGETS as TARGETS3

X86 = {"rax": 0, "rcx": 1, "rdx": 2, "rsi": 6, "rdi": 7,
       "r8": 8, "r9": 9, "r10": 10, "r11": 11}
RV = {"zero": 0, "t0": 5, "t1": 6, "a0": 10, "a1": 11}
RV2 = {"zero": 0, "t0": 5, "t1": 6, "t2": 7, **{f"a{i}": 10 + i for i in range(8)}}
CONDITIONS = {"o": 0, "no": 1, "b": 2, "ae": 3, "e": 4, "ne": 5, "be": 6, "a": 7,
              "s": 8, "ns": 9, "l": 12, "ge": 13, "le": 14, "g": 15}


def is_x86(target: str) -> bool:
    return target in (TARGETS[0], TARGETS2[0], TARGETS3[0])


def _encode_x86_memory(insn: list) -> bytes:
    """REX.W MOV with a [base], or [base + disp8] for a nonzero disp8, memory operand."""
    op, *args = insn
    if op not in ("load", "store") or len(args) != 3:
        return _encode_x86_profile2(insn)
    data, base, displacement = (args[0], args[1], args[2]) if op == "load" else (args[2], args[0], args[1])
    data, base = X86[data], X86[base]
    displacement = integer(displacement, -128, 127, "displacement")
    rex = 0x48 | ((data >> 3) << 2) | (base >> 3)
    mod = 0 if displacement == 0 else 1
    encoded = bytes([rex, 0x8B if op == "load" else 0x89, (mod << 6) | ((data & 7) << 3) | (base & 7)])
    return encoded + (displacement.to_bytes(1, "little", signed=True) if mod else b"")


IMMEDIATE_GROUP = {"add": 0, "or": 1, "and": 4, "sub": 5, "xor": 6, "cmp": 7}
ACCUMULATOR = {"add": 0x05, "or": 0x0D, "and": 0x25, "sub": 0x2D, "xor": 0x35, "cmp": 0x3D}


def _encode_x86_immediate(op: str, register: str, value: int) -> bytes:
    """The canonical encoding: 83 /n ib when the value fits 8 signed bits, else the RAX short form or 81 /n id."""
    dst = X86[register]
    value = integer(value, -(1 << 31), (1 << 31) - 1, "sign-extended immediate")
    rex, modrm = 0x48 | (dst >> 3), 0xC0 | (IMMEDIATE_GROUP[op] << 3) | (dst & 7)
    if -128 <= value <= 127:
        return bytes([rex, 0x83, modrm]) + struct.pack("<b", value)
    if dst == 0:
        return bytes([0x48, ACCUMULATOR[op]]) + struct.pack("<i", value)
    return bytes([rex, 0x81, modrm]) + struct.pack("<i", value)


def _encode_x86_profile2(insn: list) -> bytes:
    op, *args = insn
    if op in IMMEDIATE_GROUP and len(args) == 2 and type(args[1]) is int:
        return _encode_x86_immediate(op, args[0], args[1])
    rm_reg = {"adc": 0x11, "sbb": 0x19, "cmp": 0x39, "test": 0x85}
    if op in rm_reg and len(args) == 2:
        rm, reg = (X86[a] for a in args)
        return bytes([0x48 | ((reg >> 3) << 2) | (rm >> 3), rm_reg[op], 0xC0 | ((reg & 7) << 3) | (rm & 7)])
    if (op == "movzx" or op[:4] == "cmov" and op[4:] in CONDITIONS) and len(args) == 2:
        dst, src = (X86[a] for a in args)
        second = 0xB6 if op == "movzx" else 0x40 + CONDITIONS[op[4:]]
        return bytes([0x48 | ((dst >> 3) << 2) | (src >> 3), 0x0F, second, 0xC0 | ((dst & 7) << 3) | (src & 7)])
    if op[:3] == "set" and op[3:] in CONDITIONS and len(args) == 1:
        dst = X86[args[0]]
        prefix = b"\x41" if dst >= 8 else b"\x40" if dst in (6, 7) else b""
        return prefix + bytes([0x0F, 0x90 + CONDITIONS[op[3:]], 0xC0 | (dst & 7)])
    if op == "neg" and len(args) == 1:
        dst = X86[args[0]]
        return bytes([0x48 | (dst >> 3), 0xF7, 0xD8 | (dst & 7)])
    if op == "sar" and len(args) == 2:
        dst = X86[args[0]]; shift = integer(args[1], 0, 63, "shift")
        prefix = 0x48 | (dst >> 3)
        return bytes([prefix, 0xD1, 0xF8 | (dst & 7)]) if shift == 1 else bytes([prefix, 0xC1, 0xF8 | (dst & 7), shift])
    return encode_instruction(TARGETS[0], insn)


def _encode_rv_memory(insn: list) -> bytes:
    """RV64I LD rd, disp(base) and SD src, disp(base); other forms as profile 2."""
    op, *args = insn
    if op not in ("load", "store") or len(args) != 3:
        return _encode_rv_profile2(insn)
    displacement = integer(args[2] if op == "load" else args[1], -2048, 2047, "displacement") & 0xFFF
    if op == "load":
        dst, base = RV2[args[0]], RV2[args[1]]
        if dst == 0:
            raise ValueError("only the exact NOP may write zero")
        return struct.pack("<I", (displacement << 20) | (base << 15) | (3 << 12) | (dst << 7) | 0x03)
    base, source = RV2[args[0]], RV2[args[2]]
    return struct.pack("<I", ((displacement >> 5) << 25) | (source << 20) | (base << 15) | (3 << 12)
                       | ((displacement & 31) << 7) | 0x23)


def _encode_rv_profile2(insn: list) -> bytes:
    op, *args = insn
    register = {"add": (0, 0), "sub": (0x20, 0), "xor": (0, 4), "and": (0, 7), "or": (0, 6),
                "slt": (0, 2), "sltu": (0, 3), "sra": (0x20, 5)}
    immediate = {"addi": 0, "slti": 2, "sltiu": 3, "xori": 4, "ori": 6, "andi": 7}
    shift = {"slli": (0, 1), "srli": (0, 5), "srai": (0x10, 5)}
    if len(args) != 3 or op not in {**register, **immediate, **shift}:
        raise ValueError("instruction is outside the registered proposal grammar")
    dst, left = RV2[args[0]], RV2[args[1]]
    if dst == 0:
        raise ValueError("only the exact NOP may write zero")
    if op in register:
        funct7, funct3 = register[op]
        return struct.pack("<I", (funct7 << 25) | (RV2[args[2]] << 20) | (left << 15) | (funct3 << 12) | (dst << 7) | 0x33)
    if op in immediate:
        value, funct3 = integer(args[2], -2048, 2047, "immediate") & 0xFFF, immediate[op]
    else:
        high, funct3 = shift[op]
        value = (high << 6) | integer(args[2], 0, 63, "shift")
    return struct.pack("<I", (value << 20) | (left << 15) | (funct3 << 12) | (dst << 7) | 0x13)


def encode_instruction(target: str, insn: list) -> bytes:
    if target in TARGETS3 and isinstance(insn, list) and insn:
        if insn == ["nop"]:
            return encode_instruction(TARGETS[TARGETS3.index(target)], insn)
        try:
            return (_encode_x86_memory if target == TARGETS3[0] else _encode_rv_memory)(insn)
        except (KeyError, TypeError) as error:
            raise ValueError("instruction uses an unregistered register or operand") from error
    if target in TARGETS2 and isinstance(insn, list) and insn:
        if insn == ["nop"]:
            return encode_instruction(TARGETS[TARGETS2.index(target)], insn)
        try:
            return (_encode_x86_profile2 if target == TARGETS2[0] else _encode_rv_profile2)(insn)
        except (KeyError, TypeError) as error:
            raise ValueError("instruction uses an unregistered register or operand") from error
    if target not in TARGETS or not isinstance(insn, list) or not insn:
        raise ValueError("unsupported target or instruction")
    op, *args = insn
    if op == "nop" and not args:
        return b"\x90" if target == TARGETS[0] else bytes.fromhex("13000000")
    try:
        if target == TARGETS[0]:
            if op in ("mov", "add", "sub", "and", "or", "xor", "imul") and len(args) == 2:
                dst, src = (X86[a] for a in args)
                if op == "imul":
                    return bytes([0x48 | ((dst >> 3) << 2) | (src >> 3), 0x0F, 0xAF,
                                  0xC0 | ((dst & 7) << 3) | (src & 7)])
                opcode = {"mov": 0x89, "add": 0x01, "sub": 0x29,
                          "and": 0x21, "or": 0x09, "xor": 0x31}[op]
                return bytes([0x48 | ((src >> 3) << 2) | (dst >> 3), opcode,
                              0xC0 | ((src & 7) << 3) | (dst & 7)])
            if op == "imm" and len(args) == 2:
                dst = X86[args[0]]
                return bytes([0x48 | (dst >> 3), 0xB8 + (dst & 7)]) + struct.pack("<Q", word(args[1]))
            if op in ("shl", "shr") and len(args) == 2:
                dst = X86[args[0]]; shift = integer(args[1], 0, 63, "shift")
                prefix = 0x48 | (dst >> 3)
                modrm = 0xC0 | ((4 if op == "shl" else 5) << 3) | (dst & 7)
                return bytes([prefix, 0xD1, modrm]) if shift == 1 else bytes([prefix, 0xC1, modrm, shift])
            if op == "lea" and len(args) == 4:
                dst, base, index = (X86[a] for a in args[:3])
                scale = args[3]
                if type(scale) is not int or scale not in (1, 2, 4, 8):
                    raise ValueError("LEA scale must be1,2,4 or8")
                rex = 0x48 | ((dst >> 3) << 2) | ((index >> 3) << 1) | (base >> 3)
                sib = ((scale.bit_length() - 1) << 6) | ((index & 7) << 3) | (base & 7)
                return bytes([rex, 0x8D, ((dst & 7) << 3) | 4, sib])
        else:
            if op in ("add", "sub", "xor", "and", "or", "sltu") and len(args) == 3:
                dst, left, right = (RV[a] for a in args)
                if dst == 0:
                    raise ValueError("only the exact NOP may write zero")
                funct3 = {"add": 0, "sub": 0, "xor": 4, "and": 7, "or": 6, "sltu": 3}[op]
                code = ((0x20 if op == "sub" else 0) << 25) | (right << 20) | (left << 15) | (funct3 << 12) | (dst << 7) | 0x33
                return struct.pack("<I", code)
            if op in ("addi", "slli", "srli") and len(args) == 3:
                dst, source = RV[args[0]], RV[args[1]]
                if dst == 0:
                    raise ValueError("only the exact NOP may write zero")
                immediate = integer(args[2], -2048, 2047, "ADDI immediate") if op == "addi" else integer(args[2], 0, 63, "shift")
                funct3 = {"addi": 0, "slli": 1, "srli": 5}[op]
                code = ((immediate & 0xFFF) << 20) | (source << 15) | (funct3 << 12) | (dst << 7) | 0x13
                return struct.pack("<I", code)
    except (KeyError, TypeError) as error:
        raise ValueError("instruction uses an unregistered register or operand") from error
    raise ValueError("instruction is outside the registered proposal grammar")


def encode_program(target: str, instructions: list) -> bytes:
    if not isinstance(instructions, list) or len(instructions) > 16:
        raise ValueError("program exceeds16 body instructions")
    code = b"".join(encode_instruction(target, instruction) for instruction in instructions)
    return code + (b"\xc3" if is_x86(target) else bytes.fromhex("67800000"))


def assembly_source(code: bytes) -> bytes:
    text = ".text\n.globl zeno_kernel\n.type zeno_kernel,@function\nzeno_kernel:\n"
    text += ".byte " + ",".join(f"0x{value:02x}" for value in code) + "\n.size zeno_kernel,.-zeno_kernel\n"
    text += '.section .note.GNU-stack,"",@progbits\n'
    return text.encode("ascii")


def elf_object(target: str, code: bytes) -> bytes:
    """ELF64 little-endian relocatable function; no relocations or dependencies."""
    if target not in TARGETS + TARGETS2 + TARGETS3 or not isinstance(code, bytes) or not 1 <= len(code) <= 256:
        raise ValueError("unsupported ELF target/code")
    names = b"\0.text\0.symtab\0.strtab\0.shstrtab\0.note.GNU-stack\0"
    strings = b"\0zeno_kernel\0"
    data = bytearray(64)
    text_offset = len(data); data.extend(code)
    while len(data) % 8:
        data.append(0)
    symbol_offset = len(data)
    data.extend(bytes(24))
    data.extend(struct.pack("<IBBHQQ", 1, 0x12, 0, 1, 0, len(code)))
    string_offset = len(data); data.extend(strings)
    name_offset = len(data); data.extend(names)
    while len(data) % 8:
        data.append(0)
    section_offset = len(data)
    sections = [(0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
                (names.index(b".text"), 1, 6, 0, text_offset, len(code), 0, 0, 16 if is_x86(target) else 4, 0),
                (names.index(b".symtab"), 2, 0, 0, symbol_offset, 48, 3, 1, 8, 24),
                (names.index(b".strtab"), 3, 0, 0, string_offset, len(strings), 0, 0, 1, 0),
                (names.index(b".shstrtab"), 3, 0, 0, name_offset, len(names), 0, 0, 1, 0),
                (names.index(b".note.GNU-stack"), 1, 0, 0, 0, 0, 0, 0, 1, 0)]
    for row in sections:
        data.extend(struct.pack("<IIQQQQIIQQ", *row))
    ident = b"\x7fELF" + bytes([2, 1, 1, 0]) + bytes(8)
    data[:64] = struct.pack("<16sHHIQQQIHHHHHH", ident, 1, 62 if is_x86(target) else 243,
                            1, 0, 0, section_offset, 0, 64, 0, 0, 64, len(sections), 4)
    return bytes(data)
