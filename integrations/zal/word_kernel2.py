"""ZAL word-kernel/2 syntax: named inputs, one or two results, comparisons and selects.

This module parses and renders profile-2 declarations only. `word_kernel.py` dispatches here on the profile line
and keeps every source binding, session and artifact operation. Meaning comes from the machine profile-2 tools:
`profile2.validate_problem` and `profile2.evaluate`, `verify2._palette` and `session._grammar`. No expression or
instruction semantics are implemented here.
"""
from __future__ import annotations

from dataclasses import dataclass
import json
import re

from tools.machine_code import model as mm, profile2 as mp, session as ms, verify2 as mv2
import json_numbers

PROFILE = "zal/word-kernel/2"
PROFILE_LINE = {"symbolic": "profile zal/word-kernel/2;", "english": "ZAL word kernel profile 2."}
NAME = re.compile(r"[a-z][a-z0-9_]{0,15}")
SYMBOLIC = {"not": 1, "shl": 1, "lshr": 1, "ashr": 1, "ite": 3, "add": 2, "sub": 2, "mul": 2, "and": 2, "or": 2,
            "xor": 2, "ult": 2, "ule": 2, "slt": 2, "sle": 2, "eq": 2}
ENGLISH_BINARY = {"plus": "add", "minus": "sub", "times": "mul"}
SHIFT_WORDS = {"shl": "shifted left by", "lshr": "shifted right logically by", "ashr": "shifted right arithmetically by"}
COMPARE_WORDS = {"ult": ("is below", "unsigned"), "ule": ("is at most", "unsigned"),
                 "slt": ("is below", "signed"), "sle": ("is at most", "signed")}
RESERVED = set(SYMBOLIC) | {"complement", "plus", "minus", "times", "bitwise", "shifted", "left", "right", "logically",
                            "arithmetically", "by", "is", "below", "at", "most", "unsigned", "signed", "equals",
                            "if", "then", "else"}
OBSERVATION = ("Returned u64 result words under the declared ISA/register/flag model only; no cost, timing, complete "
               "ABI, whole-machine or temporal equivalence")
ASSURANCE = "Source-bound ZAL lowering to the profile-2 modular-word checker; no deployment authority"


def need(value, reason):
    if not value:
        raise ValueError(reason)


def arrays(value):
    return [arrays(x) for x in value] if isinstance(value, tuple) else value


def freeze(value):
    return tuple(freeze(x) for x in value) if isinstance(value, list) else value


@dataclass(frozen=True)
class Kernel2:
    name: str
    target: str
    inputs: tuple
    outputs: tuple
    palette: tuple
    max_body: int
    objective: str
    limits: tuple


class Expression:
    """One fully parenthesized result expression over the declared inputs."""

    def __init__(self, text, syntax, offset, inputs, path):
        self.tokens = []
        pattern = re.compile(r"[()]|[a-z][a-z0-9_]*|[0-9]+")
        at = 0
        while at < len(text):
            if text[at].isspace():
                at += 1
                continue
            match = pattern.match(text, at)
            need(match is not None, "unsupported expression token")
            self.tokens.append((match[0], offset + match.start(), offset + match.end()))
            need(len(self.tokens) <= 512, "expression token limit")
            at = match.end()
        self.at, self.nodes, self.syntax, self.inputs, self.spans = 0, 0, syntax, inputs, []
        self.root = path

    def peek(self):
        return self.tokens[self.at][0] if self.at < len(self.tokens) else None

    def take(self, wanted=None):
        need(self.at < len(self.tokens), "incomplete expression")
        token = self.tokens[self.at]
        self.at += 1
        need(wanted is None or token[0] == wanted, "expected expression token " + str(wanted))
        return token

    def count(self):
        token = self.take()[0]
        need(re.fullmatch(r"0|[1-9][0-9]*", token),
             "shift count must be a literal 0 to 63; an expression, such as a comparison result, cannot be a count")
        return mm.integer(int(token), 0, 63, "shift")

    def node(self, path=None, depth=0):
        path = path or self.root
        self.nodes += 1
        need(self.nodes <= 64 and depth <= 16, "expression node/depth limit")
        token, start, end = self.take()
        if token.isdecimal():
            mm.word(token)
            value = ("const", token)
        elif token != "(":
            need(token in self.inputs, "unknown input " + token)
            value = ("input", token)
        elif self.syntax == "symbolic":
            op = self.take()[0]
            need(op in SYMBOLIC, "unsupported word operation " + op)
            children = [self.node(f"{path}/1", depth + 1)]
            if op in SHIFT_WORDS:
                children.append(self.count())
            else:
                children += [self.node(f"{path}/{k}", depth + 1) for k in range(2, SYMBOLIC[op] + 1)]
            value = (op, *children)
        elif self.peek() == "complement":
            self.take()
            value = ("not", self.node(f"{path}/1", depth + 1))
        elif self.peek() == "if":
            self.take()
            condition = self.node(f"{path}/1", depth + 1)
            self.take("then")
            then = self.node(f"{path}/2", depth + 1)
            self.take("else")
            value = ("ite", condition, then, self.node(f"{path}/3", depth + 1))
        else:
            left = self.node(f"{path}/1", depth + 1)
            word = self.take()[0]
            if word == "shifted":
                direction = self.take()[0]
                need(direction in ("left", "right"), "shift direction")
                kind = "left" if direction == "left" else self.take()[0]
                need(kind in ("left", "logically", "arithmetically"), "right shifts are logical or arithmetic")
                self.take("by")
                value = ({"left": "shl", "logically": "lshr", "arithmetically": "ashr"}[kind], left, self.count())
            elif word == "is":
                relation = self.take()[0]
                if relation == "at":
                    self.take("most")
                else:
                    need(relation == "below", "comparison is 'is below' or 'is at most'")
                right = self.node(f"{path}/2", depth + 1)
                self.take("(")
                order = self.take()[0]
                need(order in ("unsigned", "signed"), "comparison states (unsigned) or (signed)")
                self.take(")")
                op = {("below", "unsigned"): "ult", ("at", "unsigned"): "ule",
                      ("below", "signed"): "slt", ("at", "signed"): "sle"}[(relation, order)]
                value = (op, left, right)
            elif word == "equals":
                value = ("eq", left, self.node(f"{path}/2", depth + 1))
            elif word == "bitwise":
                op = self.take()[0]
                need(op in ("and", "or", "xor"), "bitwise operation")
                value = (op, left, self.node(f"{path}/2", depth + 1))
            else:
                need(word in ENGLISH_BINARY, "unsupported English operation " + word)
                value = (ENGLISH_BINARY[word], left, self.node(f"{path}/2", depth + 1))
        if token == "(":
            end = self.take(")")[2]
        self.spans.append((path, start, end))
        return value


def _inputs(text, syntax):
    if syntax == "symbolic":
        need(re.fullmatch(r"[^,;:]+:u64(?:,\s*[^,;:]+:u64)*", text), "inputs are declared as NAME:u64, comma separated")
        names = [part.strip()[:-4] for part in text.split(",")]
    else:
        names = re.split(r", | and ", text)
    need(1 <= len(names) <= mp.MAX_INPUTS, f"profile {PROFILE} declares 1 to 6 inputs; found {len(names)}")
    for name in names:
        need(NAME.fullmatch(name), f"input name {name!r} must match [a-z][a-z0-9_]{{0,15}}")
        need(name not in RESERVED, f"input name {name!r} is a reserved expression word")
    duplicate = next((name for k, name in enumerate(names) if name in names[:k]), None)
    need(duplicate is None, f"duplicate input name {duplicate}")
    return tuple(names)


def _declared(label, check):
    try:
        return check()
    except ValueError as error:
        raise ValueError(f"{label}: {error}") from None


def parse(lines, syntax, limit_names, limit_bounds):
    """Return (Kernel2, declaration and expression spans) from stripped (text, offset) lines."""
    symbolic = syntax == "symbolic"
    need(lines and lines[0][0] == PROFILE_LINE[syntax], "profile: not a profile-2 source")
    keyword = "result" if symbolic else "Return"
    results = 0
    while 4 + results < len(lines) and lines[4 + results][0].split(" ", 1)[0] == keyword:
        results += 1
    need(1 <= results <= mp.MAX_OUTPUTS, f"result: profile {PROFILE} declares one or two results; found {results}")
    need(len(lines) == 8 + results, f"profile {PROFILE} requires the declarations profile, kernel, target, inputs, "
         "one or two results, palette, max-body, objective and limits, in that order; unsupported requirements refuse")
    head = ([r"kernel ([A-Za-z][A-Za-z0-9_]{0,63});", r"target ([a-z0-9_/-]+);", r"inputs (.+);"] if symbolic else
            [r"Kernel ([A-Za-z][A-Za-z0-9_]{0,63})\.", r"Target ([a-z0-9_/-]+)\.",
             r"(?:Input (\S+) is an unsigned 64-bit word|Inputs (.+) are unsigned 64-bit words)\."])
    tail = ([r"palette (\[.*\]);", r"max-body (0|[1-9][0-9]*);", r"objective (bytes|instructions);",
             r"limits candidates=([1-9][0-9]*) checks=([1-9][0-9]*) deadline-ms=([1-9][0-9]*) solver-ms=([1-9][0-9]*);"]
            if symbolic else
            [r"Instruction palette: (\[.*\])\.", r"Use at most (0|[1-9][0-9]*) body instructions\.",
             r"Minimize (bytes|instructions)\.",
             r"Search limits: candidates ([1-9][0-9]*), checks ([1-9][0-9]*), deadline ([1-9][0-9]*) ms, solver ([1-9][0-9]*) ms\."])
    labels = ["kernel", "target", "inputs"] + [f"result/{k + 1}" for k in range(results)] + [
        "palette", "max-body", "objective", "limits"]
    if symbolic:
        rows = [r"result (.+) modulo 2\^64;"] if results == 1 else [
            rf"result {k + 1} (.+) modulo 2\^64;" for k in range(results)]
    else:
        rows = [r"Return (.+) with wraparound modulo 2\^64\."] if results == 1 else [
            rf"Return {word} (.+) with wraparound modulo 2\^64\." for word in ("first", "second")[:results]]
    matches = []
    for label, pattern, (text, _) in zip(labels, head + rows + tail, lines[1:]):
        match = re.fullmatch(pattern, text)
        need(match is not None, f"{label}: declaration syntax is unsupported for profile {PROFILE}")
        matches.append(match)
    target = matches[1][1]
    need(target in mp.TARGETS, f"target: profile {PROFILE} requires {' or '.join(mp.TARGETS)}; found {target}")
    declared = matches[2][1] if symbolic or matches[2][1] is not None else matches[2][2]
    inputs = _declared("inputs", lambda: _inputs(declared, syntax))
    need(symbolic or (matches[2][1] is not None) == (len(inputs) == 1),
         "inputs: write 'Input NAME is an unsigned 64-bit word.' for one input and 'Inputs ... are' for more")
    outputs, spans = [], []
    for k in range(results):
        match, (_, at) = matches[3 + k], lines[4 + k]
        def expression():
            parser = Expression(match[1], syntax, at + match.start(1), inputs, f"result/{k + 1}/expression")
            value = parser.node()
            need(parser.at == len(parser.tokens), "trailing expression")
            return value, parser.spans
        value, nodes = _declared(f"result/{k + 1}", expression)
        outputs.append(value)
        spans += nodes
    def bad(_):
        raise ValueError("palette floats/nonfinite numbers refused")
    palette = _declared("palette", lambda: json.loads(matches[3 + results][1], parse_float=bad, parse_constant=bad,
                                                      parse_int=json_numbers.integer_hook(json_numbers.raising(ValueError))))
    limits = _declared("limits", lambda: tuple(mm.integer(int(v), 1, bound, name) for v, bound, name in
                                               zip(matches[-1].groups(), limit_bounds, limit_names)))
    kernel = Kernel2(matches[0][1], target, inputs, tuple(outputs), freeze(palette),
                     int(matches[4 + results][1]), matches[5 + results][1], limits)
    problem(kernel)
    names = ["profile"] + labels
    return kernel, tuple([(name, at, at + len(text)) for name, (text, at) in zip(names, lines)] + spans)


def problem(kernel):
    """Validate the lowered problem with the machine profile-2 validators; never reimplement them here."""
    result = _declared("lowering", lambda: mp.validate_problem({
        "schema": mp.SCHEMA, "target": kernel.target, "inputs": list(kernel.inputs), "outputs": arrays(kernel.outputs),
        "grammar": arrays(kernel.palette), "max_instructions": kernel.max_body, "objective": kernel.objective}))
    _declared("palette", lambda: (mv2._palette(result["target"], result["grammar"]), ms._grammar(result)))
    return result


def render(kernel, syntax):
    def expression(node):
        op = node[0]
        if op in ("input", "const"):
            return node[1]
        if syntax == "symbolic":
            tail = [str(node[2])] if op in SHIFT_WORDS else [expression(child) for child in node[2:]]
            return "(" + " ".join([op, expression(node[1])] + tail) + ")"
        if op == "not":
            return "(complement " + expression(node[1]) + ")"
        if op == "ite":
            return f"(if {expression(node[1])} then {expression(node[2])} else {expression(node[3])})"
        if op in SHIFT_WORDS:
            return f"({expression(node[1])} {SHIFT_WORDS[op]} {node[2]})"
        if op in COMPARE_WORDS:
            words, order = COMPARE_WORDS[op]
            return f"({expression(node[1])} {words} {expression(node[2])} ({order}))"
        words = {"add": "plus", "sub": "minus", "mul": "times", "and": "bitwise and", "or": "bitwise or",
                 "xor": "bitwise xor", "eq": "equals"}
        return f"({expression(node[1])} {words[op]} {expression(node[2])})"
    palette = json.dumps(arrays(kernel.palette), separators=(",", ":"))
    a, b, c, d = kernel.limits
    single = len(kernel.outputs) == 1
    if syntax == "symbolic":
        results = [f"result {'' if single else str(k + 1) + ' '}{expression(e)} modulo 2^64;"
                   for k, e in enumerate(kernel.outputs)]
        rows = [PROFILE_LINE[syntax], f"kernel {kernel.name};", f"target {kernel.target};",
                "inputs " + ",".join(n + ":u64" for n in kernel.inputs) + ";", *results,
                f"palette {palette};", f"max-body {kernel.max_body};", f"objective {kernel.objective};",
                f"limits candidates={a} checks={b} deadline-ms={c} solver-ms={d};"]
    else:
        names = kernel.inputs
        listed = names[0] if len(names) == 1 else ", ".join(names[:-1]) + " and " + names[-1]
        inputs = (f"Input {listed} is an unsigned 64-bit word." if len(names) == 1
                  else f"Inputs {listed} are unsigned 64-bit words.")
        results = [f"Return {'' if single else ('first ', 'second ')[k]}{expression(e)} with wraparound modulo 2^64."
                   for k, e in enumerate(kernel.outputs)]
        rows = [PROFILE_LINE[syntax], f"Kernel {kernel.name}.", f"Target {kernel.target}.", inputs, *results,
                f"Instruction palette: {palette}.", f"Use at most {kernel.max_body} body instructions.",
                f"Minimize {kernel.objective}.",
                f"Search limits: candidates {a}, checks {b}, deadline {c} ms, solver {d} ms."]
    return "\n".join(rows) + "\n"


def explain(kernel, lowered, topic):
    target = kernel.target
    arguments = dict(zip(kernel.inputs, mv2.ARGUMENTS[target]))
    results = list(mv2.RESULTS[target][:len(kernel.outputs)])
    scratch = [r for r in mv2.REGISTERS[target] if r not in arguments.values()]
    x86 = target == mv2.X86
    if topic == "overflow":
        return ("All result arithmetic wraps modulo 2^64. MAX+1 is 0; no overflow error result exists. Comparisons "
                "(ult, ule, slt, sle, eq) give 1 when true and 0 otherwise; a carry or borrow is stated as such a "
                "comparison. Checked overflow and U256/I256 are unsupported.")
    if topic == "clobbers":
        return {"input_registers": arguments, "result_registers": results,
                "admitted_writable_registers": list(mv2.REGISTERS[target]),
                "entry_scratch_universally_quantified": scratch,
                "entry_flags_universally_quantified": list(mv2.FLAGS) if x86 else [],
                "note": "No preservation promise for admitted writable registers or flags. CF, ZF, SF and OF are "
                        "modelled; PF, AF and the other flags are not. This is the registered profile-2 model, not "
                        "a complete ABI proof." if x86 else
                        "No preservation promise for admitted writable registers. This is the registered profile-2 "
                        "model, not a complete ABI proof."}
    if topic == "memory":
        return ("Body data-memory operands are refused. Final x86 RET reads the return address from the stack and "
                "changes control/stack state under caller ABI assumptions; these effects, unmodelled flags, "
                "instruction fetch, CET, faults and timing are not modeled. Never infer never-touches-memory."
                if x86 else
                "Body loads/stores are refused. Final JALR return assumes a valid ra/link and execution environment; "
                "PC/return, instruction fetch, traps and timing are not established by this word-result model.")
    if topic == "limits":
        return {"declared_loop_limits": lowered["binding"]["loop_limits"], "max_body_instructions": kernel.max_body,
                "profile_caps": {"inputs": mp.MAX_INPUTS, "results": mp.MAX_OUTPUTS, "expression_nodes_per_result": 64,
                                 "expression_depth": 16, "palette_entries": 32, "code_bytes": 256},
                "note": "Limits apply to each search invocation; candidate/emit/replay use the declared solver "
                        "timeout. Search exhaustion is inconclusive, not unrealizability, global optimality or an "
                        "owner decision."}
    if topic == "checking":
        return ("A proposer supplies bytes only. The independent profile-2 decoder and SMT checker compare every "
                "result word for every input assignment, every entry scratch register"
                + (" and every entry CF, ZF, SF and OF" if x86 else "")
                + " under the registered model. A read of a flag left undefined refuses. Emit rechecks and replay "
                "checks the exact source-derived problem again; no deployment grant follows.")
    if topic == "observation":
        return {"contract": OBSERVATION, "note": "Equal result words do not establish equivalence of a complete "
                "machine or shell. Entry scratch, flags, admitted clobbers and implicit return effects keep the "
                "declared assumptions. Costs and search/check charges remain separate."}
    raise ValueError("unsupported question topic")


def example(kernel, values):
    """Expected results for one complete named input assignment, by the profile-2 integer reference."""
    need(type(values) is dict and set(values) == set(kernel.inputs),
         "what-if: give every declared input exactly once: " + ", ".join(kernel.inputs))
    words = {name: mm.word(values[name]) for name in kernel.inputs}
    return words, [mp.evaluate(arrays(output), words) for output in kernel.outputs]


def question_values(kernel, text):
    """Named values from 'What if a is 1, b is 2 and c is 3?' in declared order, or None."""
    parts = [f"{re.escape(name)} is ([0-9]+)" for name in kernel.inputs]
    body = parts[0] if len(parts) == 1 else ", ".join(parts[:-1]) + " and " + parts[-1]
    match = re.fullmatch("What if " + body + r"\?", text)
    return None if match is None else dict(zip(kernel.inputs, match.groups()))


def counterexample(kernel, problem, verification):
    """Check a reported witness against the exact source meaning; return the human question."""
    inputs, scratch = verification.get("inputs"), verification.get("initial_registers")
    target = kernel.target
    need(type(inputs) is dict and set(inputs) == set(kernel.inputs), "counterexample inputs")
    arguments = mv2.ARGUMENTS[target][:len(kernel.inputs)]
    need(type(scratch) is dict and set(scratch) == {r for r in mv2.REGISTERS[target] if r not in arguments},
         "counterexample scratch must be complete")
    for value in scratch.values():
        mm.word(value)
    flags = verification.get("initial_flags")
    if target == mv2.X86:
        need(type(flags) is dict and set(flags) == set(mv2.FLAGS)
             and all(type(v) is int and v in (0, 1) for v in flags.values()), "counterexample flags must be complete")
    else:
        need(flags is None, "RV64I counterexamples have no flags")
    words = {name: mm.word(inputs[name]) for name in kernel.inputs}
    expected, actual = verification.get("expected"), verification.get("actual")
    need(type(expected) is list and type(actual) is list and len(expected) == len(actual) == len(kernel.outputs),
         "counterexample result lists")
    expected, actual = [mm.word(v) for v in expected], [mm.word(v) for v in actual]
    need(expected == [mp.evaluate(output, words) for output in problem["outputs"]] and actual != expected,
         "counterexample contradicts exact source meaning")
    shown = ", ".join(f"{name}={inputs[name]}" for name in kernel.inputs)
    return {"text": f"At {shown} and the shown entry scratch{' and flags' if flags else ''}, should the required "
                    f"results remain {[str(v) for v in expected]}?",
            "inputs": dict(inputs), "entry_scratch": dict(scratch), **({"entry_flags": dict(flags)} if flags else {}),
            "expected": [str(v) for v in expected], "reported_actual": [str(v) for v in actual],
            "note": "Reported checker witness, not an owner answer or a specification edit. A repair must recheck "
                    "the same source-derived problem."}
