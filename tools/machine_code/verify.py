"""Independent, closed-profile byte decoder and modular-64 equivalence checker.

No encoder or search implementation is imported.  SMT checks quantify the
initial scratch registers as well as both arguments.  This is not a proof of
the hardware ISA, native conformance, timing, or an unrestricted ABI.
"""
from __future__ import annotations

from dataclasses import dataclass

MASK = (1 << 64) - 1
X86 = "x86_64-sysv-register/1"
RV64 = "rv64i-register/1"
_XREG = {0: "rax", 1: "rcx", 2: "rdx", 6: "rsi", 7: "rdi",
         8: "r8", 9: "r9", 10: "r10", 11: "r11"}
_RREG = {0: "zero", 5: "t0", 6: "t1", 10: "a0", 11: "a1"}
_SCRATCH = {X86: ("rax", "rcx", "rdx", "r8", "r9", "r10", "r11"),
            RV64: ("t0", "t1")}


def _require(condition, reason):
    if not condition:
        raise ValueError(reason)


@dataclass(frozen=True)
class Instruction:
    op: str
    operands: tuple
    offset: int
    size: int

    def form(self):
        args = list(self.operands)
        if self.op == "imm":
            args[1] = str(args[1])
        return [self.op, *args]

    def text(self):
        return self.op + (" " + ", ".join(map(str, self.operands)) if self.operands else "")


def _reg(table, index):
    _require(index in table, "RegisterOutsideProfile")
    return table[index]


def _palette(target, grammar):
    """Validate declared forms independently, including unused palette entries."""
    registers = set((_XREG if target == X86 else _RREG).values())
    writable = registers - ({"zero"} if target == RV64 else set())
    for form in grammar:
        op, args = form[0], form[1:]
        if op == "nop":
            valid = not args
        elif target == X86 and op in ("mov", "add", "sub", "and", "or", "xor", "imul"):
            valid = len(args) == 2 and all(type(x) is str and x in registers for x in args)
        elif target == X86 and op == "imm":
            valid = len(args) == 2 and type(args[0]) is str and args[0] in registers
            if valid:
                text = args[1]
                valid = (type(text) is str and 1 <= len(text) <= 20 and text.isascii()
                         and text.isdecimal() and (text == "0" or not text.startswith("0"))
                         and int(text) <= MASK)
        elif target == X86 and op in ("shl", "shr"):
            valid = (len(args) == 2 and type(args[0]) is str and args[0] in registers
                     and type(args[1]) is int and 0 <= args[1] <= 63)
        elif target == X86 and op == "lea":
            valid = (len(args) == 4 and all(type(x) is str and x in registers for x in args[:3])
                     and type(args[3]) is int and args[3] in (1, 2, 4, 8))
        elif target == RV64 and op in ("add", "sub", "xor", "and", "or", "sltu"):
            valid = (len(args) == 3 and type(args[0]) is str and args[0] in writable
                     and all(type(x) is str and x in registers for x in args[1:]))
        elif target == RV64 and op in ("addi", "slli", "srli"):
            valid = (len(args) == 3 and type(args[0]) is str and args[0] in writable
                     and type(args[1]) is str and args[1] in registers and type(args[2]) is int)
            if valid:
                valid = -2048 <= args[2] <= 2047 if op == "addi" else 0 <= args[2] <= 63
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
        _require(0x48 <= first <= 0x4F, "UnsupportedPrefixOrOpcode")
        rex_r, rex_x, rex_b = (first >> 2) & 1, (first >> 1) & 1, first & 1
        opcode = take(1)[0]
        if 0xB8 <= opcode <= 0xBF:
            _require(not rex_r and not rex_x, "UnusedRexBits")
            dst = _reg(_XREG, opcode - 0xB8 + 8 * rex_b)
            value = int.from_bytes(take(8), "little")
            op, operands = "imm", (dst, value)
        elif opcode in (0x89, 0x01, 0x29, 0x21, 0x09, 0x31, 0x0F):
            _require(not rex_x, "UnusedRexBits")
            imul = opcode == 0x0F
            if imul:
                _require(take(1)[0] == 0xAF, "UnsupportedTwoByteOpcode")
            modrm = take(1)[0]
            _require(modrm >> 6 == 3, "MemoryOperandRefused")
            reg = _reg(_XREG, ((modrm >> 3) & 7) + 8 * rex_r)
            rm = _reg(_XREG, (modrm & 7) + 8 * rex_b)
            if imul:
                op, operands = "imul", (reg, rm)
            else:
                op = {0x89: "mov", 0x01: "add", 0x29: "sub", 0x21: "and",
                      0x09: "or", 0x31: "xor"}[opcode]
                operands = (rm, reg)
        elif opcode in (0xC1, 0xD1):
            _require(not rex_r and not rex_x, "UnusedRexBits")
            modrm = take(1)[0]
            _require(modrm >> 6 == 3, "MemoryOperandRefused")
            extension = (modrm >> 3) & 7
            _require(extension in (4, 5), "UnsupportedShift")
            dst = _reg(_XREG, (modrm & 7) + 8 * rex_b)
            amount = 1 if opcode == 0xD1 else take(1)[0]
            _require(0 <= amount <= 63, "ShiftOutsideProfile")
            _require(opcode != 0xC1 or amount != 1, "OneBitShiftRequiresD1")
            op, operands = ("shl" if extension == 4 else "shr"), (dst, amount)
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
            b = _reg(_RREG, (word >> 20) & 31)
            signature = ((word >> 25) & 127, funct3)
            names = {(0, 0): "add", (32, 0): "sub", (0, 4): "xor",
                     (0, 7): "and", (0, 6): "or", (0, 3): "sltu"}
            _require(signature in names, "UnsupportedRegisterInstruction")
            op, operands = names[signature], (dst, a, b)
        elif funct3 == 0:
            immediate = word >> 20
            if immediate & 0x800:
                immediate -= 0x1000
            op, operands = "addi", (dst, a, immediate)
        elif funct3 in (1, 5):
            _require(word >> 26 == 0, "UnsupportedShiftEncoding")
            op = "slli" if funct3 == 1 else "srli"
            operands = (dst, a, (word >> 20) & 63)
        else:
            raise ValueError("UnsupportedImmediateInstruction")
        result.append(Instruction(op, operands, at, 4))
        _require(len(result) <= 16, "TooManyBodyInstructions")
    raise ValueError("MissingTerminalReturn")


def decode(target, code):
    """Consume an entire admitted function, including its unique exact return."""
    _require(type(target) is str and target in _SCRATCH, "UnsupportedTarget")
    _require(type(code) is bytes and 0 < len(code) <= 256, "CodeMustBe1To256Bytes")
    return _decode_x86(code) if target == X86 else _decode_rv64(code)


def _arguments(target, inputs, initial):
    _require(type(inputs) is dict and set(inputs) == {"x", "y"}, "ExactXYInputsRequired")
    for value in inputs.values():
        _require(type(value) is int and 0 <= value <= MASK, "InputMustBeU64")
    initial = {} if initial is None else initial
    _require(type(initial) is dict and set(initial) <= set(_SCRATCH[target]), "InitialScratchFields")
    for value in initial.values():
        _require(type(value) is int and 0 <= value <= MASK, "InitialScratchMustBeU64")
    registers = {name: initial.get(name, 0) for name in _SCRATCH[target]}
    if target == X86:
        registers.update(rdi=inputs["x"], rsi=inputs["y"])
    else:
        registers.update(a0=inputs["x"], a1=inputs["y"], zero=0)
    return registers


def run_concrete(target, code, inputs, initial=None):
    """Concrete sample; omitted scratch defaults to zero ONLY for this sample."""
    instructions = decode(target, code)
    r = _arguments(target, inputs, initial)
    for ins in instructions[:-1]:
        op, a = ins.op, ins.operands
        if op == "nop":
            continue
        dst = a[0]
        if target == X86:
            if op == "imm":
                value = a[1]
            elif op == "mov":
                value = r[a[1]]
            elif op == "lea":
                value = r[a[1]] + r[a[2]] * a[3]
            elif op in ("shl", "shr"):
                value = r[dst] << a[1] if op == "shl" else r[dst] >> a[1]
            else:
                left, right = r[dst], r[a[1]]
                if op == "add": value = left + right
                elif op == "sub": value = left - right
                elif op == "imul": value = left * right
                elif op == "and": value = left & right
                elif op == "or": value = left | right
                elif op == "xor": value = left ^ right
                else: raise ValueError("InternalConcreteOpcode")
        else:
            left = r[a[1]]
            if op == "addi": value = left + a[2]
            elif op == "slli": value = left << a[2]
            elif op == "srli": value = left >> a[2]
            else:
                right = r[a[2]]
                if op == "add": value = left + right
                elif op == "sub": value = left - right
                elif op == "and": value = left & right
                elif op == "or": value = left | right
                elif op == "xor": value = left ^ right
                elif op == "sltu": value = int(left < right)
                else: raise ValueError("InternalConcreteOpcode")
        r[dst] = value & MASK
    return r["rax" if target == X86 else "a0"]


def _ast_concrete(node, inputs):
    op = node[0]
    if op == "input": return inputs[node[1]]
    if op == "const": return int(node[1])
    a = _ast_concrete(node[1], inputs)
    if op == "not": return (~a) & MASK
    if op == "shl": return (a << node[2]) & MASK
    if op == "lshr": return a >> node[2]
    b = _ast_concrete(node[2], inputs)
    if op == "add": return (a + b) & MASK
    if op == "sub": return (a - b) & MASK
    if op == "mul": return (a * b) & MASK
    if op == "xor": return a ^ b
    if op == "and": return a & b
    if op == "or": return a | b
    raise ValueError("UnsupportedExpression")


def _ast_smt(z, node, inputs):
    op = node[0]
    if op == "input": return inputs[node[1]]
    if op == "const": return z.BitVecVal(int(node[1]), 64, ctx=inputs["x"].ctx)
    a = _ast_smt(z, node[1], inputs)
    if op == "not": return ~a
    if op == "shl": return a << node[2]
    if op == "lshr": return z.LShR(a, node[2])
    b = _ast_smt(z, node[2], inputs)
    if op == "add": return a + b
    if op == "sub": return a - b
    if op == "mul": return a * b
    if op == "xor": return a ^ b
    if op == "and": return a & b
    if op == "or": return a | b
    raise ValueError("UnsupportedExpression")


def _machine_smt(z, target, instructions, inputs, scratch):
    r = dict(scratch)
    if target == X86:
        r.update(rdi=inputs["x"], rsi=inputs["y"])
    else:
        r.update(a0=inputs["x"], a1=inputs["y"], zero=z.BitVecVal(0, 64, ctx=inputs["x"].ctx))
    for ins in instructions[:-1]:
        op, a = ins.op, ins.operands
        if op == "nop": continue
        dst = a[0]
        if target == X86:
            if op == "imm": value = z.BitVecVal(a[1], 64, ctx=inputs["x"].ctx)
            elif op == "mov": value = r[a[1]]
            elif op == "lea": value = r[a[1]] + r[a[2]] * a[3]
            elif op == "shl": value = r[dst] << a[1]
            elif op == "shr": value = z.LShR(r[dst], a[1])
            else:
                left, right = r[dst], r[a[1]]
                if op == "add": value = left + right
                elif op == "sub": value = left - right
                elif op == "imul": value = left * right
                elif op == "and": value = left & right
                elif op == "or": value = left | right
                elif op == "xor": value = left ^ right
                else: raise ValueError("InternalSmtOpcode")
        else:
            left = r[a[1]]
            if op == "addi": value = left + a[2]
            elif op == "slli": value = left << a[2]
            elif op == "srli": value = z.LShR(left, a[2])
            else:
                right = r[a[2]]
                if op == "add": value = left + right
                elif op == "sub": value = left - right
                elif op == "and": value = left & right
                elif op == "or": value = left | right
                elif op == "xor": value = left ^ right
                elif op == "sltu": value = z.If(z.ULT(left, right), z.BitVecVal(1, 64, ctx=inputs["x"].ctx), z.BitVecVal(0, 64, ctx=inputs["x"].ctx))
                else: raise ValueError("InternalSmtOpcode")
        r[dst] = value
    return r["rax" if target == X86 else "a0"]


def check(problem, code, timeout_ms=1000):
    """Refute a full-64 mismatch, including arbitrary entry scratch state."""
    try:
        from .model import validate_problem
    except ImportError:
        return {"status": "inconclusive", "reason": "ProblemValidatorUnavailable"}
    try:
        p = validate_problem(problem)
        _require(type(timeout_ms) is int and 1 <= timeout_ms <= 60000, "TimeoutMustBe1To60000Ms")
        _palette(p["target"], p["grammar"])
        instructions = decode(p["target"], code)
        _require(len(instructions) - 1 <= p["max_instructions"], "ProblemInstructionBound")
        grammar = p["grammar"]
        _require(all(i.form() in grammar for i in instructions[:-1]), "InstructionOutsideGrammar")
    except (ValueError, TypeError, KeyError, IndexError, OverflowError, RecursionError) as error:
        return {"status": "refused", "reason": str(error) or type(error).__name__}
    cost = {"bytes": len(code), "instructions": len(instructions) - 1}
    scope = {"word_bits": 64, "inputs": "all 2^128 unsigned x,y pairs",
             "initial_registers": "all 64-bit assignments to " + ",".join(_SCRATCH[p["target"]]),
             "target": p["target"], "nonclaims": "Not full ISA, native conformance, timing or global optimality."}
    details = {"cost": cost, "decoded": [i.text() for i in instructions], "scope": scope}
    try:
        import z3 as z
    except ImportError:
        return {"status": "inconclusive", "reason": "Z3Unavailable", **details}
    try:
        details["z3_version"] = z.get_version_string()
        context = z.Context()
        inputs = {n: z.BitVec("mc_input_" + n, 64, ctx=context) for n in ("x", "y")}
        scratch = {n: z.BitVec("mc_initial_" + n, 64, ctx=context) for n in _SCRATCH[p["target"]]}
        expected = _ast_smt(z, p["expression"], inputs)
        actual = _machine_smt(z, p["target"], instructions, inputs, scratch)
        solver = z.Solver(ctx=context)
        solver.set(timeout=timeout_ms)
        solver.add(expected != actual)
        answer = solver.check()
        if answer == z.unsat:
            return {"status": "equivalent", **details}
        if answer != z.sat:
            return {"status": "inconclusive", "reason": "SolverUnknown:" + solver.reason_unknown(), **details}
        model = solver.model()
        values = {n: model.eval(v, model_completion=True).as_long() for n, v in inputs.items()}
        initial = {n: model.eval(v, model_completion=True).as_long() for n, v in scratch.items()}
        e = model.eval(expected, model_completion=True).as_long()
        a = model.eval(actual, model_completion=True).as_long()
        if e != _ast_concrete(p["expression"], values) or a != run_concrete(p["target"], code, values, initial) or e == a:
            return {"status": "inconclusive", "reason": "CounterexampleReplayMismatch", **details}
        return {"status": "counterexample", "reason": "Full64Mismatch", "inputs": {k: str(v) for k, v in values.items()},
                "initial_registers": {k: str(v) for k, v in initial.items()}, "expected": str(e), "actual": str(a), **details}
    except Exception as error:
        return {"status": "inconclusive", "reason": "SolverOrReplayError:" + str(error), **details}
