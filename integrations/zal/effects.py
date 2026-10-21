"""ZAL effect signatures, profile zal/effects/1, lowered to the zeno-fcis/shell-effects/1 record.

No eval, model calls, I/O, authority capabilities or effect execution. The symbolic and
controlled-English sources parse to one frozen model. `check` runs named structural
checks (port coverage, frame exhaustiveness, footprints, laws, V2 mapping and record
kinds); `lower` emits the record's canonical bytes exactly as the shell registry frames
them, so the same source gives the same registry identity.
"""

from __future__ import annotations

from dataclasses import dataclass
import hashlib
import re

import adt
from adt import TYPE_NAME, Decl, Tokens, Ty, Types, card_data, parse_members, parse_ty, render_members
from behavior import NAME, Refusal, canonical
from constructs import Construct, human_text


PROFILE = "zal/effects/1"
SCHEMA = "zeno-fcis/shell-effects/1"
MAX_SOURCE = 64 * 1024
MAX_STATEMENTS = 160
LAW_NAME = r"[a-z][a-z0-9-]{0,31}"
CALL = r"[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*"
STATEMENT_TEXT = r'[ !#-~]{1,512}'
RESULT = "T"
FRAMES = {"applied": "the operation took effect", "unchanged": "nothing changed",
          "either": "it may or may not have taken effect"}
# English list words; a resource or case with one of these names would read ambiguously.
RESERVED = frozenset({"and", "nothing", "reading", "writing", "means", "the", "of", "law", "laws"})
STORE = Decl("Store", "opaque", (1 << 288, "a store handle: a 32-byte store identity and a 32-bit handle number"))


@dataclass(frozen=True)
class Resource:
    name: str
    bit: int

    def data(self):
        return [self.name, self.bit]


@dataclass(frozen=True)
class Capability:
    name: str
    number: int
    store: bool
    reads: tuple
    writes: tuple

    def data(self):
        return [self.name, self.number, self.store, list(self.reads), list(self.writes)]


@dataclass(frozen=True)
class Answer:
    """A result shape: a variant whose one RESULT case carries the operation's payload."""
    name: str
    number: int
    cases: tuple

    def data(self):
        return [self.name, self.number, [[c, p if p == RESULT else p.data()] for c, p in self.cases]]


@dataclass(frozen=True)
class Frame:
    answer: str
    entries: tuple

    def data(self):
        return [self.answer, [list(e) for e in self.entries]]


@dataclass(frozen=True)
class Port:
    name: str
    tag: int

    def data(self):
        return [self.name, self.tag]


@dataclass(frozen=True)
class Law:
    id: int
    name: str
    statement: str

    def data(self):
        return [self.id, self.name, self.statement]


@dataclass(frozen=True)
class Operation:
    port: str
    arguments: tuple
    answer: str
    payload: Ty

    def data(self):
        return [self.port, [[n, t.data()] for n, t in self.arguments], self.answer, self.payload.data()]


@dataclass(frozen=True)
class Footprint:
    port: str
    capability: str
    reads: tuple
    writes: tuple
    laws: tuple
    v2: tuple

    def data(self):
        return [self.port, self.capability, list(self.reads), list(self.writes), list(self.laws),
                [self.v2[0], *([list(self.v2[1])] if self.v2[0] == "calls" else self.v2[1:])]]


@dataclass(frozen=True)
class Signatures:
    name: str
    schema: str
    resources: tuple
    capabilities: tuple
    answers: tuple
    frames: tuple
    ports: tuple
    types: Types
    laws: tuple
    operations: tuple
    footprints: tuple

    def __post_init__(self):
        validate(self)

    def data(self):
        return {"profile": PROFILE, "name": self.name, "schema": self.schema,
                "resources": [r.data() for r in self.resources],
                "capabilities": [c.data() for c in self.capabilities],
                "answers": [a.data() for a in self.answers], "frames": [f.data() for f in self.frames],
                "ports": [p.data() for p in self.ports], "types": self.types.data(),
                "laws": [law.data() for law in self.laws], "operations": [o.data() for o in self.operations],
                "footprints": [f.data() for f in self.footprints]}

    @property
    def revision(self):
        return hashlib.sha256(canonical(self.data()).encode()).hexdigest()

    def lookup(self, kind, name):
        table = {"resource": self.resources, "capability": self.capabilities, "answer": self.answers,
                 "port": self.ports, "operation": self.operations, "footprint": self.footprints,
                 "frame": self.frames}[kind]
        key = "port" if kind in {"operation", "footprint"} else "answer" if kind == "frame" else "name"
        return next((item for item in table if getattr(item, key) == name), None)

    def ordered_ports(self):
        """Port names in tag order, then any operation or footprint naming an undeclared port."""
        declared = [p.name for p in self.ports]
        extra = sorted(({o.port for o in self.operations} | {f.port for f in self.footprints}) - set(declared))
        return declared + extra

    def render(self, english=False) -> str:
        return "\n".join(statement_lines(self, english)) + "\n"


def english_list(items) -> str:
    items = list(items)
    if not items:
        return "nothing"
    return items[0] if len(items) == 1 else ", ".join(items[:-1]) + " and " + items[-1]


def parse_english_list(text, pattern):
    items = [] if text == "nothing" else re.split(r", | and ", text)
    if any(not re.fullmatch(pattern, item) for item in items) or english_list(items) != text:
        raise Refusal(f"Not a canonical English list: {text}")
    return items


def parse_symbolic_list(text, pattern):
    items = [] if text == "" else text.split(", ")
    if any(not re.fullmatch(pattern, item) for item in items):
        raise Refusal(f"Not a canonical list: {text}")
    return items


def render_footprint_tail(footprint, english):
    kind = footprint.v2[0]
    if english:
        if kind == "calls":
            calls = footprint.v2[1]
            return f"maps to the V2 call{'' if len(calls) == 1 else 's'} {english_list(calls)}"
        if kind == "external":
            return f"maps to the external {footprint.v2[1]} outside the store"
        return "has no default V2 call"
    if kind == "calls":
        return "v2 calls [" + ", ".join(footprint.v2[1]) + "]"
    return f"v2 external {footprint.v2[1]}" if kind == "external" else "v2 opt_in"


def answer_cases_text(answer, english):
    parts = []
    for case, payload in answer.cases:
        if english:
            parts.append(f"{case} ({'the result' if payload == RESULT else payload.render(True)})")
        else:
            parts.append(f"{case}({RESULT if payload == RESULT else payload.render()})")
    return (", " if english else " | ").join(parts)


def statement_lines(model: Signatures, english=False):
    """The canonical statements, in canonical order, with each statement's concept."""
    return [line for _, line in concept_lines(model, english)]


def concept_lines(model: Signatures, english=False):
    lines = []
    def add(concept, line):
        lines.append((concept, line))
    names = lambda items: [model.lookup("resource", n).name for n in items]
    if english:
        add("header", "ZAL effects version 1.")
        add("name", f"The effect signature is {model.name}.")
        add("schema", f"It lowers to the record schema {model.schema}.")
        for r in model.resources:
            add("resource", f"Resource {r.name} has bit value {r.bit}.")
        for c in model.capabilities:
            add("capability", f"Capability {c.name}, number {c.number}, {'is a store capability that ' if c.store else ''}"
                              f"allows reading {english_list(names(c.reads))} and writing {english_list(names(c.writes))}.")
        for a in model.answers:
            add("answer", f"Answer {a.name}, number {a.number}, is one of {answer_cases_text(a, True)}.")
        for f in model.frames:
            add("frame", f"For answer {f.answer}, " + ", ".join(f"{c} means {FRAMES[w]}" for c, w in f.entries) + ".")
        add("ports", "The ports are " + ", ".join(f"{p.name} ({p.tag})" for p in model.ports) + ".")
    else:
        add("header", "zal effects 1;")
        add("name", f"effects {model.name};")
        add("schema", f"schema {model.schema};")
        for r in model.resources:
            add("resource", f"resource {r.name} = {r.bit};")
        for c in model.capabilities:
            add("capability", f"capability {c.name} = {c.number}: {'store, ' if c.store else ''}"
                              f"reads {{{', '.join(c.reads)}}}, writes {{{', '.join(c.writes)}}};")
        for a in model.answers:
            add("answer", f"answer {a.name} = {a.number}: {answer_cases_text(a, False)};")
        for f in model.frames:
            add("frame", f"frame {f.answer}: " + ", ".join(f"{c} {w}" for c, w in f.entries) + ";")
        add("ports", "ports " + ", ".join(f"{p.name} = {p.tag}" for p in model.ports) + ";")
    for decl in model.types.decls:
        add("type", decl.render(english))
    for law in model.laws:
        add("law", f'{"Law" if english else "law"} {law.id} {law.name}: "{law.statement}"{"." if english else ";"}')
    for port in model.ordered_ports():
        operation, footprint = model.lookup("operation", port), model.lookup("footprint", port)
        if operation is not None:
            if english:
                takes = render_members(operation.arguments, True) if operation.arguments else "nothing"
                add("operation", f"Operation {port} takes {takes} and answers {operation.answer} of "
                                 f"{operation.payload.render(True)}.")
            else:
                add("operation", f"operation {port}({render_members(operation.arguments)}) -> {operation.answer} "
                                 f"{operation.payload.render()};")
        if footprint is not None:
            if english:
                laws = [str(i) for i in footprint.laws]
                add("footprint", f"Operation {port} needs capability {footprint.capability}, reads "
                                 f"{english_list(footprint.reads)}, writes {english_list(footprint.writes)}, obeys "
                                 f"law{'s' if len(laws) != 1 else ''} {english_list(laws)}, and "
                                 f"{render_footprint_tail(footprint, True)}.")
            else:
                add("footprint", f"footprint {port}: capability {footprint.capability}, reads {{{', '.join(footprint.reads)}}}, "
                                 f"writes {{{', '.join(footprint.writes)}}}, laws [{', '.join(str(i) for i in footprint.laws)}], "
                                 f"{render_footprint_tail(footprint, False)};")
    return lines


def statement_patterns(english=False):
    """The statement registry, keyed by concept; each line matches exactly one pattern."""
    tn = TYPE_NAME
    if english:
        return {
            "header": r"ZAL effects version 1\.",
            "name": rf"The effect signature is ({NAME})\.",
            "schema": r"It lowers to the record schema ([a-z0-9/._-]{1,64})\.",
            "resource": rf"Resource ({NAME}) has bit value ([0-9]{{1,3}})\.",
            "capability": (rf"Capability ({tn}), number ([0-9]{{1,3}}), (is a store capability that )?"
                           r"allows reading (.+) and writing (.+)\."),
            "answer": rf"Answer ({tn}), number ([0-9]{{1,3}}), is one of (.+)\.",
            "frame": rf"For answer ({tn}), (.+)\.",
            "ports": r"The ports are (.+)\.",
            "type": adt.type_patterns(True),
            "law": rf'Law ([0-9]{{1,5}}) ({LAW_NAME}): "({STATEMENT_TEXT})"\.',
            "operation": rf"Operation ({tn}) takes (.+) and answers ({tn}) of (.+)\.",
            "footprint": (rf"Operation ({tn}) needs capability ({tn}), reads (.+), writes (.+), "
                          r"obeys (laws?) (.+), and (.+)\."),
        }
    return {
        "header": r"zal effects 1;",
        "name": rf"effects ({NAME});",
        "schema": r"schema ([a-z0-9/._-]{1,64});",
        "resource": rf"resource ({NAME}) = ([0-9]{{1,3}});",
        "capability": rf"capability ({tn}) = ([0-9]{{1,3}}): (store, )?reads \{{(.*)\}}, writes \{{(.*)\}};",
        "answer": rf"answer ({tn}) = ([0-9]{{1,3}}): (.+);",
        "frame": rf"frame ({tn}): (.+);",
        "ports": r"ports (.+);",
        "type": adt.type_patterns(),
        "law": rf'law ([0-9]{{1,5}}) ({LAW_NAME}): "({STATEMENT_TEXT})";',
        "operation": rf"operation ({tn})\((.*)\) -> ({tn}) (.+);",
        "footprint": (rf"footprint ({tn}): capability ({tn}), reads \{{(.*)\}}, writes \{{(.*)\}}, "
                      r"laws \[(.*)\], v2 (.+);"),
    }


SINGLETONS = ("header", "name", "schema", "ports")


def _number(text, low, high, what):
    value = int(text)
    if text != str(value) or not low <= value <= high:
        raise Refusal(f"{what} {text} lies outside {low}..={high}")
    return value


def _resource_set(text, english, known):
    items = parse_english_list(text, NAME) if english else parse_symbolic_list(text, NAME)
    for item in items:
        if item not in known:
            raise Refusal(f"Unknown resource {item}")
    if len(set(items)) != len(items):
        raise Refusal("A resource set names one resource twice")
    return tuple(sorted(items, key=lambda name: known[name]))


def _answer_cases(text, english, name):
    cases = []
    for part in (text.split(", ") if english else text.split(" | ")):
        match = re.fullmatch(rf"({NAME})(?: \((.+)\))?" if english else rf"({NAME})(?:\((.+)\))?", part)
        if not match or match[1] in RESERVED:
            raise Refusal(f"Answer {name}: expected a case name with an optional payload near {part}")
        if match[2] is None:
            raise Refusal(f"Answer {name}: case {match[1]} needs a payload")
        if match[2] == ("the result" if english else RESULT):
            payload = RESULT
        else:
            tokens = Tokens(match[2], english)
            payload = parse_ty(tokens)
            tokens.done()
        cases.append((match[1], payload))
    return tuple(cases)


def _frame_entries(text, english, name):
    entries = []
    words = {phrase: word for word, phrase in FRAMES.items()}
    for part in text.split(", "):
        match = (re.fullmatch(rf"({NAME}) means (.+)", part) if english else re.fullmatch(rf"({NAME}) ({NAME})", part))
        word = match and (words.get(match[2]) if english else match[2] if match[2] in FRAMES else None)
        if not word:
            raise Refusal(f"Frame {name}: each case is applied, unchanged or either, near {part}")
        entries.append((match[1], word))
    return tuple(entries)


def _v2(text, english, port):
    if english:
        match = re.fullmatch(r"maps to the V2 (calls?) (.+)", text)
        if match:
            calls = parse_english_list(match[2], CALL)
            if (match[1] == "calls") != (len(calls) != 1):
                raise Refusal(f"Operation {port}: write the V2 call or calls in canonical form")
            return ("calls", tuple(calls))
        match = re.fullmatch(rf"maps to the external ({CALL}) outside the store", text)
        if match:
            return ("external", match[1])
        if text == "has no default V2 call":
            return ("opt_in",)
    else:
        match = re.fullmatch(r"calls \[(.*)\]", text)
        if match:
            return ("calls", tuple(parse_symbolic_list(match[1], CALL)))
        match = re.fullmatch(rf"external ({CALL})", text)
        if match:
            return ("external", match[1])
        if text == "opt_in":
            return ("opt_in",)
    raise Refusal(f"Operation {port}: the V2 mapping is calls, external or opt-in")


def parse(text: str, syntax="symbolic") -> Signatures:
    if not isinstance(text, str) or len(text.encode()) > MAX_SOURCE or not text.isascii():
        raise Refusal("Source must be ASCII text at most 64 KiB")
    if syntax not in {"symbolic", "english"}:
        raise Refusal("Choose symbolic or english syntax")
    english = syntax == "english"
    lines = [line.strip() for line in text.splitlines() if line.strip() and not line.lstrip().startswith("#")]
    if len(lines) > MAX_STATEMENTS:
        raise Refusal(f"Source exceeds {MAX_STATEMENTS} statements")
    patterns = statement_patterns(english)
    found = {key: [] for key in patterns}
    for number, line in enumerate(lines, 1):
        matches = [(key, re.fullmatch(pattern, line)) for key, pattern in patterns.items()]
        matches = [(key, match) for key, match in matches if match]
        if len(matches) != 1:
            raise Refusal(f"Statement {number}: not in the controlled grammar; clarify this statement")
        key, match = matches[0]
        if key in SINGLETONS and found[key]:
            raise Refusal(f"Statement {number}: duplicate {key}")
        found[key].append(match.groups())
    missing = [key for key in SINGLETONS if not found[key]]
    if missing:
        raise Refusal("Missing explicit declarations: " + ", ".join(missing))

    resources = []
    for name, bit in found["resource"]:
        if name in RESERVED:
            raise Refusal(f"Resource {name} uses a reserved word")
        resources.append(Resource(name, _number(bit, 1, 128, "Resource bit")))
    resources = tuple(sorted(resources, key=lambda r: r.bit))
    bits = {r.name: r.bit for r in resources}
    if len(bits) != len(resources):
        raise Refusal("Resource names must be distinct")
    capabilities = []
    for name, number, store, reads, writes in found["capability"]:
        capabilities.append(Capability(name, _number(number, 0, 255, "Capability number"), store is not None,
                                       _resource_set(reads, english, bits), _resource_set(writes, english, bits)))
    answers = tuple(sorted((Answer(n, _number(k, 0, 255, "Answer number"), _answer_cases(body, english, n))
                            for n, k, body in found["answer"]), key=lambda a: a.number))
    frames = []
    for name, body in found["frame"]:
        frames.append(Frame(name, _frame_entries(body, english, name)))
    port_text = found["ports"][0][0]
    ports = []
    for part in port_text.split(", "):
        match = re.fullmatch(rf"({TYPE_NAME}) \(([0-9]{{1,3}})\)" if english else rf"({TYPE_NAME}) = ([0-9]{{1,3}})", part)
        if not match:
            raise Refusal(f"The port list names each port with its tag, near {part}")
        ports.append(Port(match[1], _number(match[2], 1, 255, "Port tag")))
    decls = []
    for name, body in found["type"]:
        decls.append(adt.parse_decl(name, body, english))
    laws = tuple(sorted((Law(_number(i, 0, 65535, "Law ID"), n, s) for i, n, s in found["law"]), key=lambda law: law.id))
    operations = []
    for port, arguments, answer, payload in found["operation"]:
        if english:
            fields = () if arguments == "nothing" else _members(arguments, True)
        else:
            fields = () if arguments == "" else _members(arguments, False)
        tokens = Tokens(payload, english)
        result = parse_ty(tokens)
        tokens.done()
        operations.append(Operation(port, fields, answer, result))
    footprints = []
    for groups in found["footprint"]:
        if english:
            port, capability, reads, writes, word, laws_text, tail = groups
            law_ids = parse_english_list(laws_text, r"[0-9]{1,5}")
            if (word == "law") != (len(law_ids) == 1):
                raise Refusal(f"Operation {port}: write law for one law ID and laws otherwise")
            v2 = _v2(tail, True, port)
        else:
            port, capability, reads, writes, laws_text, tail = groups
            law_ids = parse_symbolic_list(laws_text, r"[0-9]{1,5}")
            v2 = _v2(tail, False, port)
        footprints.append(Footprint(port, capability, _resource_set(reads, english, bits),
                                    _resource_set(writes, english, bits),
                                    tuple(_number(i, 0, 65535, "Law ID") for i in law_ids), v2))
    types = Types(tuple(sorted(decls, key=lambda d: d.name)), (STORE,))
    return Signatures(found["name"][0][0], found["schema"][0][0], resources,
                      tuple(sorted(capabilities, key=lambda c: c.number)), answers,
                      tuple(sorted(frames, key=lambda f: f.answer)), tuple(sorted(ports, key=lambda p: p.tag)),
                      types, laws, tuple(sorted(operations, key=lambda o: o.port)),
                      tuple(sorted(footprints, key=lambda f: f.port)))


def _members(text, english):
    tokens = Tokens(text, english)
    members = parse_members(tokens, False)
    tokens.done()
    return members


def validate(model: Signatures):
    """Validate direct IR construction: names resolve, numbering is canonical, nothing repeats."""
    if not re.fullmatch(NAME, model.name) or not re.fullmatch(r"[a-z0-9/._-]{1,64}", model.schema):
        raise Refusal("Invalid effect signature or schema name")
    if not isinstance(model.types, Types) or model.types.prelude != (STORE,):
        raise Refusal("IR types must be checked types with this profile's built-in Store")
    for label, items, kind in [("resources", model.resources, Resource), ("capabilities", model.capabilities, Capability),
                               ("answers", model.answers, Answer), ("frames", model.frames, Frame),
                               ("ports", model.ports, Port), ("laws", model.laws, Law),
                               ("operations", model.operations, Operation), ("footprints", model.footprints, Footprint)]:
        if not isinstance(items, tuple) or len(items) > 64 or any(not isinstance(x, kind) for x in items):
            raise Refusal(f"IR {label} must be a bounded tuple")
    bits = {r.name: r.bit for r in model.resources}
    if (len(bits) != len(model.resources) or len({r.bit for r in model.resources}) != len(model.resources)
            or any(not re.fullmatch(NAME, r.name) or r.name in RESERVED or r.bit & (r.bit - 1) or not 1 <= r.bit <= 128
                   for r in model.resources)
            or [r.bit for r in model.resources] != sorted(r.bit for r in model.resources)):
        raise Refusal("Resources need distinct names and distinct one-bit values, sorted by bit")
    def resource_set(items, owner):
        if not isinstance(items, tuple) or any(i not in bits for i in items) or list(items) != sorted(set(items), key=bits.get):
            raise Refusal(f"{owner}: a resource set names declared resources once each, in bit order")
    for label, items in [("Capability", model.capabilities), ("Answer", model.answers)]:
        if ([x.number for x in items] != list(range(len(items)))
                or len({x.name for x in items}) != len(items) or any(not re.fullmatch(TYPE_NAME, x.name) for x in items)):
            raise Refusal(f"{label} names must be distinct and numbered 0, 1, 2, ... in order")
    for capability in model.capabilities:
        if type(capability.store) is not bool:
            raise Refusal(f"Capability {capability.name}: store is true or false")
        resource_set(capability.reads, "Capability " + capability.name)
        resource_set(capability.writes, "Capability " + capability.name)
    for answer in model.answers:
        cases = [c for c, _ in answer.cases]
        if (not answer.cases or len(set(cases)) != len(cases) or any(not re.fullmatch(NAME, c) or c in RESERVED for c in cases)
                or [p for _, p in answer.cases].count(RESULT) != 1
                or any(p != RESULT and not isinstance(p, Ty) for _, p in answer.cases)):
            raise Refusal(f"Answer {answer.name} needs distinct cases and exactly one case carrying the result T")
        for _, payload in answer.cases:
            if payload != RESULT:
                _known(model, payload, "Answer " + answer.name)
    answers = {a.name for a in model.answers}
    for frame in model.frames:
        if frame.answer not in answers:
            raise Refusal(f"Frame for unknown answer {frame.answer}")
        if any(not isinstance(e, tuple) or len(e) != 2 or e[1] not in FRAMES for e in frame.entries):
            raise Refusal(f"Frame {frame.answer}: each case is applied, unchanged or either")
    if [f.answer for f in model.frames] != sorted({f.answer for f in model.frames}):
        raise Refusal("Each answer has at most one frame statement")
    if (not model.ports or len({p.name for p in model.ports}) != len(model.ports)
            or len({p.tag for p in model.ports}) != len(model.ports)
            or any(not re.fullmatch(TYPE_NAME, p.name) or not 1 <= p.tag <= 255 for p in model.ports)
            or [p.tag for p in model.ports] != sorted(p.tag for p in model.ports)):
        raise Refusal("Ports need distinct names and distinct tags from 1 to 255, in tag order")
    collisions = {d.name for d in model.types.decls} & (answers | {RESULT})
    if collisions:
        raise Refusal("A type may not reuse an answer name or T: " + ", ".join(sorted(collisions)))
    if ([law.id for law in model.laws] != sorted({law.id for law in model.laws})
            or any(not re.fullmatch(LAW_NAME, law.name) or not re.fullmatch(STATEMENT_TEXT, law.statement)
                   for law in model.laws)):
        raise Refusal("Laws need distinct IDs in ascending order, a name and a quoted statement")
    capabilities = {c.name for c in model.capabilities}
    for label, items in [("operation", model.operations), ("footprint", model.footprints)]:
        if [x.port for x in items] != sorted({x.port for x in items}):
            raise Refusal(f"Each port has at most one {label} statement")
    for operation in model.operations:
        owner = "Operation " + operation.port
        if not re.fullmatch(TYPE_NAME, operation.port) or operation.answer not in answers:
            raise Refusal(f"{owner} answers with an undeclared answer {operation.answer}")
        names = [n for n, _ in operation.arguments]
        if len(set(names)) != len(names) or any(not re.fullmatch(NAME, n) for n in names) or len(names) > adt.MAX_MEMBERS:
            raise Refusal(f"{owner} needs distinct argument names")
        for _, ty in operation.arguments:
            _known(model, ty, owner)
        _known(model, operation.payload, owner)
    for footprint in model.footprints:
        owner = "Operation " + footprint.port
        if not re.fullmatch(TYPE_NAME, footprint.port) or footprint.capability not in capabilities:
            raise Refusal(f"{owner} needs an undeclared capability {footprint.capability}")
        resource_set(footprint.reads, owner)
        resource_set(footprint.writes, owner)
        kind = footprint.v2[0] if isinstance(footprint.v2, tuple) and footprint.v2 else None
        if (not isinstance(footprint.laws, tuple) or any(type(i) is not int or not 0 <= i <= 65535 for i in footprint.laws)
                or kind == "calls" and (len(footprint.v2) != 2 or not isinstance(footprint.v2[1], tuple)
                                        or any(not re.fullmatch(CALL, c) for c in footprint.v2[1]))
                or kind == "external" and (len(footprint.v2) != 2 or not re.fullmatch(CALL, footprint.v2[1]))
                or kind == "opt_in" and footprint.v2 != ("opt_in",) or kind not in {"calls", "external", "opt_in"}):
            raise Refusal(f"{owner}: invalid law list or V2 mapping")
    if any(len(model.render(english).encode()) > MAX_SOURCE for english in [False, True]):
        raise Refusal("Canonical symbolic or English source exceeds 64 KiB")


def _known(model, ty, owner):
    for name in adt.references(ty):
        if name not in model.types.by_name:
            raise Refusal(f"{owner} refers to undeclared type {name}")


CHECKS = ("record_schema", "port_coverage", "frames", "footprints", "laws", "v2_mapping", "record_kinds")


def _failure(model, checks, name, reason, checker_identity, **details):
    return {"status": "fail", "revision": model.revision, "profile": PROFILE, "checker": checker_identity,
            "obligation": name, "reason": reason, **details, "checks": {**checks, name: "fail"}, "authority": "none"}


def check(model: Signatures, checker_identity="unbound") -> dict:
    """Run every named check in order; the first failure is reported with its reason."""
    checks = {name: "not-run" for name in CHECKS}
    def fail(name, reason, **details):
        return _failure(model, checks, name, reason, checker_identity, **details)
    if model.schema != SCHEMA:
        return fail("record_schema", f"zal/effects/1 lowers only to {SCHEMA}, not {model.schema}")
    checks["record_schema"] = "pass"
    declared = {p.name for p in model.ports}
    for port in model.ports:
        for kind in ("operation", "footprint"):
            if model.lookup(kind, port.name) is None:
                return fail("port_coverage", f"port {port.name} has no {kind}", port=port.name)
    for item in (*model.operations, *model.footprints):
        if item.port not in declared:
            return fail("port_coverage", f"operation {item.port} names no declared port", port=item.port)
    checks["port_coverage"] = "pass"
    for answer in model.answers:
        frame = model.lookup("frame", answer.name)
        if frame is None:
            return fail("frames", f"answer {answer.name} has no frame", answer=answer.name)
        cases = [c for c, _ in answer.cases]
        covered = [c for c, _ in frame.entries]
        for case in covered:
            if case not in cases:
                return fail("frames", f"frame {answer.name} names {case}, which is not a case of {answer.name}",
                            answer=answer.name, case=case)
            if covered.count(case) > 1:
                return fail("frames", f"frame {answer.name} gives case {case} twice", answer=answer.name, case=case)
        for case in cases:
            if case not in covered:
                return fail("frames", f"frame {answer.name} misses case {case}", answer=answer.name, case=case)
    checks["frames"] = "pass"
    for footprint in model.footprints:
        capability = model.lookup("capability", footprint.capability)
        for access, used, allowed in [("reads", footprint.reads, capability.reads),
                                      ("writes", footprint.writes, capability.writes)]:
            beyond = [r for r in used if r not in allowed]
            if beyond:
                return fail("footprints", f"operation {footprint.port} {access} {', '.join(beyond)}, beyond capability "
                                          f"{capability.name}", port=footprint.port, capability=capability.name)
    checks["footprints"] = "pass"
    ids = {law.id for law in model.laws}
    for footprint in model.footprints:
        if not footprint.laws:
            return fail("laws", f"operation {footprint.port} names no law", port=footprint.port)
        for index, law in enumerate(footprint.laws):
            if law in footprint.laws[:index]:
                return fail("laws", f"operation {footprint.port} names law {law} twice", port=footprint.port, law=law)
            if law not in ids:
                return fail("laws", f"operation {footprint.port} names law {law}, which does not exist",
                            port=footprint.port, law=law)
    for law in model.laws:
        if not any(law.id in f.laws for f in model.footprints):
            return fail("laws", f"law {law.id} {law.name} is named by no operation", law=law.id)
    checks["laws"] = "pass"
    for footprint in model.footprints:
        store = model.lookup("capability", footprint.capability).store
        kind = footprint.v2[0]
        if kind == "calls" and not footprint.v2[1]:
            return fail("v2_mapping", f"operation {footprint.port} maps to an empty list of V2 calls", port=footprint.port)
        if store and kind != "calls":
            return fail("v2_mapping", f"store operation {footprint.port} must map to V2 calls", port=footprint.port)
    checks["v2_mapping"] = "pass"
    for port in model.ports:
        operation = model.lookup("operation", port.name)
        problem = _unlowerable(model, arguments=operation.arguments) or _unlowerable(model, operation.payload)
        if problem:
            return fail("record_kinds", f"operation {port.name}: {problem}", port=port.name)
    checks["record_kinds"] = "pass"
    return {"status": "pass-with-scope", "revision": model.revision, "profile": PROFILE, "checker": checker_identity,
            "scope": ("Structural checks of the declared signatures: every declared port has one operation and one "
                      "footprint, every answer frame covers each case once, footprints stay within their capability, "
                      "laws exist and are used, store ports map to V2 calls, and every type has a record kind. "
                      "Law statements are handler obligations; they are named here, not checked."),
            "checks": checks, "ports": len(model.ports), "laws": len(model.laws), "types": len(model.types.decls),
            "identity": hashlib.sha256(_record_bytes(model)).hexdigest(), "authority": "none"}


def _unlowerable(model, ty=None, arguments=None):
    """Why a type has no shell-effects/1 kind, or None. `arguments` checks an inline argument record."""
    types = model.types
    if arguments is not None:
        for _, field in arguments:
            problem = _unlowerable(model, field)
            if problem:
                return problem
        depth = 1 + max([types.depth(t) for _, t in arguments], default=0)
    else:
        depth = types.depth(ty)
        problem = _kind_problem(types, ty)
        if problem:
            return problem
    if depth > adt.MAX_DEPTH:
        return f"nests deeper than {adt.MAX_DEPTH} levels"
    return None


def _kind_problem(types, ty):
    target = types.resolve(ty)
    if isinstance(target, Decl):
        if target.form == "opaque":
            return None
        for _, member in target.body:
            problem = member is not None and _kind_problem(types, member)
            if problem:
                return problem
        return None
    if target.kind == "bool":
        return "bool has no record kind; use int 0..=1 or a two-case variant"
    if target.kind == "bytes" and target.args[0] != 32:
        return f"exactly {target.args[0]} bytes has no record kind; only exactly 32 bytes (a hash) does"
    if target.kind == "int" and target.args[0] < 0:
        return f"int {target.args[0]}..={target.args[1]} is signed; record integers are unsigned 64-bit"
    if target.kind == "list":
        return _kind_problem(types, target.args[0])
    return None


def _count(out, length):
    out += length.to_bytes(8, "big")


def _text(out, value):
    _count(out, len(value.encode()))
    out += value.encode()


def _kind(out, types, ty):
    target = types.resolve(ty)
    if isinstance(target, Decl):
        if target.form == "opaque":
            out.append(1)
            return
        out.append(5 if target.form == "record" else 6)
        _members_kind(out, types, target.body)
        return
    kind, args = target.kind, target.args
    if kind == "unit":
        out.append(0)
    elif kind == "bytes":
        out.append(2)
    elif kind == "bytes_max":
        out.append(3)
        out += args[0].to_bytes(8, "big")
    elif kind == "int":
        out.append(4)
        out += args[0].to_bytes(8, "big") + args[1].to_bytes(8, "big")
    elif kind == "list":
        out.append(7)
        out += args[1].to_bytes(8, "big")
        _kind(out, types, args[0])
    else:
        raise Refusal(f"{ty.render()} has no record kind")


def _members_kind(out, types, members):
    _count(out, len(members))
    for name, member in members:
        _text(out, name)
        _kind(out, types, Ty("unit") if member is None else member)


def lower(model: Signatures) -> bytes:
    """The record's canonical bytes, framed exactly as the shell registry's `canonical_bytes`."""
    report = check(model)
    if report["status"] != "pass-with-scope":
        raise Refusal(f"Lowering needs a passing check; {report['obligation']}: {report['reason']}")
    return _record_bytes(model)


def identity(model: Signatures) -> str:
    """SHA-256 of the lowered record, as the shell registry's `identity` computes it."""
    return hashlib.sha256(lower(model)).hexdigest()


def _record_bytes(model):
    """Frame a model that passed every check: tag, kinds, shape, capability, footprint, laws, V2, then laws."""
    out = bytearray()
    _text(out, model.schema)
    _count(out, len(model.ports))
    for port in model.ports:
        operation, footprint = model.lookup("operation", port.name), model.lookup("footprint", port.name)
        out.append(port.tag)
        out.append(5)
        _members_kind(out, model.types, operation.arguments)
        out.append(model.lookup("answer", operation.answer).number)
        _kind(out, model.types, operation.payload)
        out.append(model.lookup("capability", footprint.capability).number)
        for access in (footprint.reads, footprint.writes):
            out.append(sum(model.lookup("resource", r).bit for r in access))
        _count(out, len(footprint.laws))
        for law in footprint.laws:
            out += law.to_bytes(2, "big")
        if footprint.v2[0] == "calls":
            out.append(0)
            _count(out, len(footprint.v2[1]))
            for call in footprint.v2[1]:
                _text(out, call)
        elif footprint.v2[0] == "external":
            out.append(1)
            _text(out, footprint.v2[1])
        else:
            out.append(2)
    _count(out, len(model.laws))
    for law in model.laws:
        out += law.id.to_bytes(2, "big")
        _text(out, law.name)
        _text(out, law.statement)
    return bytes(out)
