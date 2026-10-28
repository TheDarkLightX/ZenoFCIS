"""Untrusted instruction proposals and deterministic native object emission.

The independent verifier decodes actual bytes without importing this module.
"""

from __future__ import annotations

import struct

from .model import TARGETS, integer, word

X86 = {"rax": 0, "rcx": 1, "rdx": 2, "rsi": 6, "rdi": 7,
       "r8": 8, "r9": 9, "r10": 10, "r11": 11}
RV = {"zero": 0, "t0": 5, "t1": 6, "a0": 10, "a1": 11}


def encode_instruction(target: str, insn: list) -> bytes:
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
    return code + (b"\xc3" if target == TARGETS[0] else bytes.fromhex("67800000"))


def assembly_source(code: bytes) -> bytes:
    text = ".text\n.globl zeno_kernel\n.type zeno_kernel,@function\nzeno_kernel:\n"
    text += ".byte " + ",".join(f"0x{value:02x}" for value in code) + "\n.size zeno_kernel,.-zeno_kernel\n"
    text += '.section .note.GNU-stack,"",@progbits\n'
    return text.encode("ascii")


def elf_object(target: str, code: bytes) -> bytes:
    """ELF64 little-endian relocatable function; no relocations or dependencies."""
    if target not in TARGETS or not isinstance(code, bytes) or not 1 <= len(code) <= 256:
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
                (names.index(b".text"), 1, 6, 0, text_offset, len(code), 0, 0, 16 if target == TARGETS[0] else 4, 0),
                (names.index(b".symtab"), 2, 0, 0, symbol_offset, 48, 3, 1, 8, 24),
                (names.index(b".strtab"), 3, 0, 0, string_offset, len(strings), 0, 0, 1, 0),
                (names.index(b".shstrtab"), 3, 0, 0, name_offset, len(names), 0, 0, 1, 0),
                (names.index(b".note.GNU-stack"), 1, 0, 0, 0, 0, 0, 0, 1, 0)]
    for row in sections:
        data.extend(struct.pack("<IIQQQQIIQQ", *row))
    ident = b"\x7fELF" + bytes([2, 1, 1, 0]) + bytes(8)
    data[:64] = struct.pack("<16sHHIQQQIHHHHHH", ident, 1, 62 if target == TARGETS[0] else 243,
                            1, 0, 0, section_offset, 0, 64, 0, 0, 64, len(sections), 4)
    return bytes(data)
