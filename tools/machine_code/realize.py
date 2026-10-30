"""Solver unrealizability: is there an admitted program of length k with cost below a bound?

For one length k, each of the k slots chooses a grammar form through a bounded selector over exactly the problem's
palette (its `grammar`, in order, the list the enumerator walks). Each form's semantics come from the profile
checker's own SMT functions, and the next machine state is an If chain over the selector. One query asks for a
selector assignment that is admitted, stays under the cost bound and gives the specified results on every example
state. It is solved by counterexample-guided refinement:

- a satisfying assignment is decoded to bytes and decided by the independent checker; a counterexample becomes a new
  example and the query is asked again;
- unsat on a finite example set means no selector assignment meets even those states, so no admitted program of
  length k with cost below the bound exists under the stated model and palette;
- budget exhaustion, a solver `unknown` or an inconclusive check is inconclusive, never unrealizable.

Exactness of an unsat rests on the encoding ranging over exactly the enumerator's candidate space: the same forms,
the same length, the profile-2 undefined-flag refusal as a constraint, and the checker's own semantics. The tests
cross-check every verdict against exhaustive exact enumeration on small problems. A recorded unsat binds the
query's SMT-LIB text by hash, together with the examples that produced it, so `replay` rebuilds and re-solves it.
Nothing here is a claim of global optimality.
"""

from __future__ import annotations

import time

from . import profile2
from .model import canonical, digest, integer
from .targets import encode_instruction, encode_program

SCHEMA = "zeno-fcis/machine-realize/1"
STATEMENT = ("no admitted program of length {length} with {objective} cost below {bound} exists "
             "under the stated model and palette")
MAX_ROUNDS = 64


class _Profile1:
    """Profile 1: registers only; each form's effect on every register from the checker's own SMT run."""

    def __init__(self, z, context, problem):
        from . import verify
        self.verify, self.problem, self.target = verify, problem, problem["target"]
        x86 = self.target == verify.X86
        self.arguments = ("rdi", "rsi") if x86 else ("a0", "a1")
        self.locations = list(verify._SCRATCH[self.target]) + list(self.arguments)
        self.result = "rax" if x86 else "a0"
        self.flags = ()
        entry = {name: z.BitVec("realize_entry_" + name, 64, ctx=context) for name in self.locations}
        inputs = {"x": entry[self.arguments[0]], "y": entry[self.arguments[1]]}
        scratch = {name: entry[name] for name in verify._SCRATCH[self.target]}
        self.entry, self.transfers, self.reads = entry, [], []
        for form in problem["grammar"]:
            decoded = verify.decode(self.target, encode_program(self.target, [form]))
            if len(decoded) != 2:
                raise ValueError("grammar entry did not decode to one instruction")
            final = {}
            for name in self.locations:
                # Profile 1 returns only its result register; a final move of `name` into it reads that location.
                probe = (verify.Instruction("mov", ("rax", name), 0, 0) if x86
                         else verify.Instruction("add", ("a0", name, "zero"), 0, 0))
                final[name] = verify._machine_smt(z, self.target, [decoded[0], probe, decoded[1]], inputs, scratch)
            self.transfers.append(final)
            self.reads.append(())

    def start(self, z, context, example):
        arguments, initial, _ = example
        registers = self.verify._arguments(self.target, {"x": arguments[0], "y": arguments[1]}, initial)
        return {name: z.BitVecVal(registers[name], 64, ctx=context) for name in self.locations}, {}

    def step(self, z, context, index, registers, flags):
        pairs = [(self.entry[name], registers[name]) for name in self.locations]
        return {name: z.substitute(self.transfers[index][name], *pairs) for name in self.locations}, {}

    def flag_effects(self, index):
        return (), (), ()

    def results(self, registers):
        return [registers[self.result]]

    def expected(self, example):
        arguments = example[0]
        return [self.verify._ast_concrete(self.problem["expression"], {"x": arguments[0], "y": arguments[1]})]


class _Profile2:
    """Profile 2: registers and, on x86, CF, ZF, SF and OF; one checker step per slot from the current state."""

    def __init__(self, z, context, problem):
        from . import verify2
        self.verify2, self.problem, self.target = verify2, problem, problem["target"]
        self.x86 = self.target == verify2.X86
        self.locations = list(verify2.REGISTERS[self.target])
        self.flags = verify2.FLAGS if self.x86 else ()
        self.results_names = verify2.RESULTS[self.target][:len(problem["outputs"])]
        self.decoded, self.effects = [], []
        for form in problem["grammar"]:
            decoded = verify2.decode(self.target, encode_program(self.target, [form]))
            if len(decoded) != 2:
                raise ValueError("grammar entry did not decode to one instruction")
            self.decoded.append(decoded)
            self.effects.append(verify2.flag_effects(decoded[0]) if self.x86 else ((), (), ()))

    def start(self, z, context, example):
        arguments, initial, flags = example
        registers, entry_flags = self.verify2.entry_state(self.target, list(arguments), initial, flags)
        return ({name: z.BitVecVal(registers[name], 64, ctx=context) for name in self.locations},
                {name: z.BoolVal(bool(entry_flags[name]), ctx=context) for name in self.flags})

    def step(self, z, context, index, registers, flags):
        final, final_flags = self.verify2.machine_smt(z, self.target, self.decoded[index], registers, flags, context)
        # A flag the form leaves undefined is never read by an admitted program (see `admitted`), so its
        # placeholder value cannot reach a result.
        placeholder = z.BoolVal(False, ctx=context)
        return ({name: final[name] for name in self.locations},
                {name: placeholder if final_flags.get(name) is None else final_flags[name] for name in self.flags})

    def flag_effects(self, index):
        return self.effects[index]

    def results(self, registers):
        return [registers[name] for name in self.results_names]

    def expected(self, example):
        values = dict(zip(self.problem["inputs"], example[0]))
        return [self.verify2._ast_concrete(output, values) for output in self.problem["outputs"]]


def _profile(z, context, problem):
    if problem["schema"] == profile2.SCHEMA:
        return _Profile2(z, context, problem)
    if problem["schema"] == "zeno-fcis/machine-problem/1":
        return _Profile1(z, context, problem)
    raise ValueError("realize supports profiles 1 and 2 only")


def _select(z, selector, values):
    """values[i] when the selector is i; the range constraint keeps the selector inside the palette."""
    term = values[-1]
    for index in range(len(values) - 2, -1, -1):
        term = z.If(selector == index, values[index], term)
    return term


def _example_json(example):
    arguments, initial, flags = example
    return {"arguments": [str(v) for v in arguments], "initial_registers": {k: str(v) for k, v in sorted(initial.items())},
            "initial_flags": None if flags is None else {k: int(v) for k, v in sorted(flags.items())}}


def _example(value):
    flags = value["initial_flags"]
    return (tuple(int(v) for v in value["arguments"]),
            {k: int(v) for k, v in value["initial_registers"].items()},
            None if flags is None else {k: int(v) for k, v in flags.items()})


def query(z, problem, length, bound, examples):
    """(solver, selectors) for one length; deterministic in its inputs, so its SMT-LIB text replays."""
    context = z.Context()
    profile = _profile(z, context, problem)
    grammar = problem["grammar"]
    sizes = [len(encode_instruction(problem["target"], form)) for form in grammar]
    terminal = len(encode_program(problem["target"], []))
    solver = z.Solver(ctx=context)
    selectors = [z.Int(f"realize_slot_{j}", ctx=context) for j in range(length)]
    palette = len(grammar)
    for selector in selectors:
        solver.add(selector >= 0, selector < palette)
    total = terminal + sum((_select(z, s, [z.IntVal(n, ctx=context) for n in sizes]) for s in selectors),
                           z.IntVal(0, ctx=context))
    solver.add(total <= 256)
    if problem["objective"] == "bytes" and bound is not None:
        solver.add(total < bound)
    if profile.flags:
        defined = {name: z.BoolVal(True, ctx=context) for name in profile.flags}
        for selector in selectors:
            for index in range(palette):
                reads = profile.flag_effects(index)[0]
                if reads:
                    solver.add(z.Implies(selector == index, z.And([defined[name] for name in reads])))
            after = {}
            for name in profile.flags:
                options = []
                for index in range(palette):
                    _, written, undefined = profile.flag_effects(index)
                    options.append(z.BoolVal(False, ctx=context) if name in undefined
                                   else z.BoolVal(True, ctx=context) if name in written else defined[name])
                after[name] = _select(z, selector, options)
            defined = after
    for example in examples:
        registers, flags = profile.start(z, context, example)
        for selector in selectors:
            steps = [profile.step(z, context, index, registers, flags) for index in range(palette)]
            registers = {name: _select(z, selector, [s[0][name] for s in steps]) for name in profile.locations}
            flags = {name: _select(z, selector, [s[1][name] for s in steps]) for name in profile.flags}
        for actual, value in zip(profile.results(registers), profile.expected(example)):
            solver.add(actual == z.BitVecVal(value, 64, ctx=context))
    return solver, selectors, profile


def _lengths(problem, bound):
    """Lengths whose least possible cost is below the bound, and lengths excluded by arithmetic alone."""
    sizes = [len(encode_instruction(problem["target"], form)) for form in problem["grammar"]]
    terminal = len(encode_program(problem["target"], []))
    asked, excluded = [], []
    for length in range(problem["max_instructions"] + 1):
        least = length if problem["objective"] == "instructions" else terminal + length * min(sizes, default=0)
        if bound is None or least < bound:
            asked.append(length)
        else:
            excluded.append(length)
    return asked, excluded


def _query_hash(solver):
    return digest(solver.to_smt2().encode())


def realize(problem, bound=None, timeout_ms=1000, max_rounds=MAX_ROUNDS, deadline_ms=60_000, seeds=None):
    """One record: for every length whose cost can be below `bound` (None = no bound), unrealizable, realizable or
    inconclusive. A realizable length carries checked bytes; the first one found stops the run."""
    import z3 as z
    from .session import _agrees, _checker, _seeds, _validated, _witness
    problem = _validated(problem)
    if problem["schema"] not in (profile2.SCHEMA, "zeno-fcis/machine-problem/1"):
        raise ValueError("realize supports profiles 1 and 2 only")
    if bound is not None:
        integer(bound, 1, 1 << 16, "cost bound")
    integer(timeout_ms, 1, 60_000, "solver timeout")
    integer(max_rounds, 1, 1024, "round budget")
    integer(deadline_ms, 1, 3_600_000, "deadline")
    check = _checker(problem).check
    began = time.monotonic()
    asked, excluded = _lengths(problem, bound)
    lengths, status = [], "unrealizable"
    for length in asked:
        examples = list(_seeds(problem) if seeds is None else seeds)
        entry = {"length": length, "rounds": 0}
        while True:
            if time.monotonic() - began > deadline_ms / 1000 or entry["rounds"] >= max_rounds:
                entry.update(status="inconclusive", reason="budget-exhausted")
                break
            entry["rounds"] += 1
            solver, selectors, _ = query(z, problem, length, bound, examples)
            solver.set(timeout=timeout_ms)
            answer = solver.check()
            if answer == z.unsat:
                entry.update(status="unrealizable", query_sha256=_query_hash(solver),
                             examples=[_example_json(e) for e in examples],
                             statement=STATEMENT.format(length=length, objective=problem["objective"],
                                                        bound="infinity" if bound is None else bound))
                break
            if answer != z.sat:
                entry.update(status="inconclusive", reason="SolverUnknown:" + solver.reason_unknown())
                break
            model = solver.model()
            choice = [model.eval(s, model_completion=True).as_long() for s in selectors]
            code = encode_program(problem["target"], [problem["grammar"][i] for i in choice])
            if not all(_agrees(problem, code, e) for e in examples):
                raise ValueError("encoding disagrees with the checker's concrete model on an example state")
            verification = check(problem, code, timeout_ms=timeout_ms)
            if verification["status"] == "counterexample":
                examples.append(_witness(problem, verification))
                continue
            if verification["status"] == "refused":
                raise ValueError("encoding admitted a program the checker refuses: " + verification["reason"])
            if verification["status"] == "equivalent":
                entry.update(status="realizable", code_hex=code.hex(), forms=choice, verification=verification)
            else:
                entry.update(status="inconclusive", reason="check:" + verification.get("reason", "inconclusive"))
            break
        lengths.append(entry)
        if entry["status"] == "realizable":
            status = "realizable"
            break
        if entry["status"] == "inconclusive":
            status = "inconclusive"
    return {"schema": SCHEMA, "status": status, "problem_sha256": digest(canonical(problem)),
            "objective": problem["objective"], "bound": bound, "lengths": lengths,
            "excluded_by_cost": excluded, "z3_version": z.get_version_string(),
            "elapsed_seconds": time.monotonic() - began, "global_optimality": False, "authority": "none"}


def replay(problem, record, timeout_ms=10_000):
    """Rebuild each recorded unrealizable query from its examples; its hash must match and it must stay unsat."""
    import z3 as z
    from .session import _validated
    problem = _validated(problem)
    if record.get("schema") != SCHEMA or record.get("problem_sha256") != digest(canonical(problem)):
        raise ValueError("realize record does not bind this problem")
    replayed = []
    for entry in record["lengths"]:
        if entry["status"] != "unrealizable":
            continue
        solver, _, _ = query(z, problem, entry["length"], record["bound"], [_example(e) for e in entry["examples"]])
        same = _query_hash(solver) == entry["query_sha256"]
        solver.set(timeout=timeout_ms)
        answer = solver.check()
        replayed.append({"length": entry["length"], "query_hash_matches": same, "answer": str(answer)})
    ok = all(r["query_hash_matches"] and r["answer"] == "unsat" for r in replayed)
    return {"schema": SCHEMA, "status": "replayed" if ok else "failed", "replayed": replayed,
            "z3_version": z.get_version_string(), "authority": "none"}


def realize_session(directory, timeout_ms=1000, max_rounds=MAX_ROUNDS, deadline_ms=60_000):
    """Ask below the session's best checked candidate, after rechecking it now."""
    from .session import _best, _checker, _load, _locked, _rank
    with _locked(directory):
        problem, _, events = _load(directory)
        best = _best(directory, problem, events)
    if not best:
        return {"status": "inconclusive", "reason": "session has no checked candidate", "authority": "none"}
    _, _, receipt, code = best
    fresh = _checker(problem).check(problem, code, timeout_ms=timeout_ms)
    if fresh["status"] != "equivalent" or fresh != receipt["verification"]:
        return {"status": "inconclusive", "reason": "session incumbent did not recheck", "authority": "none"}
    bound = _rank(problem, fresh, code)[0]
    record = realize(problem, bound, timeout_ms, max_rounds, deadline_ms)
    record["incumbent"] = {"code_hex": code.hex(), "cost": fresh["cost"], "recheck": "equivalent"}
    return record
