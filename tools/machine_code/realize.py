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

from . import profile2, profile3
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
        return {name: z.BitVecVal(registers[name], 64, ctx=context) for name in self.locations}, {}, {}

    def step(self, z, context, index, registers, flags, memory):
        pairs = [(self.entry[name], registers[name]) for name in self.locations]
        return {name: z.substitute(self.transfers[index][name], *pairs) for name in self.locations}, {}, {}

    def flag_effects(self, index):
        return (), (), ()

    def pairs(self, registers, memory, example):
        arguments = example[0]
        return [(registers[self.result],
                 self.verify._ast_concrete(self.problem["expression"], {"x": arguments[0], "y": arguments[1]}))]


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
                {name: z.BoolVal(bool(entry_flags[name]), ctx=context) for name in self.flags}, {})

    def step(self, z, context, index, registers, flags, memory):
        final, final_flags = self.verify2.machine_smt(z, self.target, self.decoded[index], registers, flags, context)
        # A flag the form leaves undefined is never read by an admitted program (see `admitted`), so its
        # placeholder value cannot reach a result.
        placeholder = z.BoolVal(False, ctx=context)
        return ({name: final[name] for name in self.locations},
                {name: placeholder if final_flags.get(name) is None else final_flags[name] for name in self.flags}, {})

    def flag_effects(self, index):
        return self.effects[index]

    def pairs(self, registers, memory, example):
        values = dict(zip(self.problem["inputs"], example[0]))
        return [(registers[name], self.verify2._ast_concrete(output, values))
                for name, output in zip(self.results_names, self.problem["outputs"])]


class _Profile3:
    """Profile 3: registers, x86 flags and every region word as quantified state; region bases must stay intact."""

    bases_rule = True

    def __init__(self, z, context, problem):
        from . import verify2, verify3
        self.verify3, self.problem, self.target = verify3, problem, problem["target"]
        verify3._palette(problem)
        self.x86 = self.target == verify3.X86
        self.locations = list(verify3.REGISTERS[self.target])
        self.flags = verify3.FLAGS if self.x86 else ()
        self.inputs, self.base_of, self.scratch = verify3._layout(problem)
        self.regions = {region["base"]: region["name"] for region in problem["regions"]}
        self.writable = [r["name"] for r in problem["regions"] if r["access"] == "read-write"]
        self.results_names = verify3.RESULTS[self.target][:len(problem["outputs"])]
        self.decoded, self.effects, self.bases = [], [], sorted(self.regions)
        for form in problem["grammar"]:
            decoded = verify3.decode(self.target, encode_program(self.target, [form]))
            if len(decoded) != 2:
                raise ValueError("grammar entry did not decode to one instruction")
            ins = decoded[0]
            self.decoded.append(ins)
            self.effects.append(verify2.flag_effects(ins) if self.x86 else ((), (), ()))

    def base_effect(self, index):
        """(region base this form addresses or None, register it writes or None), as `verify3.admit` reads them."""
        ins = self.decoded[index]
        read = self.verify3._access(ins.form())[0] if ins.op in self.verify3.MEMORY else None
        written = None if ins.op in ("cmp", "test", "nop", "store") else ins.operands[0]
        return read, written

    def start(self, z, context, example):
        inputs, bases, initial, flags, memory = example
        word = lambda value: z.BitVecVal(value, 64, ctx=context)
        registers = {name: word(initial.get(name, 0)) for name in self.scratch}
        registers.update({self.inputs[n]: word(v) for n, v in zip(self.problem["inputs"], inputs)})
        registers.update({self.base_of[r["name"]]: word(v) for r, v in zip(self.problem["regions"], bases)})
        entry_flags = {name: z.BoolVal(bool((flags or {}).get(name, 0)), ctx=context) for name in self.flags}
        return registers, entry_flags, {r["name"]: [word(v) for v in memory[r["name"]]] for r in self.problem["regions"]}

    def step(self, z, context, index, registers, flags, memory):
        r, f, m = dict(registers), dict(flags), {name: list(words) for name, words in memory.items()}
        if not self.x86:
            r["zero"] = z.BitVecVal(0, 64, ctx=context)
        self.verify3._step_smt(z, self.target, self.decoded[index], r, f, m, self.regions, context)
        r.pop("zero", None)
        placeholder = z.BoolVal(False, ctx=context)
        return ({name: r[name] for name in self.locations},
                {name: placeholder if f.get(name) is None else f[name] for name in self.flags}, m)

    def flag_effects(self, index):
        return self.effects[index]

    def pairs(self, registers, memory, example):
        inputs, _, _, _, initial = example
        values = dict(zip(self.problem["inputs"], inputs))
        results, words = profile3.expected(self.problem, values, {n: list(v) for n, v in initial.items()})
        pairs = [(registers[name], value) for name, value in zip(self.results_names, results)]
        pairs += [(term, value) for name in self.writable for term, value in zip(memory[name], words[name])]
        pairs += [(term, value) for name, terms in memory.items() if name not in self.writable
                  for term, value in zip(terms, initial[name])]
        return pairs


def _profile(z, context, problem):
    if problem["schema"] == profile2.SCHEMA:
        return _Profile2(z, context, problem)
    if problem["schema"] == "zeno-fcis/machine-problem/1":
        return _Profile1(z, context, problem)
    if problem["schema"] == profile3.SCHEMA:
        return _Profile3(z, context, problem)
    raise ValueError("realize supports profiles 1, 2 and 3 only")


def _select(z, selector, values, indices=None):
    """values[i] when the selector is indices[i]; range constraints keep the selector inside its forms."""
    indices = range(len(values)) if indices is None else indices
    term = values[-1]
    for position in range(len(values) - 2, -1, -1):
        term = z.If(selector == indices[position], values[position], term)
    return term


def _example_json(example):
    if len(example) == 5:
        inputs, bases, initial, flags, regions = example
        return {"inputs": [str(v) for v in inputs], "bases": [str(v) for v in bases],
                "initial_registers": {k: str(v) for k, v in sorted(initial.items())},
                "initial_flags": None if flags is None else {k: int(v) for k, v in sorted(flags.items())},
                "initial_regions": {k: [str(v) for v in words] for k, words in sorted(regions.items())}}
    arguments, initial, flags = example
    return {"arguments": [str(v) for v in arguments], "initial_registers": {k: str(v) for k, v in sorted(initial.items())},
            "initial_flags": None if flags is None else {k: int(v) for k, v in sorted(flags.items())}}


def _example(value):
    flags = value["initial_flags"]
    flags = None if flags is None else {k: int(v) for k, v in flags.items()}
    initial = {k: int(v) for k, v in value["initial_registers"].items()}
    if "initial_regions" in value:
        return (tuple(int(v) for v in value["inputs"]), tuple(int(v) for v in value["bases"]), initial, flags,
                {k: tuple(int(v) for v in words) for k, words in value["initial_regions"].items()})
    return tuple(int(v) for v in value["arguments"]), initial, flags


def query(z, problem, length, bound, examples, ranges=None):
    """(solver, selectors, profile) for one length; deterministic in its inputs, so its SMT-LIB text replays.

    `ranges`, one (low, high) per slot, restricts each slot to forms low..high-1 of the palette: the query of one
    subspace of a partition. Without it every slot ranges over the whole palette."""
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
    if ranges is None:
        slots = [list(range(palette))] * length
    else:
        if len(ranges) != length or not all(0 <= low < high <= palette for low, high in ranges):
            raise ValueError("subspace ranges must give one nonempty palette range per slot")
        slots = [list(range(low, high)) for low, high in ranges]
        for selector, (low, high) in zip(selectors, ranges):
            solver.add(selector >= low, selector < high)
    total = terminal + sum((_select(z, s, [z.IntVal(sizes[i], ctx=context) for i in forms], forms)
                            for s, forms in zip(selectors, slots)), z.IntVal(0, ctx=context))
    solver.add(total <= 256)
    if problem["objective"] == "bytes" and bound is not None:
        solver.add(total < bound)
    if profile.flags:
        defined = {name: z.BoolVal(True, ctx=context) for name in profile.flags}
        for selector, forms in zip(selectors, slots):
            for index in forms:
                reads = profile.flag_effects(index)[0]
                if reads:
                    solver.add(z.Implies(selector == index, z.And([defined[name] for name in reads])))
            after = {}
            for name in profile.flags:
                options = []
                for index in forms:
                    _, written, undefined = profile.flag_effects(index)
                    options.append(z.BoolVal(False, ctx=context) if name in undefined
                                   else z.BoolVal(True, ctx=context) if name in written else defined[name])
                after[name] = _select(z, selector, options, forms)
            defined = after
    if getattr(profile, "bases_rule", False):
        intact = {base: z.BoolVal(True, ctx=context) for base in profile.bases}
        for selector, forms in zip(selectors, slots):
            for index in forms:
                read = profile.base_effect(index)[0]
                if read is not None:
                    solver.add(z.Implies(selector == index, intact[read]))
            intact = {base: _select(z, selector, [z.BoolVal(False, ctx=context) if profile.base_effect(i)[1] == base
                                                  else intact[base] for i in forms], forms)
                      for base in profile.bases}
    for example in examples:
        registers, flags, memory = profile.start(z, context, example)
        for selector, forms in zip(selectors, slots):
            steps = [profile.step(z, context, index, registers, flags, memory) for index in forms]
            registers = {name: _select(z, selector, [s[0][name] for s in steps], forms) for name in profile.locations}
            flags = {name: _select(z, selector, [s[1][name] for s in steps], forms) for name in profile.flags}
            memory = {name: [_select(z, selector, [s[2][name][i] for s in steps], forms) for i in range(len(words))]
                      for name, words in memory.items()}
        for actual, value in profile.pairs(registers, memory, example):
            solver.add(actual == z.BitVecVal(value, 64, ctx=context))
    return solver, selectors, profile


def _lengths(problem, bound):
    """(asked, excluded by cost, empty): the lengths a query must decide, the lengths whose least possible cost is
    not below the bound, and the positive lengths of an empty palette, which have no candidate at all. The last two
    are discharged by arithmetic, without a query."""
    sizes = [len(encode_instruction(problem["target"], form)) for form in problem["grammar"]]
    terminal = len(encode_program(problem["target"], []))
    asked, excluded, empty = [], [], []
    for length in range(problem["max_instructions"] + 1):
        least = length if problem["objective"] == "instructions" else terminal + length * min(sizes, default=0)
        if bound is not None and least >= bound:
            excluded.append(length)
        elif length > 0 and not sizes:
            empty.append(length)
        else:
            asked.append(length)
    return asked, excluded, empty


def _query_hash(solver):
    return digest(solver.to_smt2().encode())


def _groups(palette, count):
    """Contiguous palette ranges that cover forms 0..palette-1 exactly once."""
    bounds = [palette * g // count for g in range(count + 1)]
    return [(bounds[g], bounds[g + 1]) for g in range(count)]


def _cegis(problem, length, bound, examples, ranges, timeout_ms, max_rounds, deadline, probe_ms=None):
    """Decide one length, or one subspace of it, before the wall-clock `deadline` (a time.time() value).

    With `probe_ms`, a query that does not answer within it returns status `split`, carrying the examples found so
    far, so that the caller can halve the subspace instead of waiting."""
    import z3 as z
    from .session import _agrees, _checker, _witness
    check = _checker(problem).check
    examples = list(examples)
    entry = {"rounds": 0}
    while True:
        remaining = int((deadline - time.time()) * 1000)
        if remaining <= 0 or entry["rounds"] >= max_rounds:
            entry.update(status="inconclusive", reason="budget-exhausted")
            return entry
        entry["rounds"] += 1
        solver, selectors, _ = query(z, problem, length, bound, examples, ranges)
        # Building the query takes time: the solver gets only what is left of the deadline now.
        remaining = int((deadline - time.time()) * 1000)
        if remaining <= 0:
            entry.update(status="inconclusive", reason="budget-exhausted")
            return entry
        solver.set(timeout=min(timeout_ms if probe_ms is None else probe_ms, remaining))
        answer = solver.check()
        if answer == z.unsat:
            entry.update(status="unrealizable", query_sha256=_query_hash(solver),
                         examples=[_example_json(e) for e in examples])
            return entry
        if answer != z.sat:
            if probe_ms is not None and remaining > probe_ms:
                entry.update(status="split", carry=examples)
            else:
                entry.update(status="inconclusive", reason="SolverUnknown:" + solver.reason_unknown())
            return entry
        model = solver.model()
        choice = [model.eval(s, model_completion=True).as_long() for s in selectors]
        code = encode_program(problem["target"], [problem["grammar"][i] for i in choice])
        if not all(_agrees(problem, code, e) for e in examples):
            raise ValueError("encoding disagrees with the checker's concrete model on an example state")
        remaining = int((deadline - time.time()) * 1000)
        if remaining <= 0:
            entry.update(status="inconclusive", reason="budget-exhausted")
            return entry
        verification = check(problem, code, timeout_ms=max(1, min(timeout_ms, remaining)))
        if verification["status"] == "counterexample":
            examples.append(_witness(problem, verification))
            continue
        if verification["status"] == "refused":
            raise ValueError("encoding admitted a program the checker refuses: " + verification["reason"])
        if verification["status"] == "equivalent":
            entry.update(status="realizable", code_hex=code.hex(), forms=choice, verification=verification)
        else:
            entry.update(status="inconclusive", reason="check:" + verification.get("reason", "inconclusive"))
        return entry


def _decide(job):
    """Worker entry point: one subspace in this process, with its own Z3 context."""
    problem, length, bound, ranges, examples, timeout_ms, probe_ms, max_rounds, deadline = job
    entry = _cegis(problem, length, bound, examples, ranges, timeout_ms, max_rounds, deadline, probe_ms)
    entry["ranges"] = [list(r) for r in ranges]
    return entry


def _halve(ranges):
    """Two subspaces that partition `ranges`: the widest slot's range split in the middle."""
    slot = max(range(len(ranges)), key=lambda j: (ranges[j][1] - ranges[j][0], -j))
    low, high = ranges[slot]
    middle = (low + high) // 2
    return [ranges[:slot] + [(low, middle)] + ranges[slot + 1:], ranges[:slot] + [(middle, high)] + ranges[slot + 1:]]


def _partitioned(problem, length, bound, examples, initial, timeout_ms, split_ms, max_rounds, deadline, pool,
                 workers=1):
    """Decide every subspace of one length. `initial` partitions the space and is consumed lazily; with `split_ms`, a
    subspace whose query does not answer in time is halved and its two children are decided next. Unsat needs every
    final subspace unsat; any checked solution makes the length realizable; anything undecided, or left unvisited
    when the deadline passes, makes it inconclusive. At most 2 * `workers` jobs are outstanding, and none starts
    after the deadline."""
    def job(ranges, known):
        probe = split_ms if split_ms is not None and any(high - low > 1 for low, high in ranges) else None
        return (problem, length, bound, [tuple(r) for r in ranges], known, timeout_ms, probe, max_rounds, deadline)

    leaves, undecided, found, splits, rounds = [], [], [], 0, 0

    def settle(result):
        nonlocal splits, rounds
        rounds += result["rounds"]
        if result["status"] == "split":
            splits += 1
            return [job(child, result["carry"]) for child in _halve([tuple(r) for r in result["ranges"]])]
        (found if result["status"] == "realizable" else leaves if result["status"] == "unrealizable"
         else undecided).append(result)
        return []

    pending, fresh = [], iter(initial)

    def take():
        """The next job: split children first, then the next initial subspace; None when none is left."""
        if pending:
            return pending.pop(0)
        ranges = next(fresh, None)
        return None if ranges is None else job(ranges, examples)

    def expired():
        return time.time() >= deadline

    if pool is None:
        while not found and not expired():
            item = take()
            if item is None:
                break
            pending[:0] = settle(_decide(item))
    else:
        from concurrent.futures import FIRST_COMPLETED, wait
        from concurrent.futures.process import BrokenProcessPool
        running = set()
        try:
            while not found:
                while len(running) < 2 * workers and not expired():
                    item = take()
                    if item is None:
                        break
                    running.add(pool.submit(_decide, item))
                if not running:
                    break
                done, running = wait(running, return_when=FIRST_COMPLETED)
                for future in done:
                    pending[:0] = settle(future.result())
        except BrokenProcessPool:
            # A worker that died decided nothing; the length cannot be unsat without every subspace.
            undecided.append({"status": "inconclusive", "reason": "WorkerProcessFailed", "rounds": 0})
        for future in running:
            future.cancel()
    if not found and take() is not None:
        # Subspaces were left unvisited: the length cannot be unsat without them.
        undecided.append({"status": "inconclusive", "rounds": 0,
                          "reason": "budget-exhausted" if expired() else "unvisited-subspaces"})
    entry = {"length": length, "subspace_count": len(leaves) + len(undecided) + len(found), "splits": splits,
             "rounds": rounds}
    if found:
        best = min(found, key=lambda r: r["ranges"])
        entry.update(status="realizable", code_hex=best["code_hex"], forms=best["forms"],
                     verification=best["verification"], subspace=best["ranges"])
    elif not undecided:
        entry.update(status="unrealizable",
                     subspaces=[{key: r.get(key) for key in ("ranges", "status", "query_sha256", "examples", "rounds")}
                                for r in sorted(leaves, key=lambda r: r["ranges"])])
    else:
        entry.update(status="inconclusive", reason=sorted({r.get("reason", "") for r in undecided}))
    return entry


def realize(problem, bound=None, timeout_ms=1000, max_rounds=MAX_ROUNDS, deadline_ms=60_000, seeds=None,
            groups=1, workers=1, split_ms=None):
    """One record: for every length whose cost can be below `bound` (None = no bound), unrealizable, realizable or
    inconclusive. A realizable length carries checked bytes; the first one found stops the run.

    With `groups` > 1 each slot's palette is split into that many contiguous groups, and every combination of groups
    over the slots is one subspace, decided by its own query; `workers` > 1 decides them in that many processes.
    With `split_ms`, a subspace whose query does not answer within it is halved, as many times as needed (adaptive,
    hierarchical decomposition). All work shares one deadline."""
    import z3 as z
    from .session import _seeds, _validated
    problem = _validated(problem)
    if problem["schema"] not in (profile2.SCHEMA, profile3.SCHEMA, "zeno-fcis/machine-problem/1"):
        raise ValueError("realize supports profiles 1, 2 and 3 only")
    if bound is not None:
        integer(bound, 0, 1 << 16, "cost bound")
    integer(timeout_ms, 1, 60_000, "solver timeout")
    integer(max_rounds, 1, 1024, "round budget")
    integer(deadline_ms, 1, 3_600_000, "deadline")
    integer(groups, 1, 32, "groups per slot")
    integer(workers, 1, 4, "worker processes")
    if split_ms is not None:
        integer(split_ms, 1, 60_000, "split probe")
    partitioned = groups > 1 or split_ms is not None
    began, deadline = time.monotonic(), time.time() + deadline_ms / 1000
    asked, excluded, empty = _lengths(problem, bound)
    palette = len(problem["grammar"])
    palette_groups = _groups(palette, min(groups, palette)) if palette else []
    pool = None
    if partitioned and workers > 1:
        import multiprocessing
        from concurrent.futures import ProcessPoolExecutor
        pool = ProcessPoolExecutor(max_workers=workers, mp_context=multiprocessing.get_context("spawn"))
    lengths, status = [], "unrealizable"
    try:
        for length in asked:
            examples = list(_seeds(problem) if seeds is None else seeds)
            if partitioned:
                from itertools import product
                # Generated as consumed: the number of subspaces grows exponentially with the length.
                initial = ([palette_groups[g] for g in combo]
                           for combo in product(range(len(palette_groups)), repeat=length))
                entry = _partitioned(problem, length, bound, examples, initial, timeout_ms, split_ms, max_rounds,
                                     deadline, pool, workers)
            else:
                entry = {"length": length, **_cegis(problem, length, bound, examples, None, timeout_ms, max_rounds,
                                                    deadline)}
            if entry["status"] == "unrealizable":
                entry["statement"] = STATEMENT.format(length=length, objective=problem["objective"],
                                                      bound="infinity" if bound is None else bound)
            lengths.append(entry)
            if entry["status"] == "realizable":
                status = "realizable"
                break
            if entry["status"] == "inconclusive":
                status = "inconclusive"
    finally:
        if pool is not None:
            pool.shutdown(wait=True, cancel_futures=True)
    record = {"schema": SCHEMA, "status": status, "problem_sha256": digest(canonical(problem)),
              "objective": problem["objective"], "bound": bound, "lengths": lengths,
              "excluded_by_cost": excluded, "excluded_by_empty_palette": empty, "z3_version": z.get_version_string(),
              "elapsed_seconds": time.monotonic() - began, "global_optimality": False, "authority": "none"}
    if partitioned:
        record["partition"] = {"groups_per_slot": len(palette_groups), "palette_groups": [list(g) for g in palette_groups],
                               "workers": workers, "split_ms": split_ms}
    return record


def _covers(entry, palette):
    """Independently of how they were made: the recorded subspaces lie in the palette, are pairwise disjoint and
    their sizes add up to the whole space, so they cover every candidate exactly once."""
    from math import prod
    boxes = [[tuple(r) for r in s["ranges"]] for s in entry["subspaces"]]
    if any(len(b) != entry["length"] or not all(0 <= low < high <= palette for low, high in b) for b in boxes):
        return False
    if sum(prod(high - low for low, high in b) for b in boxes) != palette ** entry["length"]:
        return False
    return not any(all(a[0] < b[1] and b[0] < a[1] for a, b in zip(boxes[i], boxes[j]))
                   for i in range(len(boxes)) for j in range(i + 1, len(boxes)))


def _derived_status(statuses):
    if statuses and statuses[-1] == "realizable":
        return "realizable"
    return "inconclusive" if "inconclusive" in statuses else "unrealizable"


def replay(problem, record, timeout_ms=10_000):
    """Re-establish a record's whole claim, not only its queries.

    The eligible and excluded lengths are recomputed from the problem, objective and bound. Every eligible length
    must appear once, in order, with the record's status following from its entries; a realizable record may end at
    its realizable length. Each unrealizable entry must carry the exact statement and either one query or subspaces
    that exactly cover the palette space; each query is rebuilt from its examples, must have the recorded hash and
    must stay unsat. A realizable entry's bytes must recheck equivalent, at that length and below the bound."""
    import z3 as z
    from .session import _checker, _validated
    problem = _validated(problem)
    if record.get("schema") != SCHEMA or record.get("problem_sha256") != digest(canonical(problem)):
        raise ValueError("realize record does not bind this problem")
    bound = record.get("bound")
    if bound is not None:
        integer(bound, 0, 1 << 16, "record cost bound")
    failures, replayed = [], []
    if record.get("objective") != problem["objective"]:
        failures.append("objective differs from the problem's")
    asked, excluded, empty = _lengths(problem, bound)
    if record.get("excluded_by_cost") != excluded:
        failures.append("cost-excluded lengths differ from the bound's")
    if record.get("excluded_by_empty_palette", []) != empty:
        failures.append("empty-palette lengths differ from the problem's")
    entries = record.get("lengths")
    if not isinstance(entries, list) or not all(isinstance(e, dict) for e in entries):
        raise ValueError("record lengths must be a list of entries")
    labels = [e.get("length") for e in entries]
    statuses = [e.get("status") for e in entries]
    if labels != asked[:len(labels)]:
        failures.append("lengths are missing, repeated, relabelled or out of order")
    if any(status not in ("unrealizable", "realizable", "inconclusive") for status in statuses):
        failures.append("unknown entry status")
    if "realizable" in statuses[:-1]:
        failures.append("an entry follows a realizable length")
    if _derived_status(statuses) != "realizable" and len(entries) != len(asked):
        failures.append("not every eligible length is present")
    if record.get("status") != _derived_status(statuses):
        failures.append("record status does not follow from its entries")
    partitioned = "partition" in record
    for entry in entries:
        length = entry.get("length")
        if type(length) is not int or length not in asked:
            continue
        if entry.get("status") == "realizable":
            code = bytes.fromhex(entry.get("code_hex", ""))
            verification = _checker(problem).check(problem, code, timeout_ms=timeout_ms)
            cost = verification.get("cost", {})
            primary = cost.get("bytes") if problem["objective"] == "bytes" else cost.get("instructions")
            if (verification["status"] != "equivalent" or cost.get("instructions") != length
                    or (bound is not None and not primary < bound)):
                failures.append(f"length {length}: recorded bytes do not recheck below the bound")
            continue
        if entry.get("status") != "unrealizable":
            continue
        if entry.get("statement") != STATEMENT.format(length=length, objective=problem["objective"],
                                                      bound="infinity" if bound is None else bound):
            failures.append(f"length {length}: statement does not match the bound")
        if partitioned != ("subspaces" in entry) or ("subspaces" in entry) == ("query_sha256" in entry):
            failures.append(f"length {length}: entry shape does not match the record's mode")
            continue
        try:
            if "subspaces" not in entry:
                parts = [(None, entry["examples"], entry["query_sha256"])]
                covered = True
            else:
                parts = [(s["ranges"], s["examples"], s["query_sha256"]) for s in entry["subspaces"]]
                covered = _covers(entry, len(problem["grammar"]))
        except (KeyError, TypeError, ValueError):
            failures.append(f"length {length}: entry is malformed")
            continue
        # Checked here, not only on replayed queries: an entry with no subspaces replays no query at all.
        if not covered:
            failures.append(f"length {length}: subspaces do not cover the palette space exactly once")
        for ranges, examples, recorded in parts:
            solver, _, _ = query(z, problem, length, bound, [_example(e) for e in examples],
                                 None if ranges is None else [tuple(r) for r in ranges])
            same = _query_hash(solver) == recorded
            solver.set(timeout=timeout_ms)
            answer = solver.check()
            replayed.append({"length": length, "ranges": ranges, "coverage": covered,
                             "query_hash_matches": same, "answer": str(answer)})
    ok = not failures and all(r["coverage"] and r["query_hash_matches"] and r["answer"] == "unsat" for r in replayed)
    return {"schema": SCHEMA, "status": "replayed" if ok else "failed", "claim": record.get("status"),
            "failures": failures, "replayed": replayed, "z3_version": z.get_version_string(), "authority": "none"}


def realize_session(directory, timeout_ms=1000, max_rounds=MAX_ROUNDS, deadline_ms=60_000, groups=1, workers=1,
                    split_ms=None):
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
    record = realize(problem, bound, timeout_ms, max_rounds, deadline_ms, groups=groups, workers=workers,
                     split_ms=split_ms)
    record["incumbent"] = {"code_hex": code.hex(), "cost": fresh["cost"], "recheck": "equivalent"}
    return record
