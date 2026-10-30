"""Exact specialization of profile-2 problems and checked kernels for fixed input values.

`specialize` fixes some inputs of a profile-2 problem to constant words. The specialized problem keeps the
remaining inputs in their original order, renumbered from the first argument register, and its outputs are the
original outputs with the fixed inputs substituted and constants folded. Z3 must prove, for all remaining inputs,
that every specialized output equals the original output with those inputs fixed; unless the answer is unsat,
nothing is written.

With a kernel, the specializer also rewrites the kernel's decoded instructions, in the spirit of WiPEr: argument
moves for the renumbered inputs, the fixed inputs as constants, constant propagation through the instructions, and
removal of instructions whose results become constant or dead. The rewrite is advisory. The specialized bytes are
written only if the independent profile-2 checker finds them equivalent to the specialized problem, quantifying
every remaining input, scratch register and entry flag.

The claims hold under the registered ISA model. They do not establish native conformance, timing, optimality of
the rewritten kernel or any authority.
"""

from __future__ import annotations

from pathlib import Path

from . import profile2, verify2
from .model import MASK, canonical, digest, read_json, word
from .targets import encode_program
from .verify import Instruction

SCHEMA = "zeno-fcis/machine-specialization/1"
ASSURANCE = ("Z3 proof of output equality for all remaining inputs; kernel checked under the registered ISA model, "
             "not a kernel proof or deployment grant")
CLAIM = "for every assignment to the remaining inputs, each specialized output equals the original output with the fixed inputs set"
NONCLAIMS = "Not native conformance, timing, optimality of the rewritten kernel, other targets or authority."


def _require(condition, reason):
    if not condition:
        raise ValueError(reason)


def parse_fixes(problem: dict, fixes) -> dict:
    """{input name: word} from name=value texts; every name is a declared input, at most once."""
    _require(isinstance(fixes, (list, tuple)) and fixes, "SpecializationNeedsAFixedInput")
    fixed = {}
    for text in fixes:
        _require(isinstance(text, str) and text.count("=") == 1, "FixMustBeNameEqualsValue")
        name, value = text.split("=")
        _require(name in problem["inputs"], "FixedNameIsNotAnInput")
        _require(name not in fixed, "InputFixedTwice")
        fixed[name] = word(value)
    return fixed


# --- Problem specialization -------------------------------------------------------------------------------------

def _const(value: int) -> list:
    return ["const", str(value & MASK)]


def substitute(expr: list, fixed: dict) -> list:
    if expr[0] == "input":
        return _const(fixed[expr[1]]) if expr[1] in fixed else list(expr)
    if expr[0] == "const":
        return list(expr)
    if expr[0] in profile2.SHIFTS:
        return [expr[0], substitute(expr[1], fixed), expr[2]]
    return [expr[0], *(substitute(part, fixed) for part in expr[1:])]


def simplify(expr: list) -> list:
    """Bottom-up constant folding and identities whose exactness the specialization proof rechecks."""
    op = expr[0]
    if op in ("input", "const"):
        return list(expr)
    if op in profile2.SHIFTS:
        inner = simplify(expr[1])
        if expr[2] == 0:
            return inner
        node = [op, inner, expr[2]]
        return _const(profile2.evaluate(node, {})) if inner[0] == "const" else node
    parts = [simplify(part) for part in expr[1:]]
    node = [op, *parts]
    if all(part[0] == "const" for part in parts):
        return _const(profile2.evaluate(node, {}))
    value = lambda part: int(part[1]) if part[0] == "const" else None
    if op == "ite" and parts[0][0] == "const":
        return parts[1] if value(parts[0]) else parts[2]
    if op in ("add", "or", "xor") and value(parts[0]) == 0:
        return parts[1]
    if op in ("add", "sub", "or", "xor") and value(parts[1]) == 0:
        return parts[0]
    if op == "mul" and 1 in (value(parts[0]), value(parts[1])):
        return parts[1] if value(parts[0]) == 1 else parts[0]
    if op in ("mul", "and") and 0 in (value(parts[0]), value(parts[1])):
        return _const(0)
    if op == "and" and MASK in (value(parts[0]), value(parts[1])):
        return parts[1] if value(parts[0]) == MASK else parts[0]
    return node


def specialized_problem(problem: dict, fixed: dict) -> dict:
    remaining = [name for name in problem["inputs"] if name not in fixed]
    result = {"schema": problem["schema"], "target": problem["target"], "inputs": remaining,
              "outputs": [simplify(substitute(output, fixed)) for output in problem["outputs"]],
              "grammar": [list(form) for form in problem["grammar"]],
              "max_instructions": problem["max_instructions"], "objective": problem["objective"]}
    return profile2.validate_problem(result)


def prove(original: dict, fixed: dict, specialized: dict, timeout_ms: int = 1000) -> dict:
    """Z3 query asserting some output differs; unsat proves the specialization for all remaining inputs."""
    import z3 as z
    context = z.Context()
    remaining = {name: z.BitVec("spec_input_" + name, 64, ctx=context) for name in specialized["inputs"]}
    fixed_terms = {name: z.BitVecVal(value, 64, ctx=context) for name, value in fixed.items()}
    before = [verify2._ast_smt(z, output, {**remaining, **fixed_terms}, context) for output in original["outputs"]]
    after = [verify2._ast_smt(z, output, remaining, context) for output in specialized["outputs"]]
    solver = z.Solver(ctx=context)
    solver.set(timeout=timeout_ms)
    solver.add(z.Or([b != a for b, a in zip(before, after)]))
    query = solver.to_smt2()
    answer = solver.check()
    result = {"answer": str(answer), "solver": "z3 " + z.get_version_string(), "query": query,
              "query_sha256": digest(query.encode("ascii"))}
    if answer == z.sat:
        model = solver.model()
        result["counterexample"] = {name: str(model.eval(term, model_completion=True).as_long())
                                    for name, term in remaining.items()}
    elif answer != z.unsat:
        result["reason"] = solver.reason_unknown()
    return result


def recheck_proof(query: str, timeout_ms: int = 1000) -> str:
    """Re-run a stored proof query with the installed Z3; returns its answer."""
    import z3 as z
    context = z.Context()
    solver = z.Solver(ctx=context)
    solver.set(timeout=timeout_ms)
    solver.add(z.parse_smt2_string(query, ctx=context))
    return str(solver.check())


# --- Kernel specialization ---------------------------------------------------------------------------------------

def _x86_footprint(form):
    """(register reads, register writes, flag reads, flags defined, flags left undefined) of one x86 form."""
    op, args = form[0], form[1:]
    if op == "nop":
        return (), (), (), (), ()
    if op == "imm":
        return (), (args[0],), (), (), ()
    if op in ("mov", "movzx"):
        return (args[1],), (args[0],), (), (), ()
    if op == "lea":
        return (args[1], args[2]), (args[0],), (), (), ()
    ins = Instruction(op, tuple(args), 0, 0)
    flag_reads, defined, undefined = verify2.flag_effects(ins)
    if op in ("xor", "sub", "sbb") and args[0] == args[1]:
        reads = ()  # the result does not depend on the register's value
    elif op in verify2.SETCC or op in ("neg", "shl", "shr", "sar"):
        reads = (args[0],)
    else:
        reads = (args[0], args[1])
    writes = () if op in ("cmp", "test") else (args[0],)
    return reads, writes, flag_reads, defined, undefined


def _rv_footprint(form):
    op, args = form[0], form[1:]
    if op == "nop":
        return (), (), (), (), ()
    if op in ("sub", "xor") and args[1] == args[2]:
        reads = ()
    elif isinstance(args[2], str):
        reads = (args[1], args[2])
    else:
        reads = (args[1],)
    return tuple(r for r in reads if r != "zero"), (args[0],), (), (), ()


def footprint(target, form):
    return (_x86_footprint if target == verify2.X86 else _rv_footprint)(form)


def _instruction(form):
    operands = list(form[1:])
    if form[0] == "imm":
        operands[1] = int(operands[1])
    return Instruction(form[0], tuple(operands), 0, 0)


def fold(target, form, registers: dict, flags: dict):
    """Concrete result of one form whose reads are all known: (register values, flag values or None)."""
    # Registers the form does not read get a placeholder; the result does not depend on them.
    r = {name: registers.get(name) or 0 for name in verify2.REGISTERS[target]}
    if target == verify2.X86:
        f = {name: (flags[name] if isinstance(flags.get(name), int) else 0) for name in verify2.FLAGS}
        verify2._x86_concrete(_instruction(form), r, f)
    else:
        r["zero"] = 0
        f = {}
        verify2._rv_concrete(_instruction(form), r)
    _, writes, _, defined, undefined = footprint(target, form)
    return {name: r[name] for name in writes}, {**{n: f[n] for n in defined}, **{n: None for n in undefined}}


def materialize(target, register: str, value: int) -> list:
    """Forms that set one register to a constant without touching flags."""
    if target == verify2.X86:
        return [["imm", register, str(value)]]
    signed = value - (1 << 64) if value >> 63 else value

    def build(constant):
        if -2048 <= constant <= 2047:
            return [["addi", register, "zero", constant]]
        low = ((constant & 0xFFF) ^ 0x800) - 0x800
        high, shift = (constant - low) >> 12, 12
        while high and not high & 1 and shift < 63:
            high, shift = high >> 1, shift + 1
        forms = build(high) + [["slli", register, register, shift]]
        return forms + ([["addi", register, register, low]] if low else [])
    return build(signed)


def _copy(target, form):
    """(destination, source) when a form only copies one register to another, else None."""
    if target == verify2.X86 and form[0] == "mov":
        return form[1], form[2]
    if target == verify2.RV64 and form[0] == "addi" and form[3] == 0 and form[2] != "zero":
        return form[1], form[2]
    return None


def _identity(target, form, values, flags_dead):
    """The value an instruction leaves in its destination when known operands make it a copy or a constant.

    `values` maps each register operand to ("c", word) or a symbolic value. An x86 instruction qualifies only
    when the flags it defines are not read afterwards.
    """
    op, constant = form[0], lambda name: values[name][1] if values[name][0] == "c" else None
    if target == verify2.RV64:
        if op in ("slli", "srli", "srai", "addi", "ori", "xori") and form[3] == 0:
            return values[form[2]]
        if op == "andi" and form[3] == -1:
            return values[form[2]]
        if op == "andi" and form[3] == 0:
            return ("c", 0)
        if op not in ("add", "sub", "or", "xor", "and") or not isinstance(form[3], str):
            return None
        left, right = constant(form[2]), constant(form[3])
        if op in ("add", "sub", "or", "xor") and right == 0:
            return values[form[2]]
        if op in ("add", "or", "xor") and left == 0:
            return values[form[3]]
        if op == "and" and 0 in (left, right):
            return ("c", 0)
        if op == "and" and MASK in (left, right):
            return values[form[3]] if left == MASK else values[form[2]]
        return None
    if op in ("shl", "shr", "sar") and form[2] == 0:
        return values[form[1]]
    if not flags_dead:
        return None
    if op == "lea":
        base, index = constant(form[2]), constant(form[3])
        if index == 0:
            return values[form[2]]
        if base == 0 and form[4] == 1:
            return values[form[3]]
        return None
    if op not in ("add", "sub", "or", "xor", "and", "imul"):
        return None
    destination, source = constant(form[1]), constant(form[2])
    if (op in ("add", "sub", "or", "xor") and source == 0) or (op == "and" and source == MASK) \
            or (op == "imul" and source == 1):
        return values[form[1]]
    if (op in ("add", "or", "xor") and destination == 0) or (op == "and" and destination == MASK) \
            or (op == "imul" and destination == 1):
        return values[form[2]]
    if op in ("and", "imul") and 0 in (destination, source):
        return ("c", 0)
    return None


def _operands(target, form):
    """(source positions, destination position or None, whether the destination is also read)."""
    op = form[0]
    if target == verify2.RV64:
        if op in ("sub", "xor") and form[2] == form[3]:
            return (), 1, False
        return tuple(i for i in (2, 3) if i < len(form) and isinstance(form[i], str)), 1, False
    if op in ("xor", "sub", "sbb") and form[1] == form[2]:
        return (), 1, False  # the idiom's value does not depend on the register
    if op in ("cmp", "test"):
        return (1, 2), None, False
    if op == "lea":
        return (2, 3), 1, False
    if op in ("mov", "movzx"):
        return (2,), 1, False
    if op == "imm":
        return (), 1, False
    if op in verify2.SETCC or op in ("neg", "shl", "shr", "sar"):
        return (), 1, True
    return (2,), 1, True


def _flags_live(target, forms):
    """Flags live immediately before each position, and after the last."""
    live, result = set(), [set()]
    for form in reversed(forms):
        _, _, reads, defined, undefined = footprint(target, form)
        live = (live - set(defined) - set(undefined)) | set(reads)
        result.append(set(live))
    result.reverse()
    return result


def _zero_idioms(target, forms):
    """x86: replace a 10-byte zero immediate by 3-byte XOR at the earliest point where no flag is live."""
    if target != verify2.X86:
        return forms
    forms = [list(form) for form in forms]
    for index in range(len(forms)):
        form = forms[index]
        if form[0] != "imm" or form[2] != "0":
            continue
        register = form[1]
        rest = forms[:index] + forms[index + 1:]
        earliest = index
        while earliest > 0:
            reads, writes = footprint(target, rest[earliest - 1])[:2]
            if register in reads or register in writes or register in rest[earliest - 1][1:]:
                break
            earliest -= 1
        live = _flags_live(target, rest)
        for at in range(earliest, index + 1):
            if not live[at]:
                forms = rest[:at] + [["xor", register, register]] + rest[at:]
                break
    return forms


def specialize_kernel(problem: dict, code: bytes, fixed: dict, results: int) -> dict:
    """Advisory rewrite: constant propagation, folding and placement of the remaining work.

    Values are tracked rather than registers. The fixed inputs become constants; every instruction whose reads are
    all known is folded away, except that a kept instruction's flag producers stay. The kept instructions run in
    their original order, but each operand is taken from whichever register holds its value, and constants are set
    in a free register just before their first use, so renumbered arguments need no prologue of moves.
    """
    target = problem["target"]
    registers = verify2.REGISTERS[target]
    arguments, inputs = verify2.ARGUMENTS[target], problem["inputs"]
    remaining = [name for name in inputs if name not in fixed]
    body = [ins.form() for ins in verify2.decode(target, code)[:-1] if ins.op != "nop"]
    # Phase 1: the value of every register and flag in the original program with the fixed inputs set.
    value = {name: ("s", "entry:" + name) for name in registers}
    for index, name in enumerate(inputs):
        value[arguments[index]] = ("c", fixed[name]) if name in fixed else ("s", "in:" + name)
    if target == verify2.RV64:
        value["zero"] = ("c", 0)
    flags = {name: ("s", "flag:" + name) for name in verify2.FLAGS} if target == verify2.X86 else {}
    # Flags read later in the original body before being set again; a conservative liveness.
    live_after, live = [], set()
    for form in reversed(body):
        live_after.append(set(live))
        _, _, flag_reads, defined, undefined = footprint(target, form)
        live = (live - set(defined) - set(undefined)) | set(flag_reads)
    live_after.reverse()
    steps = []
    for index, form in enumerate(body):
        reads, writes, flag_reads, defined, undefined = footprint(target, form)
        operands = {name: value[name] for name in form[1:] if isinstance(name, str) and name in value}
        ready = all(value[r][0] == "c" for r in reads) and all(flags[f][0] == "c" for f in flag_reads)
        copy = _copy(target, form)
        same = None if ready or copy else _identity(target, form, operands, not set(defined) & live_after[index])
        if same is not None:
            # The instruction leaves a known operand value or a constant; its flags are never read.
            out = {writes[0]: same}
            flag_out = {n: ("u",) for n in defined + undefined}
            copy = (writes[0], None)
        elif ready:
            words, bits = fold(target, form, {r: value[r][1] for r in reads}, {f: flags[f][1] for f in flag_reads})
            out = {name: ("c", word) for name, word in words.items()}
            flag_out = {name: ("u",) if bit is None else ("c", bit) for name, bit in bits.items()}
        elif copy is not None:
            out, flag_out = {copy[0]: value[copy[1]]}, {}
        else:
            out = {name: ("s", f"{index}:{name}") for name in writes}
            flag_out = {**{n: ("s", f"{index}:{n}") for n in defined}, **{n: ("u",) for n in undefined}}
        steps.append({"form": form, "operands": operands, "reads": reads, "out": out,
                      "kept": not ready and copy is None})
        value.update(out)
        flags.update(flag_out)
    wanted = [value[name] for name in verify2.RESULTS[target][:results]]
    # A kept flag reader keeps the instruction that last set each flag it reads, to a fixpoint.
    changed = True
    while changed:
        changed = False
        for index, step in enumerate(steps):
            if not step["kept"]:
                continue
            for flag in footprint(target, step["form"])[2]:
                producer = next((j for j in range(index - 1, -1, -1)
                                 if flag in footprint(target, steps[j]["form"])[3] + footprint(target, steps[j]["form"])[4]), None)
                if producer is not None and not steps[producer]["kept"]:
                    steps[producer]["kept"] = changed = True
    # Remaining uses of each value: operands of kept instructions, then the results.
    uses = {}
    for step in steps:
        if step["kept"]:
            for name in set(step["reads"]):
                uses[step["operands"][name]] = uses.get(step["operands"][name], 0) + 1
    for item in wanted:
        uses[item] = uses.get(item, 0) + 1
    # Phase 2: placement. hw is what each hardware register holds in the specialized calling convention.
    hw = {name: ("s", "entry:" + name) for name in registers}
    for index in range(len(inputs)):
        hw[arguments[index]] = ("s", "unused:" + arguments[index])
    for index, name in enumerate(remaining):
        hw[arguments[index]] = ("s", "in:" + name)
    emitted = []
    # Prefer computing a result value directly in its result register.
    result_of = {}
    for name, item in zip(verify2.RESULTS[target][:results], wanted):
        result_of.setdefault(item, name)

    def holders(item):
        found = [name for name in registers if hw[name] == item]
        return found + (["zero"] if target == verify2.RV64 and item == ("c", 0) else [])

    def free(name, reserved):
        item = hw[name]
        return name not in reserved and (uses.get(item, 0) <= 0 or len(holders(item)) > 1)

    def pick(prefer, reserved):
        for name in ([prefer] if prefer in registers else []) + list(registers):
            if free(name, reserved):
                return name
        raise ValueError("NoFreeRegister")

    def put(item, name):
        """Set register `name` to value `item` without touching flags: a copy if one exists, else a constant."""
        if hw[name] == item:
            return
        sources = holders(item)
        if sources:
            emitted.append(["mov", name, sources[0]] if target == verify2.X86 else ["addi", name, sources[0], 0])
        else:
            _require(item[0] == "c", "ValueLost")
            emitted.extend(materialize(target, name, item[1]))
        hw[name] = item

    def place(item, prefer, reserved):
        found = holders(item)
        if found:
            return prefer if prefer in found else found[0]
        name = pick(prefer, reserved)
        put(item, name)
        return name

    # Hints: compute a value where a later read-modify-write of it, or the result, will need it.
    hint_of = dict(result_of)
    for step in reversed(steps):
        sources, destination, modifies = _operands(target, step["form"])
        if step["kept"] and modifies and step["out"].get(step["form"][destination]) in hint_of:
            hint_of.setdefault(step["operands"][step["form"][destination]],
                               hint_of[step["out"][step["form"][destination]]])
    for step in steps:
        if not step["kept"]:
            continue
        form = list(step["form"])
        sources, destination, modifies = _operands(target, form)
        values = [step["operands"][form[p]] for p in sources if form[p] != "zero"]
        for name in set(step["reads"]):
            uses[step["operands"][name]] -= 1
        # Until the instruction runs, every register holding one of its source values keeps it.
        protect = {name for name in registers if hw[name] in values}
        choice = None
        if destination is not None:
            original = form[destination]
            hint = hint_of.get(step["out"][original])
            if modifies:
                item = step["operands"][original]
                spare = [r for r in holders(item) if r in registers
                         and (uses.get(item, 0) <= 0 or len(holders(item)) > 1)]
                if hint is not None and (hint in spare or free(hint, protect)):
                    choice = hint
                elif spare:
                    choice = original if original in spare else spare[0]
                else:
                    choice = pick(original, protect)
                put(item, choice)
            else:
                choice = pick(hint or original, set())
        reserved = protect | ({choice} if modifies else set())
        for position in sources:
            if form[position] != "zero":
                form[position] = place(step["operands"][step["form"][position]], step["form"][position], reserved)
                reserved.add(form[position])
        if destination is not None:
            form[destination] = choice
            hw[choice] = step["out"][original]
        emitted.append(form)
    # The results, including a cycle between result registers through a free temporary.
    results_at = verify2.RESULTS[target][:results]
    pending = []
    for name, item in zip(results_at, wanted):
        if hw[name] == item:
            uses[item] -= 1
        else:
            pending.append((name, item))
    while pending:
        for at, (name, item) in enumerate(pending):
            if free(name, set()):
                put(item, name)
                uses[item] -= 1
                pending.pop(at)
                break
        else:
            name = pending[0][0]
            temporary = pick(None, {r for r, _ in pending})
            put(hw[name], temporary)
    # Dead-instruction removal by backward register and flag liveness, then the XOR zero idiom.
    live, live_flags, final = set(results_at), set(), []
    for form in reversed(emitted):
        reads, writes, flag_reads, defined, undefined = footprint(target, form)
        if not (set(writes) & live or set(defined) & live_flags):
            continue
        final.append(form)
        live = (live - set(writes)) | set(reads)
        live_flags = (live_flags - set(defined) - set(undefined)) | set(flag_reads)
    final = _zero_idioms(target, final[::-1])
    _require(len(final) <= 16, "SpecializedKernelExceeds16Instructions")
    return {"forms": final, "code": encode_program(target, final), "original_instructions": len(body),
            "folded": sum(1 for step in steps if not step["kept"])}


def _grammar(problem: dict, forms: list) -> list:
    grammar = [list(form) for form in problem["grammar"]]
    for form in forms:
        if form not in grammar:
            grammar.append(list(form))
    _require(len(grammar) <= 32, "SpecializedGrammarExceeds32Forms")
    return grammar


# --- Command ----------------------------------------------------------------------------------------------------

def specialize(problem: dict, fixes, out: Path | None, code: bytes | None = None, timeout_ms: int = 1000) -> dict:
    """Specialize a problem (and optionally a checked kernel); write `out` only after every check passes."""
    from .session import _json, _write
    _require(isinstance(problem, dict) and problem.get("schema") == profile2.SCHEMA,
             "SpecializationSupportsProfile2ProblemsOnly")
    original = profile2.validate_problem(problem)
    fixed = parse_fixes(original, fixes)
    specialized = specialized_problem(original, fixed)
    proof = prove(original, fixed, specialized, timeout_ms)
    if proof["answer"] != "unsat":
        return {"status": "counterexample" if proof["answer"] == "sat" else "inconclusive",
                "reason": "SpecializationProofFailed", "proof": {k: v for k, v in proof.items() if k != "query"},
                "written": False}
    remaining = specialized["inputs"]
    record = {"schema": SCHEMA, "authority": "none",
              "original_problem_sha256": digest(canonical(original)),
              "fixed": {name: str(value) for name, value in fixed.items()},
              "remaining": remaining,
              "argument_map": [[original["inputs"].index(name), k] for k, name in enumerate(remaining)],
              "proof": {"claim": CLAIM, "answer": "unsat", "solver": proof["solver"],
                        "query_sha256": proof["query_sha256"]},
              "kernel": None, "assurance": ASSURANCE, "nonclaims": NONCLAIMS}
    if code is not None:
        before = verify2.check(original, code, timeout_ms)
        if before["status"] != "equivalent":
            return {"status": "refused", "reason": "OriginalKernelDidNotCheck", "verification": before,
                    "written": False}
        rewrite = specialize_kernel(original, code, fixed, len(original["outputs"]))
        forms = [ins.form() for ins in verify2.decode(original["target"], rewrite["code"])[:-1]]
        specialized = profile2.validate_problem({**specialized, "grammar": _grammar(specialized, forms),
                                                 "max_instructions": max(specialized["max_instructions"], len(forms))})
        verdict = verify2.check(specialized, rewrite["code"], timeout_ms)
        if verdict["status"] != "equivalent":
            return {"status": verdict["status"], "reason": "SpecializedKernelDidNotCheck", "verification": verdict,
                    "code_hex": rewrite["code"].hex(), "written": False}
        record["kernel"] = {"original_code_sha256": digest(code), "specialized_code_sha256": digest(rewrite["code"]),
                            "original_cost": before["cost"], "specialized_cost": verdict["cost"],
                            "verification": "equivalent", "folded_instructions": rewrite["folded"],
                            "rewrite": "advisory; only the checker verdict counts"}
    record["specialized_problem_sha256"] = digest(canonical(specialized))
    result = {"status": "specialized", "record": record, "problem": specialized, "written": out is not None}
    if code is not None:
        result["code_hex"] = rewrite["code"].hex()
    if out is not None:
        out.mkdir(mode=0o700)
        _json(out / "original.json", original)
        _json(out / "problem.json", specialized)
        _write(out / "proof.smt2", proof["query"].encode("ascii"))
        if code is not None:
            _write(out / "original.bin", code)
            _write(out / "kernel.bin", rewrite["code"])
        _json(out / "specialization.json", record)
    return result


def replay(directory: Path, timeout_ms: int = 1000) -> dict:
    """Recompute a written specialization from its original inputs and recheck its proof and kernel."""
    from .session import _code
    _require(not directory.is_symlink() and directory.is_dir(), "SpecializationMustBeADirectory")
    record = read_json(directory / "specialization.json")
    original = read_json(directory / "original.json")
    fixes = [f"{name}={value}" for name, value in record.get("fixed", {}).items()]
    code = _code(directory / "original.bin") if (directory / "original.bin").exists() else None
    fresh = specialize(original, fixes, None, code, timeout_ms)
    if fresh["status"] != "specialized":
        return {"status": "inconclusive", "reason": "SpecializationDidNotRecompute", "detail": fresh}
    _require(fresh["record"] == record, "SpecializationRecordChanged")
    _require(canonical(fresh["problem"]) + b"\n" == (directory / "problem.json").read_bytes(),
             "SpecializedProblemChanged")
    if code is not None:
        _require(fresh["code_hex"] == _code(directory / "kernel.bin").hex(), "SpecializedKernelChanged")
    query = (directory / "proof.smt2").read_text("ascii")
    _require(digest(query.encode("ascii")) == record["proof"]["query_sha256"], "ProofQueryChanged")
    answer = recheck_proof(query, timeout_ms)
    if answer != "unsat":
        return {"status": "inconclusive", "reason": "StoredProofQueryDidNotRecheck", "answer": answer}
    return {"status": "replayed", "specialized_problem_sha256": record["specialized_problem_sha256"],
            "kernel": record["kernel"] is not None, "authority": "none"}
