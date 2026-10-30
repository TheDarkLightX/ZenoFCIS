"""Independent profile-3 checker: straight-line x86-64 word kernels over declared memory regions.

Profile 3 admits every profile-2 x86 form and two memory forms: the 64-bit loads and stores
`mov r64, [base + disp]` and `mov [base + disp], r64`, with no displacement (ModRM mod 0) or a nonzero 8-bit
displacement (mod 1). The base must be a declared region's base register that no earlier instruction wrote. The
displacement must be a multiple of 8 inside the region, and a store must target a read-write region. Everything
else refuses before any solver call, with an exact reason.

Regions are modelled as separate word arrays: the static rules make every access an aligned word of exactly one
declared region, and the problem assumes the regions are pairwise disjoint and disjoint from the stack. The base
pointer values, every other admitted register, the four entry flags and every initial region word are quantified.
Non-memory instructions use the profile-2 models in `verify2.py` unchanged. This is not a proof of the hardware
ISA, of the disjointness assumption, of native conformance or timing.
"""
from __future__ import annotations

from . import profile3, verify2
from .verify import Instruction, _reg, _require

MASK = (1 << 64) - 1
X86 = profile3.TARGETS[0]
WORD = 8
REGISTERS = {X86: verify2.REGISTERS[verify2.X86]}
RESULTS = profile3.RESULTS
FLAGS = verify2.FLAGS
MEMORY = ("load", "store")


def _index(displacement):
    """The region word a byte displacement addresses; admission makes it an exact multiple of the word size."""
    return displacement // WORD


def _memory_form(code, at):
    """Decode one REX.W MOV with a memory operand at `at`, or None when the bytes are not that form."""
    if at + 3 > len(code) or not 0x48 <= code[at] <= 0x4F or code[at + 1] not in (0x8B, 0x89) or code[at + 2] >> 6 == 3:
        return None
    rex, opcode, modrm = code[at], code[at + 1], code[at + 2]
    _require(not (rex >> 1) & 1, "UnusedRexBits")
    mod, low = modrm >> 6, modrm & 7
    _require(mod in (0, 1) and low not in (4, 5), "UnsupportedAddressing")
    base = _reg(verify2._XREG, low + 8 * (rex & 1))
    data = _reg(verify2._XREG, ((modrm >> 3) & 7) + 8 * ((rex >> 2) & 1))
    size, displacement = 3, 0
    if mod == 1:
        _require(at + 4 <= len(code), "TruncatedInstruction")
        displacement = int.from_bytes(code[at + 3:at + 4], "little", signed=True)
        _require(displacement != 0, "NonCanonicalDisplacement")
        size = 4
    if opcode == 0x8B:
        return Instruction("load", (data, base, displacement), at, size)
    return Instruction("store", (base, displacement, data), at, size)


def _register_form(code, at):
    """One profile-2 instruction at `at`: the shortest slice that verify2 decodes as exactly it and a return."""
    for size in range(1, min(10, len(code) - at) + 1):
        try:
            decoded = verify2.decode(verify2.X86, code[at:at + size] + b"\xc3")
        except ValueError:
            continue
        if len(decoded) == 2 and decoded[0].size == size:
            return Instruction(decoded[0].op, decoded[0].operands, at, size)
    verify2.decode(verify2.X86, code[at:])  # raises the profile-2 reason for this instruction
    raise ValueError("UnsupportedOpcode")


def decode(code):
    """Consume an entire admitted function, including its unique exact return."""
    _require(type(code) is bytes and 0 < len(code) <= 256, "CodeMustBe1To256Bytes")
    result, at = [], 0
    while at < len(code):
        if code[at] == 0xC3:
            _require(at + 1 == len(code), "ReturnMustBeTerminal")
            result.append(Instruction("ret", (), at, 1))
            return result
        ins = _memory_form(code, at) or _register_form(code, at)
        result.append(ins)
        at += ins.size
        _require(len(result) <= 16, "TooManyBodyInstructions")
    raise ValueError("MissingTerminalReturn")


def _access(form):
    """(base register, byte displacement) of a load or store."""
    return (form[2], form[3]) if form[0] == "load" else (form[1], form[2])


def _admit_access(problem, op, base, displacement):
    regions = {region["base"]: region for region in problem["regions"]}
    _require(base in regions, "MemoryBaseNotARegion")
    region = regions[base]
    _require(type(displacement) is int and displacement % WORD == 0, "MisalignedAccess")
    _require(0 <= displacement < WORD * region["words"], "DisplacementOutsideRegion")
    _require(op == "load" or region["access"] == "read-write", "StoreToReadOnlyRegion")


def admit(problem, instructions):
    """The static memory rules, in order, over decoded instructions; the base must still hold its region base."""
    bases = {region["base"] for region in problem["regions"]}
    written = set()
    for ins in instructions[:-1]:
        if ins.op in MEMORY:
            base, displacement = _access(ins.form())
            _require(base in bases, "MemoryBaseNotARegion")
            _require(base not in written, "RedefinedRegionBase")
            _admit_access(problem, ins.op, base, displacement)
        if ins.op not in ("cmp", "test", "nop", "store"):
            written.add(ins.operands[0])
    verify2.require_defined_flag_reads(verify2.X86, instructions)


def _palette(problem):
    """Validate declared forms independently, including unused palette entries."""
    registers = set(REGISTERS[X86])
    plain = []
    for form in problem["grammar"]:
        if form[0] == "load":
            _require(len(form) == 4 and all(type(x) is str and x in registers for x in form[1:3])
                     and type(form[3]) is int, "InvalidDeclaredInstruction")
        elif form[0] == "store":
            _require(len(form) == 4 and type(form[1]) is str and form[1] in registers and type(form[2]) is int
                     and type(form[3]) is str and form[3] in registers, "InvalidDeclaredInstruction")
        else:
            plain.append(form)
            continue
        _admit_access(problem, form[0], *_access(form))
    verify2._palette(verify2.X86, plain)


def _layout(problem):
    inputs, bases = profile3.registers(problem)
    arguments = set(inputs.values()) | set(bases.values())
    return inputs, bases, [name for name in REGISTERS[X86] if name not in arguments]


def _step_concrete(ins, r, f, m, regions):
    if ins.op == "load":
        dst, base, displacement = ins.operands
        r[dst] = m[regions[base]][_index(displacement)]
    elif ins.op == "store":
        base, displacement, src = ins.operands
        m[regions[base]][_index(displacement)] = r[src]
    else:
        verify2._x86_concrete(ins, r, f)


def execute(problem, code, registers, flags, memory):
    """Concrete whole-state run: every admitted register, the four flags and every region word."""
    instructions = decode(code)
    admit(problem, instructions)
    _require(type(registers) is dict and set(registers) == set(REGISTERS[X86]), "ExactRegisterStateRequired")
    _require(type(flags) is dict and set(flags) == set(FLAGS), "ExactFlagStateRequired")
    _require(all(type(v) is int and 0 <= v <= MASK for v in registers.values()), "RegisterMustBeU64")
    _require(all(type(v) is int and v in (0, 1) for v in flags.values()), "FlagMustBe0Or1")
    sizes = {region["name"]: region["words"] for region in problem["regions"]}
    _require(type(memory) is dict and set(memory) == set(sizes), "ExactRegionStateRequired")
    for name, values in memory.items():
        _require(isinstance(values, (list, tuple)) and len(values) == sizes[name]
                 and all(type(v) is int and 0 <= v <= MASK for v in values), "RegionWordsMustBeU64")
    r, f, m = dict(registers), dict(flags), {name: list(values) for name, values in memory.items()}
    regions = {region["base"]: region["name"] for region in problem["regions"]}
    for ins in instructions[:-1]:
        _step_concrete(ins, r, f, m, regions)
    return r, f, m


def run_concrete(problem, code, inputs, bases, memory, initial=None, flags=None):
    """(results, final regions) for one sample; omitted scratch registers and flags are zero for this sample only."""
    input_registers, base_registers, scratch = _layout(problem)
    initial = {} if initial is None else initial
    _require(type(initial) is dict and set(initial) <= set(scratch), "InitialScratchFields")
    _require(type(inputs) is dict and set(inputs) == set(input_registers), "ExactInputsRequired")
    _require(type(bases) is dict and set(bases) == set(base_registers), "ExactBasesRequired")
    registers = {name: initial.get(name, 0) for name in scratch}
    registers.update({input_registers[n]: v for n, v in inputs.items()})
    registers.update({base_registers[n]: v for n, v in bases.items()})
    flags = {name: (flags or {}).get(name, 0) for name in FLAGS}
    r, _, m = execute(problem, code, registers, flags, memory)
    return [r[name] for name in RESULTS[X86][:len(problem["outputs"])]], m


def _step_smt(z, ins, r, f, m, regions, context):
    if ins.op == "load":
        dst, base, displacement = ins.operands
        r[dst] = m[regions[base]][_index(displacement)]
    elif ins.op == "store":
        base, displacement, src = ins.operands
        m[regions[base]][_index(displacement)] = r[src]
    else:
        verify2._x86_smt(z, ins, r, f, context)


def check(problem, code, timeout_ms=1000):
    """Refute a mismatch on any result or read-write region word over every admitted entry state."""
    try:
        p = profile3.validate_problem(problem)
        _require(type(timeout_ms) is int and 1 <= timeout_ms <= 60000, "TimeoutMustBe1To60000Ms")
        _palette(p)
        instructions = decode(code)
        _require(len(instructions) - 1 <= p["max_instructions"], "ProblemInstructionBound")
        _require(all(i.form() in p["grammar"] for i in instructions[:-1]), "InstructionOutsideGrammar")
        admit(p, instructions)
    except (ValueError, TypeError, KeyError, IndexError, OverflowError, RecursionError) as error:
        return {"status": "refused", "reason": str(error) or type(error).__name__}
    input_registers, base_registers, scratch = _layout(p)
    regions = {r["base"]: r["name"] for r in p["regions"]}
    writable = [r["name"] for r in p["regions"] if r["access"] == "read-write"]
    results = RESULTS[X86][:len(p["outputs"])]
    cost = {"bytes": len(code), "instructions": len(instructions) - 1}
    scope = {"word_bits": 64, "inputs": ",".join(f"{n}={r}" for n, r in input_registers.items()) or "none",
             "regions": ";".join(f"{r['name']}: base {r['base']}, {r['words']} words, {r['access']}"
                                 for r in p["regions"]),
             "results": ",".join(results) or "none",
             "quantified": "every input, base pointer value, initial region word, other admitted register "
                           "(" + ",".join(scratch) + ") and CF,ZF,SF,OF",
             "assumptions": "regions are pairwise disjoint, disjoint from the stack and 8-byte aligned at their "
                            "bases; read-only regions are not written by anything else during the call",
             "target": X86,
             "nonclaims": "Not full ISA, aliasing, faults, other flags, native conformance, timing or global optimality."}
    details = {"cost": cost, "decoded": [i.text() for i in instructions], "scope": scope}
    try:
        import z3 as z
    except ImportError:
        return {"status": "inconclusive", "reason": "Z3Unavailable", **details}
    try:
        details["z3_version"] = z.get_version_string()
        context = z.Context()
        word = lambda name: z.BitVec(name, 64, ctx=context)
        inputs = {n: word("mc_input_" + n) for n in input_registers}
        bases = {n: word("mc_base_" + n) for n in base_registers}
        initial = {n: word("mc_initial_" + n) for n in scratch}
        flags = {n: z.Bool("mc_flag_" + n, ctx=context) for n in FLAGS}
        memory = {r["name"]: [word(f"mc_region_{r['name']}_{i}") for i in range(r["words"])] for r in p["regions"]}
        registers = {**initial, **{input_registers[n]: v for n, v in inputs.items()},
                     **{base_registers[n]: v for n, v in bases.items()}}
        r, f, m = dict(registers), dict(flags), {name: list(values) for name, values in memory.items()}
        for ins in instructions[:-1]:
            _step_smt(z, ins, r, f, m, regions, context)
        env = {**inputs, **{profile3.slot(n, i): v for n, values in memory.items() for i, v in enumerate(values)}}
        outputs, expected_memory = profile3.lifted(p)
        expected = [verify2._ast_smt(z, e, env, context) for e in outputs]
        expected += [verify2._ast_smt(z, e, env, context) for n in writable for e in expected_memory[n]]
        actual = [r[name] for name in results] + [term for n in writable for term in m[n]]
        unchanged = [a == b for n in memory if n not in writable for a, b in zip(m[n], memory[n])]
        solver = z.Solver(ctx=context)
        solver.set(timeout=timeout_ms)
        solver.add(z.Or([e != a for e, a in zip(expected, actual)] + [z.Not(c) for c in unchanged]))
        answer = solver.check()
        if answer == z.unsat:
            return {"status": "equivalent", **details}
        if answer != z.sat:
            return {"status": "inconclusive", "reason": "SolverUnknown:" + solver.reason_unknown(), **details}
        model = solver.model()
        value = lambda term: model.eval(term, model_completion=True).as_long()
        witness_inputs = {n: value(v) for n, v in inputs.items()}
        witness_bases = {n: value(v) for n, v in bases.items()}
        entry = {n: value(v) for n, v in initial.items()}
        entry_flags = {n: int(z.is_true(model.eval(v, model_completion=True))) for n, v in flags.items()}
        entry_memory = {n: [value(v) for v in values] for n, values in memory.items()}
        e = [value(term) for term in expected]
        a = [value(term) for term in actual]
        replayed, final = run_concrete(p, code, witness_inputs, witness_bases, entry_memory, entry, entry_flags)
        want, want_memory = profile3.expected(p, witness_inputs, entry_memory)
        concrete_actual = replayed + [v for n in writable for v in final[n]]
        concrete_expected = want + [v for n in writable for v in want_memory[n]]
        read_only_kept = all(final[n] == entry_memory[n] for n in memory if n not in writable)
        if e != concrete_expected or a != concrete_actual or (e == a and read_only_kept):
            return {"status": "inconclusive", "reason": "CounterexampleReplayMismatch", **details}
        split = len(results)
        witness = {"inputs": {k: str(v) for k, v in witness_inputs.items()},
                   "bases": {k: str(v) for k, v in witness_bases.items()},
                   "initial_registers": {k: str(v) for k, v in entry.items()}, "initial_flags": entry_flags,
                   "initial_regions": {k: [str(v) for v in values] for k, values in entry_memory.items()},
                   "expected": [str(v) for v in e[:split]], "actual": [str(v) for v in a[:split]],
                   "expected_regions": {n: [str(v) for v in want_memory[n]] for n in writable},
                   "actual_regions": {n: [str(v) for v in final[n]] for n in memory}}
        return {"status": "counterexample", "reason": "Full64Mismatch", **witness, **details}
    except Exception as error:
        return {"status": "inconclusive", "reason": "SolverOrReplayError:" + str(error), **details}
