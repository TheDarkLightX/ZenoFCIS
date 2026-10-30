"""Exact instruction footprints and the search prunings they justify.

Each grammar instruction runs once through its profile checker's own symbolic semantics, from an entry state in
which every register (and, on x86, every modelled flag) is a distinct variable. For profile 3 every region word is
a location too, named `<region>[<index>]`, and a load or store also reads the base register it addresses through,
because admission requires that register to still hold its region's base. Each load or store also writes, and
overwrites, its own observed location `admitted:<form>`: removing an access could turn a refused candidate (an
access through a redefined base) into an admitted one, so an access is never dead unless an identical access
follows it. After simplification:

- an instruction *writes* every location whose value is not syntactically its entry variable (over-approximate);
- it *reads* every entry variable that occurs in a written value, plus the flags it is declared to read
  (over-approximate);
- it *overwrites* a written location whose own entry variable does not occur in the new value, and every x86 flag
  it leaves undefined (under-approximate).

Two candidate classes are then skipped during enumeration:

- **dead**: one instruction's every write is overwritten before it is read, or reaches the end unread and
  unobserved. Deleting that instruction gives a strictly cheaper sequence with the same results and the same
  admission, which the length-ordered enumeration has already proposed.
- **order**: two neighbouring instructions are independent (neither writes what the other reads or writes) and
  their bytes are out of order. Swapping them preserves the results and the cost. The byte-least ordering of each
  class of reorderings never has such a pair, so it is never skipped for this reason.

So every skipped candidate has a kept candidate of no greater cost with the same observed results, and a search that
exhausts its grammar reports the same cost-then-bytes optimum with or without pruning. Footprints only steer the
search; the checker still decides every candidate. A footprint that cannot be derived is never pruned against.
"""

from __future__ import annotations

from dataclasses import dataclass

from . import profile2
from .targets import encode_instruction, encode_program

SCHEME = "exact-footprint/1"


@dataclass(frozen=True)
class Footprint:
    reads: frozenset
    writes: frozenset
    overwrites: frozenset


def _variables(z, term) -> set:
    """Names of the uninterpreted constants in one Z3 term."""
    seen, found, stack = set(), set(), [term]
    while stack:
        node = stack.pop()
        if node.get_id() in seen:
            continue
        seen.add(node.get_id())
        if z.is_const(node) and node.decl().kind() == z.Z3_OP_UNINTERPRETED:
            found.add(node.decl().name())
        else:
            stack.extend(node.children())
    return found


def _effects(z, entry: dict, final: dict, declared_reads=()) -> Footprint:
    """Footprint from entry variables `loc_<name>` and final values; a final value of None is left undefined."""
    reads, writes, overwrites = set(declared_reads), set(), set()
    for name, start in entry.items():
        value = final[name]
        if value is None:
            writes.add(name)
            overwrites.add(name)
            continue
        value = z.simplify(value)
        if value.eq(start):
            continue
        used = {variable[4:] for variable in _variables(z, value)}
        writes.add(name)
        reads |= used
        if name not in used:
            overwrites.add(name)
    return Footprint(frozenset(reads), frozenset(writes), frozenset(overwrites))


def _profile1(z, context, target: str, form: list) -> Footprint:
    from . import verify
    x86 = target == verify.X86
    arguments = ("rdi", "rsi") if x86 else ("a0", "a1")
    inputs = {key: z.BitVec("loc_" + name, 64, ctx=context) for key, name in zip(("x", "y"), arguments)}
    scratch = {name: z.BitVec("loc_" + name, 64, ctx=context) for name in verify._SCRATCH[target]}
    entry = {**scratch, **{name: inputs[key] for key, name in zip(("x", "y"), arguments)}}
    decoded = verify.decode(target, encode_program(target, [form]))
    if len(decoded) != 2:
        raise ValueError("grammar entry did not decode to one instruction")
    final = {}
    for name in entry:
        # Profile 1 returns only its result register, so a move of `name` into it reads the final value.
        probe = (verify.Instruction("mov", ("rax", name), 0, 0) if x86
                 else verify.Instruction("add", ("a0", name, "zero"), 0, 0))
        final[name] = verify._machine_smt(z, target, [decoded[0], probe, decoded[1]], inputs, scratch)
    return _effects(z, entry, final)


def _profile2(z, context, target: str, form: list) -> Footprint:
    from . import verify2
    entry = {name: z.BitVec("loc_" + name, 64, ctx=context) for name in verify2.REGISTERS[target]}
    flags = ({name: z.Bool("loc_" + name, ctx=context) for name in verify2.FLAGS}
             if target == verify2.X86 else {})
    decoded = verify2.decode(target, encode_program(target, [form]))
    if len(decoded) != 2:
        raise ValueError("grammar entry did not decode to one instruction")
    final, final_flags = verify2.machine_smt(z, target, decoded, entry, flags, context)
    declared = verify2.flag_effects(decoded[0])[0] if target == verify2.X86 else ()
    return _effects(z, {**entry, **flags}, {**final, **final_flags}, declared)


def _word(region: str, index: int) -> str:
    return f"{region}[{index}]"


def _admitted(form: list) -> str:
    return "admitted:" + " ".join(map(str, form))


def _profile3(z, context, problem: dict, form: list) -> Footprint:
    from . import verify2, verify3
    target = problem["target"]
    word = lambda name: z.BitVec("loc_" + name, 64, ctx=context)
    entry = {name: word(name) for name in verify3.REGISTERS[target]}
    flags = {name: z.Bool("loc_" + name, ctx=context) for name in verify3.FLAGS} if target == verify3.X86 else {}
    memory = {r["name"]: [word(_word(r["name"], i)) for i in range(r["words"])] for r in problem["regions"]}
    decoded = verify3.decode(target, encode_program(target, [form]))
    if len(decoded) != 2:
        raise ValueError("grammar entry did not decode to one instruction")
    registers, final_flags = dict(entry), dict(flags)
    if target == verify3.RV64:
        registers["zero"] = z.BitVecVal(0, 64, ctx=context)
    final_memory = {name: list(words) for name, words in memory.items()}
    regions = {r["base"]: r["name"] for r in problem["regions"]}
    verify3._step_smt(z, target, decoded[0], registers, final_flags, final_memory, regions, context)
    registers.pop("zero", None)
    declared = verify2.flag_effects(decoded[0])[0] if target == verify3.X86 else ()
    if decoded[0].op in verify3.MEMORY:
        declared = (*declared, verify3._access(decoded[0].form())[0])
    locations = {_word(name, i): v for name, words in memory.items() for i, v in enumerate(words)}
    finals = {_word(name, i): v for name, words in final_memory.items() for i, v in enumerate(words)}
    effects = _effects(z, {**entry, **flags, **locations}, {**registers, **final_flags, **finals}, declared)
    if decoded[0].op not in verify3.MEMORY:
        return effects
    admitted = frozenset({_admitted(decoded[0].form())})
    return Footprint(effects.reads, effects.writes | admitted, effects.overwrites | admitted)


def footprints(problem: dict) -> list:
    """One Footprint per grammar entry, in grammar order; None where it cannot be derived."""
    import z3 as z
    context = z.Context()
    if problem["schema"] == "zeno-fcis/machine-problem/3":
        derive = lambda z, context, target, form: _profile3(z, context, problem, form)
    else:
        derive = _profile2 if problem["schema"] == profile2.SCHEMA else _profile1
    result = []
    for form in problem["grammar"]:
        try:
            result.append(derive(z, context, problem["target"], form))
        except (ValueError, KeyError, TypeError, IndexError):
            result.append(None)
    return result


def observed(problem: dict) -> frozenset:
    """Registers, and for profile 3 read-write region words, the checker compares at the end."""
    if problem["schema"] == "zeno-fcis/machine-problem/3":
        from . import verify3
        words = {_word(r["name"], i) for r in problem["regions"] if r["access"] == "read-write"
                 for i in range(r["words"])}
        accesses = {_admitted(form) for form in problem["grammar"] if form[0] in verify3.MEMORY}
        return frozenset({*verify3.RESULTS[problem["target"]][:len(problem["outputs"])], *words, *accesses})
    if problem["schema"] == profile2.SCHEMA:
        from . import verify2
        return frozenset(verify2.RESULTS[problem["target"]][:len(problem["outputs"])])
    from . import verify
    return frozenset({"rax" if problem["target"] == verify.X86 else "a0"})


class Pruner:
    """Judges index prefixes for `sequences`; see the module docstring for why each refusal is exact."""

    def __init__(self, problem: dict, encoded: list):
        self.prints = footprints(problem)
        self.observed = observed(problem)
        size = len(encoded)
        self.order = [[False] * size for _ in range(size)]
        for a, first in enumerate(self.prints):
            for b, second in enumerate(self.prints):
                if first is None or second is None:
                    continue
                independent = (not first.writes & (second.reads | second.writes)
                               and not second.writes & first.reads)
                self.order[a][b] = independent and encoded[b] + encoded[a] < encoded[a] + encoded[b]

    def dead(self, prefix, complete: bool) -> bool:
        """Some instruction's writes are all overwritten unread (or, when complete, also unobserved at the end)."""
        for position, index in enumerate(prefix):
            footprint = self.prints[index]
            if footprint is None:
                continue
            live = False
            for location in footprint.writes:
                for later in prefix[position + 1:]:
                    after = self.prints[later]
                    if after is None or location in after.reads:
                        live = True
                        break
                    if location in after.overwrites:
                        break
                else:
                    live = not complete or location in self.observed
                if live:
                    break
            if not live:
                return True
        return False

    def judge(self, prefix, length: int):
        """None to keep the prefix, else the reason every extension of it is skipped."""
        if len(prefix) >= 2 and self.order[prefix[-2]][prefix[-1]]:
            return "order"
        if self.dead(prefix, len(prefix) == length):
            return "dead"
        return None


def sequences(size: int, length: int, judge, skipped: dict):
    """Index tuples in itertools.product order, omitting every extension of a prefix that `judge` refuses.

    `skipped` counts the omitted complete sequences by reason.
    """
    prefix = []

    def walk():
        if len(prefix) == length:
            yield tuple(prefix)
            return
        for index in range(size):
            prefix.append(index)
            reason = judge(prefix, length)
            if reason:
                skipped[reason] = skipped.get(reason, 0) + size ** (length - len(prefix))
            else:
                yield from walk()
            prefix.pop()

    yield from walk()
