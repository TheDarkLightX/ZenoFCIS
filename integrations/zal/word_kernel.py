"""Explicit ZAL word-kernel/1 authoring, source binding and checked machine sessions.

This profile does not change ZAL's other abstraction profiles. The existing
machine-byte decoder/checker decides candidates; no user or LLM grants authority.
"""
from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
from itertools import islice
import json
from pathlib import Path
import re
import stat
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from tools.machine_code import model as mm, session as ms, targets as mt, verify as mv

PROFILE = "zal/word-kernel/1"
LIMIT_NAMES = ("max_candidates", "max_checks", "deadline_ms", "timeout_ms")
LIMIT_BOUNDS = (100000, 128, 60000, 5000)
BIN = {"plus": "add", "minus": "sub", "times": "mul"}
ASSURANCE = "Source-bound ZAL lowering to the existing modular-word checker; no deployment authority"
OBSERVATION = "Returned u64 word under the declared ISA/register model only; no cost, timing, complete ABI, whole-machine or temporal equivalence"


def need(value, reason):
    if not value:
        raise ValueError(reason)


def canon(value):
    return mm.canonical(value)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def ordinary(path, directory=False):
    path = Path(path)
    need(".." not in path.parts, "parent path components refused")
    path = path.absolute()
    for parent in reversed(path.parents):
        need(stat.S_ISDIR(parent.lstat().st_mode), "symlink/nonordinary ancestor")
    mode = path.lstat().st_mode
    need(stat.S_ISDIR(mode) if directory else stat.S_ISREG(mode), "nonordinary path")
    return path


def read_bytes(path, cap=65536):
    path = ordinary(path)
    need(path.stat().st_size <= cap, "input exceeds byte limit")
    with path.open("rb") as stream:
        data = stream.read(cap + 1)
    need(len(data) <= cap, "input grew beyond byte limit")
    return data


def read_json(path):
    def pairs(rows):
        result = {}
        for key, value in rows:
            need(key not in result, "duplicate JSON key")
            result[key] = value
        return result
    def bad(_):
        raise ValueError("nonfinite/noninteger JSON refused")
    return json.loads(read_bytes(path).decode("utf-8"), object_pairs_hook=pairs,
                      parse_float=bad, parse_constant=bad)


def source_identity():
    base = ROOT / "tools/machine_code"
    for module in (mm, ms, mt, mv):
        need(Path(module.__file__).resolve().parent == base, "machine module loaded from another source tree")
    paths = sorted(base.glob("*.py"))
    need(0 < len(paths) <= 16, "machine source inventory")
    return {p.name: digest(read_bytes(p, 131072)) for p in paths}


@dataclass(frozen=True)
class Kernel:
    name: str
    target: str
    expression: tuple
    palette: tuple
    max_body: int
    objective: str
    limits: tuple


@dataclass(frozen=True)
class Document:
    kernel: Kernel
    source: str
    syntax: str
    spans: tuple


def arrays(value):
    return [arrays(x) for x in value] if isinstance(value, tuple) else value


def freeze(value):
    return tuple(freeze(x) for x in value) if isinstance(value, list) else value


class Expression:
    def __init__(self, text, syntax, offset):
        self.tokens = []
        pattern = re.compile(r"[()]|[a-z_]+|[0-9]+")
        at = 0
        while at < len(text):
            if text[at].isspace():
                at += 1
                continue
            match = pattern.match(text, at)
            need(match is not None, "unsupported expression token")
            end = match.end()
            self.tokens.append((match[0], offset + at, offset + end))
            need(len(self.tokens) <= 512, "expression token limit")
            at = end
        self.at, self.nodes, self.syntax, self.spans = 0, 0, syntax, []

    def take(self, wanted=None):
        need(self.at < len(self.tokens), "incomplete expression")
        token = self.tokens[self.at]
        self.at += 1
        need(wanted is None or token[0] == wanted, "expected expression token " + str(wanted))
        return token

    def node(self, path="expression", depth=0):
        self.nodes += 1
        need(self.nodes <= 64 and depth <= 16, "expression node/depth limit")
        token, start, end = self.take()
        if token in ("x", "y"):
            value = ("input", token)
        elif token.isdecimal():
            mm.word(token)
            value = ("const", token)
        else:
            need(token == "(", "unsupported expression/input")
            if self.syntax == "symbolic":
                op = self.take()[0]
                need(op in ("not", "add", "sub", "mul", "and", "or", "xor", "shl", "lshr"), "unsupported word operation")
                left = self.node(path + "/1", depth + 1)
                if op == "not":
                    value = (op, left)
                elif op in ("shl", "lshr"):
                    count = self.take()[0]
                    need(re.fullmatch(r"0|[1-9][0-9]*", count), "canonical shift count required")
                    value = (op, left, mm.integer(int(count), 0, 63, "shift"))
                else:
                    value = (op, left, self.node(path + "/2", depth + 1))
            elif self.at < len(self.tokens) and self.tokens[self.at][0] == "complement":
                self.take("complement")
                value = ("not", self.node(path + "/1", depth + 1))
            else:
                left = self.node(path + "/1", depth + 1)
                word = self.take()[0]
                if word == "shifted":
                    direction = self.take()[0]
                    need(direction in ("left", "right"), "shift direction")
                    if direction == "right":
                        self.take("logically")
                    self.take("by")
                    count = self.take()[0]
                    need(re.fullmatch(r"0|[1-9][0-9]*", count), "canonical shift count required")
                    value = ("shl" if direction == "left" else "lshr", left,
                             mm.integer(int(count), 0, 63, "shift"))
                else:
                    if word == "bitwise":
                        word = self.take()[0]
                        need(word in ("and", "or", "xor"), "bitwise operation")
                        op = word
                    else:
                        need(word in BIN, "unsupported English operation")
                        op = BIN[word]
                    value = (op, left, self.node(path + "/2", depth + 1))
            _, _, end = self.take(")")
        self.spans.append((path, start, end))
        return value


def problem(kernel):
    result = mm.validate_problem({"schema": "zeno-fcis/machine-problem/1", "target": kernel.target,
        "inputs": ["x", "y"], "expression": arrays(kernel.expression), "grammar": arrays(kernel.palette),
        "max_instructions": kernel.max_body, "objective": kernel.objective})
    # Reuse both existing palette boundaries, never implement instruction semantics here.
    mv._palette(result["target"], result["grammar"])
    ms._grammar(result)
    return result


def parse(source, syntax="symbolic"):
    need(type(source) is str and source.isascii() and len(source.encode()) <= 65536, "bounded ASCII source required")
    need(all(c in "\t\r\n" or " " <= c <= "~" for c in source) and "\r" not in source.replace("\r\n", ""), "source uses printable ASCII, tabs and LF/CRLF lines")
    need(syntax in ("symbolic", "english"), "unsupported syntax")
    lines, offset = [], 0
    for raw in source.splitlines(keepends=True):
        if raw.strip():
            lines.append((raw.strip(), offset + len(raw) - len(raw.lstrip())))
        offset += len(raw)
    need(len(lines) == 9, "word profile requires exactly nine declarations; unsupported requirements refuse")
    symbolic = syntax == "symbolic"
    patterns = ([r"profile zal/word-kernel/1;", r"kernel ([A-Za-z][A-Za-z0-9_]{0,63});",
        r"target ([a-z0-9_/-]+);", r"inputs x:u64,\s*y:u64;", r"result (.+) modulo 2\^64;",
        r"palette (\[.*\]);", r"max-body (0|[1-9][0-9]*);", r"objective (bytes|instructions);",
        r"limits candidates=([1-9][0-9]*) checks=([1-9][0-9]*) deadline-ms=([1-9][0-9]*) solver-ms=([1-9][0-9]*);"]
        if symbolic else [r"ZAL word kernel profile 1\.", r"Kernel ([A-Za-z][A-Za-z0-9_]{0,63})\.",
        r"Target ([a-z0-9_/-]+)\.", r"Inputs x and y are unsigned 64-bit words\.",
        r"Return (.+) with wraparound modulo 2\^64\.", r"Instruction palette: (\[.*\])\.",
        r"Use at most (0|[1-9][0-9]*) body instructions\.", r"Minimize (bytes|instructions)\.",
        r"Search limits: candidates ([1-9][0-9]*), checks ([1-9][0-9]*), deadline ([1-9][0-9]*) ms, solver ([1-9][0-9]*) ms\."])
    matches = [re.fullmatch(p, row[0]) for p, row in zip(patterns, lines)]
    need(all(matches), "declaration/profile/overflow/ABI syntax is unsupported")
    expr = Expression(matches[4][1], syntax, lines[4][1] + matches[4].start(1))
    expression = expr.node()
    need(expr.at == len(expr.tokens), "trailing expression")
    def bad(_):
        raise ValueError("palette floats/nonfinite numbers refused")
    palette = json.loads(matches[5][1], parse_float=bad, parse_constant=bad)
    limits = tuple(mm.integer(int(v), 1, bound, name) for v, bound, name in
                   zip(matches[8].groups(), LIMIT_BOUNDS, LIMIT_NAMES))
    kernel = Kernel(matches[1][1], matches[2][1], expression, freeze(palette),
                    int(matches[6][1]), matches[7][1], limits)
    problem(kernel)
    spans = [(name, at, at + len(line)) for name, (line, at) in zip(
        ("profile", "kernel", "target", "inputs", "result", "palette", "max-body", "objective", "limits"), lines)]
    return Document(kernel, source, syntax, tuple(spans + expr.spans))


def same_typed(left, right):
    if type(left) is not type(right):
        return False
    if type(left) in (tuple, list):
        return len(left) == len(right) and all(same_typed(a, b) for a, b in zip(left, right))
    if type(left) is dict:
        return left.keys() == right.keys() and all(same_typed(left[key], right[key]) for key in left)
    return left == right


def parsed_document(document):
    need(type(document) is Document and type(document.kernel) is Kernel
         and type(document.syntax) is str, "parsed word document required")
    reparsed = parse(document.source, document.syntax)
    need(all(same_typed(getattr(reparsed.kernel, name), getattr(document.kernel, name))
             for name in Kernel.__dataclass_fields__) and same_typed(reparsed.spans, document.spans),
         "document differs from exact source parsing")
    return reparsed


def render(document, syntax):
    document = parsed_document(document)
    need(syntax in ("symbolic", "english"), "unsupported syntax")
    k = document.kernel
    def expression(node):
        op = node[0]
        if op == "input": return node[1]
        if op == "const": return node[1]
        left = expression(node[1])
        if syntax == "symbolic":
            tail = "" if op == "not" else " " + (str(node[2]) if op in ("shl", "lshr") else expression(node[2]))
            return "(" + op + " " + left + tail + ")"
        if op == "not": return "(complement " + left + ")"
        if op in ("shl", "lshr"):
            return "(" + left + (" shifted left by " if op == "shl" else " shifted right logically by ") + str(node[2]) + ")"
        words = {"add": "plus", "sub": "minus", "mul": "times", "and": "bitwise and", "or": "bitwise or", "xor": "bitwise xor"}
        return "(" + left + " " + words[op] + " " + expression(node[2]) + ")"
    palette = json.dumps(arrays(k.palette), separators=(",", ":"))
    a, b, c, d = k.limits
    if syntax == "symbolic":
        rows = ["profile " + PROFILE + ";", f"kernel {k.name};", f"target {k.target};", "inputs x:u64,y:u64;",
                f"result {expression(k.expression)} modulo 2^64;", f"palette {palette};", f"max-body {k.max_body};",
                f"objective {k.objective};", f"limits candidates={a} checks={b} deadline-ms={c} solver-ms={d};"]
    else:
        rows = ["ZAL word kernel profile 1.", f"Kernel {k.name}.", f"Target {k.target}.", "Inputs x and y are unsigned 64-bit words.",
                f"Return {expression(k.expression)} with wraparound modulo 2^64.", f"Instruction palette: {palette}.",
                f"Use at most {k.max_body} body instructions.", f"Minimize {k.objective}.",
                f"Search limits: candidates {a}, checks {b}, deadline {c} ms, solver {d} ms."]
    return "\n".join(rows) + "\n"


def lower(document):
    document = parsed_document(document)
    k = document.kernel
    p = problem(k)
    limits = dict(zip(LIMIT_NAMES, k.limits))
    meaning = {"profile": PROFILE, "kernel": k.name, "problem": p, "loop_limits": limits, "observation_contract": OBSERVATION}
    spans = [{"id": name, "start": start, "end": end, "line": document.source.count("\n", 0, start) + 1,
              "column": start - document.source.rfind("\n", 0, start)} for name, start, end in document.spans]
    binding = {"schema": "zal/word-kernel-binding/1", "profile": PROFILE, "syntax": document.syntax,
        "revision": digest(canon(meaning)), "source_sha256": digest(document.source.encode("ascii")),
        "problem_sha256": digest(canon(p)), "bridge_sha256": digest(read_bytes(__file__, 131072)),
        "machine_sources": source_identity(), "loop_limits": limits, "source_map": spans, "observation_contract": OBSERVATION}
    return {"schema": "zal/word-kernel-lowering/1", "status": "lowered", "problem": p, "binding": binding,
            "authority": "none", "scope": "Exact declared modular word profile only; lowering is not checking or compilation"}


def subject(document, revision):
    lowered = lower(document)
    need(type(revision) is str and revision == lowered["binding"]["revision"], "stale source/meaning revision")
    return lowered


def explain(document, revision, topic):
    lowered = subject(document, revision)
    target = document.kernel.target
    scratch = list(mv._SCRATCH[target])
    if topic == "overflow":
        answer = "All expression arithmetic wraps modulo 2^64. MAX+1 is 0; no overflow error result exists. Checked overflow and U256/I256 are unsupported."
    elif topic == "clobbers":
        regs = list(mv._XREG.values()) if target == mv.X86 else [r for r in mv._RREG.values() if r != "zero"]
        answer = {"input_registers": {"x": "rdi", "y": "rsi"} if target == mv.X86 else {"x": "a0", "y": "a1"},
                  "return_register": "rax" if target == mv.X86 else "a0", "admitted_writable_registers": regs,
                  "entry_scratch_universally_quantified": scratch,
                  "note": "No preservation promise for admitted writable registers or unmodeled flags. This is the registered word-result model, not a complete ABI proof."}
    elif topic == "memory":
        answer = ("Body data-memory operands are refused. Final x86 RET reads the return address from the stack and changes control/stack state under caller ABI assumptions; these effects, flags, instruction fetch, CET, faults and timing are not modeled. Never infer never-touches-memory."
                  if target == mv.X86 else "Body loads/stores are refused. Final JALR return assumes a valid ra/link and execution environment; PC/return, instruction fetch, traps and timing are not established by this word-result model.")
    elif topic == "limits":
        answer = {"declared_loop_limits": lowered["binding"]["loop_limits"], "max_body_instructions": document.kernel.max_body,
                  "profile_caps": {"expression_nodes": 64, "expression_depth": 16, "palette_entries": 32, "code_bytes": 256},
                  "note": "Limits apply to each search invocation; candidate/emit/replay use the declared solver timeout. The existing session event/byte ceilings also apply. Repeated invocations do not share a total CPU budget. Search exhaustion is inconclusive, not unrealizability, global optimality or an owner decision."}
    elif topic == "checking":
        answer = "A proposer supplies bytes only. The unchanged independent decoder and SMT checker compare the returned u64 word for every x,y word and every entry scratch assignment under the registered model. Unknown/refused/counterexample and recorded session events remain visible, even when the incumbent is unchanged; a deadline never becomes success or a no-op. Emit rechecks and replay checks the exact source-derived problem again; no deployment grant follows."
    elif topic == "observation":
        answer = {"contract": OBSERVATION, "note": "Equal words do not establish stuttering equivalence of a complete machine or shell. Entry scratch, admitted clobbers, flags and implicit return effects retain the declared assumptions. Byte/body-instruction costs and search/check charges remain separate. The admitted straight-line body has bounded steps, not general external progress; temporal next/within requirements are unsupported."}
    else:
        raise ValueError("unsupported question topic")
    return {"schema": "zal/word-kernel-answer/1", "status": "answered", "topic": topic,
            "revision": revision, "source_sha256": lowered["binding"]["source_sha256"],
            "problem_sha256": lowered["binding"]["problem_sha256"], "answer": answer, "authority": "none"}


def what_if(document, revision, x, y):
    lowered = subject(document, revision)
    values = {"x": mm.word(x), "y": mm.word(y)}
    return {"schema": "zal/word-kernel-example/1", "status": "answered", "revision": revision,
            "source_sha256": lowered["binding"]["source_sha256"], "problem_sha256": lowered["binding"]["problem_sha256"],
            "inputs": {k: str(v) for k, v in values.items()},
            "expected": str(mm.evaluate(lowered["problem"]["expression"], values["x"], values["y"])),
            "scope": "One specification example; no candidate execution or full-word qualification", "authority": "none"}


def show(document, revision):
    lowered = subject(document, revision)
    return {"status": "shown", "revision": revision, "binding": lowered["binding"],
            "symbolic": render(document, "symbolic"), "english": render(document, "english"),
            "problem": lowered["problem"], "authority": "none"}


def question(document, revision, text):
    names = {"What happens on overflow?": "overflow", "What can be clobbered?": "clobbers",
             "Does it access memory?": "memory", "What are the limits?": "limits", "How is a candidate checked?": "checking", "What is observed?": "observation"}
    if text in names:
        return explain(document, revision, names[text])
    match = re.fullmatch(r"What if x is ([0-9]+) and y is ([0-9]+)\?", text)
    need(match is not None, "unsupported controlled question; use exact profile questions")
    return what_if(document, revision, *match.groups())


def new_directory(path):
    path = Path(path).absolute()
    need(".." not in path.parts and not path.is_relative_to(ROOT), "output must be outside source, without parent components")
    ordinary(path.parent, True)
    need(not path.exists() and not path.is_symlink(), "output already exists; preserve partials, no retry")
    path.mkdir(mode=0o700)
    return path


def write_json(path, value):
    ms._write(path, canon(value) + b"\n")


def load_bound(directory, revision, artifact=False):
    directory = ordinary(directory, True)
    expected = {"source.zal", "binding.json", "machine"} | ({"manifest.json"} if artifact else set())
    names = [p.name for p in islice(directory.iterdir(), len(expected) + 1)]
    need(len(names) == len(expected) and set(names) == expected, "bridge directory incomplete or has unexpected files")
    ordinary(directory / "machine", True)
    binding = read_json(directory / "binding.json")
    need(type(binding) is dict and binding.get("syntax") in ("symbolic", "english"), "binding syntax")
    doc = parse(read_bytes(directory / "source.zal").decode("ascii"), binding["syntax"])
    lowered = subject(doc, revision)
    need(same_typed(binding, lowered["binding"]), "source/bridge/checker/limits/map binding changed")
    actual = mm.validate_problem(read_json(directory / "machine/problem.json"))
    need(actual == lowered["problem"], "machine problem differs from exact ZAL lowering")
    return directory, doc, lowered


def open_session(source, syntax, directory, revision):
    doc = parse(source, syntax)
    lowered = subject(doc, revision)
    directory = new_directory(directory)
    ms._write(directory / "source.zal", source.encode("ascii"))
    write_json(directory / "binding.json", lowered["binding"])
    # Existing open requires a problem file. Keep it only in owned staging,
    # then remove staging after the existing API has copied and bound it.
    staging = directory / "lowered.problem.json"
    write_json(staging, lowered["problem"])
    result = ms.open_session(staging, directory / "machine")
    staging.unlink()
    return {"status": result["status"], "revision": lowered["binding"]["revision"],
            "source_sha256": lowered["binding"]["source_sha256"], "problem_sha256": result["problem_sha256"], "authority": "none"}


def feedback(document, lowered, result):
    verification = result.get("verification", {})
    answer = {"status": result["status"], "revision": lowered["binding"]["revision"],
              "source_sha256": lowered["binding"]["source_sha256"], "problem_sha256": lowered["binding"]["problem_sha256"],
              "machine": result, "authority": "none"}
    if verification.get("status") == "counterexample":
        inputs, scratch = verification.get("inputs"), verification.get("initial_registers")
        need(type(inputs) is dict and set(inputs) == {"x", "y"}, "counterexample inputs")
        need(type(scratch) is dict and set(scratch) == set(mv._SCRATCH[document.kernel.target]), "counterexample scratch must be complete")
        values = {k: mm.word(v) for k, v in inputs.items()}
        for value in scratch.values(): mm.word(value)
        expected, actual = mm.word(verification["expected"]), mm.word(verification["actual"])
        need(expected == mm.evaluate(lowered["problem"]["expression"], values["x"], values["y"]) and actual != expected,
             "counterexample contradicts exact source meaning")
        answer["human_question"] = {"text": f"At x={inputs['x']}, y={inputs['y']} and the shown entry scratch, should the required result remain {expected}?",
            "inputs": dict(inputs), "entry_scratch": dict(scratch), "expected": str(expected), "reported_actual": str(actual),
            "note": "Reported checker witness, not an owner answer or a specification edit. A repair must recheck the same source-derived problem."}
    return answer


def packet(directory, revision):
    directory, doc, lowered = load_bound(directory, revision)
    result = ms.packet(directory / "machine")
    need(result["problem_sha256"] == lowered["binding"]["problem_sha256"] and result["problem"] == lowered["problem"], "packet/source mismatch")
    return {"schema": "zal/word-kernel-proposer-packet/1", "revision": revision,
            "source_sha256": lowered["binding"]["source_sha256"], "source_map": lowered["binding"]["source_map"],
            "machine_packet": result, "human_feedback": [feedback(doc, lowered,
                {"status": row["verification"]["status"], "sequence": row["sequence"], "verification": row["verification"]})
                for row in result["feedback"]], "authority": "none"}


def candidate(directory, revision, proposal):
    directory, doc, lowered = load_bound(directory, revision)
    return feedback(doc, lowered, ms.submit(directory / "machine", Path(proposal), doc.kernel.limits[3]))


def search(directory, revision):
    directory, doc, lowered = load_bound(directory, revision)
    return feedback(doc, lowered, ms.search(directory / "machine", **dict(zip(LIMIT_NAMES, doc.kernel.limits))))


def emit(directory, revision, output):
    directory, doc, lowered = load_bound(directory, revision)
    output = new_directory(output)
    ms._write(output / "source.zal", doc.source.encode("ascii"))
    write_json(output / "binding.json", lowered["binding"])
    result = ms.emit(directory / "machine", output / "machine", doc.kernel.limits[3])
    if result["status"] != "emitted":
        return {"status": "inconclusive", "machine": result, "authority": "none"}
    manifest = {"schema": "zal/word-kernel-artifact/1", "profile": PROFILE, "revision": revision,
        "source_sha256": lowered["binding"]["source_sha256"], "problem_sha256": lowered["binding"]["problem_sha256"],
        "files": {n: {"sha256": digest(read_bytes(output / n)), "bytes": (output / n).stat().st_size} for n in ("source.zal", "binding.json")},
        "machine_manifest_sha256": digest(read_bytes(output / "machine/manifest.json")), "assurance": ASSURANCE, "authority": "none"}
    write_json(output / "manifest.json", manifest)
    return {"status": "emitted", "revision": revision, "problem_sha256": lowered["binding"]["problem_sha256"], "authority": "none"}


def replay(directory, revision):
    directory, doc, lowered = load_bound(directory, revision, True)
    manifest = read_json(directory / "manifest.json")
    expected = {"schema": "zal/word-kernel-artifact/1", "profile": PROFILE, "revision": revision,
        "source_sha256": lowered["binding"]["source_sha256"], "problem_sha256": lowered["binding"]["problem_sha256"],
        "files": {n: {"sha256": digest(read_bytes(directory / n)), "bytes": (directory / n).stat().st_size} for n in ("source.zal", "binding.json")},
        "machine_manifest_sha256": digest(read_bytes(directory / "machine/manifest.json")), "assurance": ASSURANCE, "authority": "none"}
    need(manifest == expected, "artifact/source/assurance binding changed")
    return feedback(doc, lowered, ms.replay_artifact(directory / "machine", doc.kernel.limits[3]))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    subs = parser.add_subparsers(dest="command", required=True)
    for name in ("read", "show", "render", "lower", "open", "question", "what-if"):
        p = subs.add_parser(name)
        p.add_argument("--source", type=Path, required=True)
        p.add_argument("--syntax", choices=("symbolic", "english"), default="symbolic")
        if name != "read": p.add_argument("--revision", required=True)
        if name == "render": p.add_argument("--to", choices=("symbolic", "english"), required=True)
        if name == "open": p.add_argument("--session", type=Path, required=True)
        if name == "question": p.add_argument("--text", required=True)
        if name == "what-if":
            p.add_argument("--x", required=True); p.add_argument("--y", required=True)
    for name in ("packet", "candidate", "search", "emit", "replay"):
        p = subs.add_parser(name)
        p.add_argument("--artifact" if name == "replay" else "--session", type=Path, required=True)
        p.add_argument("--revision", required=True)
        if name == "candidate": p.add_argument("--proposal", type=Path, required=True)
        if name == "emit": p.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        if hasattr(args, "source"):
            source = read_bytes(args.source).decode("ascii")
            doc = parse(source, args.syntax)
            if args.command == "read": result = lower(doc)
            elif args.command == "open": result = open_session(source, args.syntax, args.session, args.revision)
            else:
                subject(doc, args.revision)
                if args.command == "render":
                    print(render(doc, args.to), end=""); return 0
                if args.command == "lower": result = lower(doc)
                elif args.command == "show": result = show(doc, args.revision)
                elif args.command == "question": result = question(doc, args.revision, args.text)
                else: result = what_if(doc, args.revision, args.x, args.y)
        elif args.command == "packet": result = packet(args.session, args.revision)
        elif args.command == "candidate": result = candidate(args.session, args.revision, args.proposal)
        elif args.command == "search": result = search(args.session, args.revision)
        elif args.command == "emit": result = emit(args.session, args.revision, args.out)
        else: result = replay(args.artifact, args.revision)
        print(json.dumps(result, sort_keys=True, allow_nan=False))
        return 1 if result.get("status") in ("counterexample", "refused") else 2 if result.get("status") in ("inconclusive", "no-checked-candidate") else 0
    except ImportError as error:
        print(json.dumps({"status": "inconclusive", "reason": str(error), "authority": "none"})); return 2
    except (ValueError, OSError, TypeError, KeyError, IndexError, RecursionError) as error:
        print(json.dumps({"status": "refused", "reason": str(error), "authority": "none"})); return 65


if __name__ == "__main__":
    raise SystemExit(main())
