"""Effect signatures: the shipped ports lower byte for byte to the shell registry, checks, surfaces and help."""

import hashlib
import json
import os
from pathlib import Path
import random
import re
import subprocess
import unittest

import adt
import effects
import effects_guide as guide
import typed2
from behavior import Refusal


ZAL = Path(__file__).resolve().parent
ROOT = Path(os.environ.get("ZENO_FCIS_ROOT", str(ZAL.parents[1]))).resolve()
EXAMPLE = (ZAL / "examples/shell_effects.zal").read_text()
TYPED = (ZAL / "examples/relay_typed.zal").read_text()
# The shell library's pinned registry identity; the native printer must report the same.
PINNED = "f97591cd12b68199e9bb6d4b27a2aab878b5e70f4507388a43fd9fb752cbc872"


def native():
    """The live Rust registry, from the printer built from this tree; a stale build is refused."""
    target = Path(os.environ.get("CARGO_TARGET_DIR", str(ROOT / "target")))
    target = target if target.is_absolute() else ROOT / target
    executable = Path(os.environ.get("ZAL_EFFECTS_REGISTRY", str(target / "debug/examples/shell_effects_registry")))
    if not executable.is_file():
        raise AssertionError("Build the registry printer with check_effects.py native-build")
    result = subprocess.run([str(executable)], capture_output=True, text=True, timeout=60)
    if result.returncode:
        raise AssertionError(result.stderr)
    data = json.loads(result.stdout)
    source = hashlib.sha256((ROOT / "crates/zeno-fcis-shell/src/effects/registry.rs").read_bytes()).hexdigest()
    if data["registry_source_sha256"] != source:
        raise AssertionError("The registry printer was built from another registry.rs; rerun check_effects.py native-build")
    if data["authority"] != "none":
        raise AssertionError("printer authority changed")
    return data


def snake(name):
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def edited(*edits):
    text = EXAMPLE
    for old, new in edits:
        if text.count(old) != 1:
            raise AssertionError("edit anchor drift: " + old)
        text = text.replace(old, new)
    return effects.parse(text)


def declarations(model):
    """What the record bytes only number: names, tags, capability allowances, answer cases, codes and faults."""
    bits = {r.name: r.bit for r in model.resources}
    cases = lambda name: [c for c, _ in model.types.by_name[name].body] if name in model.types.by_name else None
    return {"ports": [[p.name, p.tag] for p in model.ports], "resources": [[r.name, r.bit] for r in model.resources],
            "capabilities": [[c.name, c.number, c.store, sum(bits[r] for r in c.reads), sum(bits[r] for r in c.writes)]
                             for c in model.capabilities],
            "answers": [[a.name, a.number, [[c, effects.RESULT if p == effects.RESULT else p.render()] for c, p in a.cases]]
                        for a in model.answers],
            "codes": cases("Code"), "faults": cases("Fault")}


def rust_declarations(rust):
    return {"ports": rust["ports"], "resources": rust["resources"], "capabilities": rust["capabilities"],
            "answers": [[n, k, [[snake(c), p] for c, p in cases]] for n, k, cases in rust["answers"]],
            "codes": [snake(c) for c in rust["codes"]], "faults": [snake(c) for c in rust["faults"]]}


def first_difference(left, right):
    index = next((i for i, (a, b) in enumerate(zip(left, right)) if a != b), min(len(left), len(right)))
    return f"first difference at byte {index} of {len(left)} and {len(right)}"


def canonical_source(text):
    return "\n".join(line for line in text.splitlines() if line.strip() and not line.startswith("#")) + "\n"


class ShellRegistryTests(unittest.TestCase):
    """The headline: ZAL is the readable source of the Rust registry, and any drift fails here."""

    def test_shipped_ports_lower_byte_for_byte(self):
        model = effects.parse(EXAMPLE)
        report = effects.check(model)
        self.assertEqual(report["status"], "pass-with-scope", report.get("reason"))
        rust = native()
        self.assertEqual((rust["check"], rust["complete"], rust["schema"]), ("Ok(())", True, effects.SCHEMA))
        record = effects.lower(model)
        expected = bytes.fromhex(rust["canonical"])
        self.assertTrue(record == expected, "the ZAL source and the Rust registry differ: " + first_difference(record, expected))
        self.assertEqual(effects.identity(model), rust["identity"])
        self.assertEqual(rust["identity"], PINNED)
        english = effects.parse(model.render(True), "english")
        self.assertEqual(effects.lower(english), record)
        print(f"SHELL EFFECTS: {len(model.ports)} ports, {len(record)} record bytes equal to the Rust registry, "
              f"identity {rust['identity'][:8]}")

    def test_declarations_match_the_rust_registry(self):
        """Names, numbering, capability allowances and answer cases, which the record bytes only number."""
        model, rust = effects.parse(EXAMPLE), native()
        mine, theirs = declarations(model), rust_declarations(rust)
        for key in theirs:
            with self.subTest(declaration=key):
                self.assertEqual(mine[key], theirs[key])

    def test_any_drift_is_caught(self):
        """Every single edit fails a check, lowers to other bytes, or declares other names than the Rust registry."""
        rust = native()
        expected, theirs = bytes.fromhex(rust["canonical"]), rust_declarations(rust)
        drifts = [("type Envelope = bytes ..=1048576;", "type Envelope = bytes ..=1048575;"),
                  ("lane: int 0..=1,", "lane: int 0..=2,"),
                  ("type Status = committed | replay;", "type Status = replay | committed;"),
                  ("{version: Version, cap: Any}", "{cap: Any, version: Version}"),
                  ("laws [1, 2]", "laws [2, 1]"), ("V2SqliteShell::commit_at,", "V2SqliteShell::commit_at2,"),
                  ("applies once and", "applies once, and"), ("resource jitter = 16;", "resource jitter = 32;"),
                  ("Reopen = 7,", "Reopen = 11,"), ("v2 external relay::export_line", "v2 opt_in"),
                  ("capability Clock = 5: reads {clock}, writes {};", "capability Clock = 5: reads {clock}, writes {clock};"),
                  ("-> Transport Report;", "-> Outcome Report;"),
                  ("footprint Reopen: capability OpenStore, reads {durable}", "footprint Reopen: capability OpenStore, reads {}"),
                  ("| pre_state |", "| prestate |"), ("type Fault = busy | full | io;", "type Fault = busy | io;"),
                  ("capability Relay = 2: store,", "capability Relay = 2:")]
        caught = {"check": 0, "bytes": 0, "declarations": 0}
        for old, new in drifts:
            with self.subTest(drift=new):
                model = edited((old, new))
                if effects.check(model)["status"] != "pass-with-scope":
                    caught["check"] += 1
                elif effects.lower(model) != expected:
                    caught["bytes"] += 1
                else:
                    self.assertNotEqual(declarations(model), theirs, "drift not caught")
                    caught["declarations"] += 1
        print(f"SHELL EFFECTS DRIFT: {len(drifts)} single edits caught: {caught}")

    def test_round_trip_holds_for_every_edited_variant(self):
        """Both surfaces round trip for passing and failing variants alike; only parsing may refuse."""
        edits = [line.strip() for line in EXAMPLE.splitlines() if line.startswith(("footprint", "operation", "capability"))]
        variants = 0
        for line in edits:
            for old, new in [("laws [", "laws [1, "), ("v2 calls [", "v2 calls []#"), ("writes {", "writes {clock, "),
                             ("reads {", "reads {}#"), ("-> Outcome ", "-> Fallible "), ("(store: Store", "(")]:
                if old not in line:
                    continue
                changed = line.replace(old, new, 1)
                changed = re.sub(r"\[\]#[^\]]*\]", "[]", re.sub(r"\{\}#[^}]*\}", "{}", changed))
                try:
                    model = effects.parse(EXAMPLE.replace(line, changed))
                except Refusal:
                    continue
                english = model.render(True)
                self.assertEqual(effects.parse(english, "english"), model, changed)
                self.assertEqual(effects.parse(model.render()), model, changed)
                variants += 1
        self.assertGreater(variants, 40)
        print(f"EFFECTS ROUND TRIP: {variants} edited variants in both surfaces")

    def test_example_is_canonical_and_both_surfaces_agree(self):
        model = effects.parse(EXAMPLE)
        self.assertEqual(model.render(), canonical_source(EXAMPLE))
        english = model.render(True)
        self.assertEqual(effects.parse(english, "english"), model)
        self.assertEqual(effects.parse(english, "english").render(True), english)
        lines = canonical_source(EXAMPLE).splitlines()
        rng = random.Random(20261019)
        for _ in range(20):
            rng.shuffle(lines)
            self.assertEqual(effects.parse("\n".join(lines)).revision, model.revision)


class CheckTests(unittest.TestCase):
    def failed(self, model, obligation, reason):
        report = effects.check(model)
        self.assertEqual((report["status"], report.get("obligation")), ("fail", obligation), report.get("reason"))
        self.assertIn(reason, report["reason"])
        with self.assertRaises(Refusal):
            effects.lower(model)

    def test_missing_port_fails(self):
        operation = [line for line in EXAMPLE.splitlines() if line.startswith(("operation Acknowledge", "footprint Acknowledge"))]
        self.failed(edited(*[(line + "\n", "") for line in operation]), "port_coverage", "port Acknowledge has no operation")
        self.failed(edited((operation[1] + "\n", "")), "port_coverage", "port Acknowledge has no footprint")
        self.failed(edited(("Wait = 10;", "Wait = 10, Spare = 11;")), "port_coverage", "port Spare has no operation")
        model = effects.parse(EXAMPLE.replace("Clock = 8, ", ""))
        self.failed(model, "port_coverage", "operation Clock names no declared port")
        with self.assertRaises(Refusal):
            effects.parse(EXAMPLE.replace("Wait = 10;", "Wait = 9;"))

    def test_frame_must_cover_every_case(self):
        self.failed(edited((", unknown either;\nframe Transport", ";\nframe Transport")), "frames", "frame Outcome misses case unknown")
        self.failed(edited(("frame Fallible: ok applied, err unchanged;", "frame Fallible: ok applied, err unchanged, lost either;")),
                    "frames", "frame Fallible names lost, which is not a case of Fallible")
        self.failed(edited(("frame Fallible: ok applied, err unchanged;", "frame Fallible: ok applied, err unchanged, ok either;")),
                    "frames", "frame Fallible gives case ok twice")
        self.failed(edited(("frame Fallible: ok applied, err unchanged;\n", "")), "frames", "answer Fallible has no frame")
        self.failed(edited(("answer Outcome = 0: ok(T) | refused(Code) | not_applied(Fault) | unknown(Fault);",
                            "answer Outcome = 0: ok(T) | refused(Code) | not_applied(Fault);")),
                    "frames", "frame Outcome names unknown, which is not a case of Outcome")

    def test_footprint_beyond_capability_fails(self):
        self.failed(edited(("footprint ReadState: capability ReadStore, reads {durable, volatile}, writes {volatile}",
                            "footprint ReadState: capability ReadStore, reads {durable, volatile}, writes {durable, volatile}")),
                    "footprints", "operation ReadState writes durable, beyond capability ReadStore")
        self.failed(edited(("footprint Send: capability Transport, reads {transport}",
                            "footprint Send: capability Transport, reads {durable, transport}")),
                    "footprints", "operation Send reads durable, beyond capability Transport")
        with self.assertRaises(Refusal):
            edited(("reads {jitter}, writes {jitter}, laws [9]", "reads {entropy}, writes {jitter}, laws [9]"))

    def test_unknown_law_fails(self):
        self.failed(edited(("laws [1, 2]", "laws [1, 99]")), "laws", "operation Commit names law 99, which does not exist")
        self.failed(edited(("laws [1, 2]", "laws [1, 1, 2]")), "laws", "operation Commit names law 1 twice")
        self.failed(edited(("laws [9]", "laws []")), "laws", "operation Random names no law")
        self.failed(edited(("laws [9]", "laws [8]")), "laws", "law 9 jitter-in-range is named by no operation")

    def test_v2_mapping_fails_for_unmapped_store_ports(self):
        self.failed(edited(("laws [3], v2 calls [V2SqliteShell::snapshot]", "laws [3], v2 opt_in")),
                    "v2_mapping", "store operation ReadState must map to V2 calls")
        self.failed(edited(("laws [7], v2 calls [V2SqliteShell::open, V2SqliteShell::audit]", "laws [7], v2 calls []")),
                    "v2_mapping", "operation Reopen maps to an empty list of V2 calls")
        self.assertEqual(effects.check(edited(("v2 external relay::export_line", "v2 opt_in")))["status"], "pass-with-scope")

    def test_record_kinds_and_schema(self):
        self.failed(edited(("operation Clock()", "operation Clock(flag: bool)")), "record_kinds", "bool has no record kind")
        self.failed(edited(("type Hash = bytes 32;", "type Hash = bytes 16;")), "record_kinds", "exactly 16 bytes has no record kind")
        self.failed(edited(("lane: int 0..=1,", "lane: int -1..=1,")), "record_kinds", "is signed")
        nested = "".join(f"type N{i} = {{x: N{i + 1}}};\n" for i in range(7)) + "type N7 = {x: Hash};\n"
        self.failed(effects.parse(EXAMPLE.replace("operation Clock()", "operation Clock(deep: N0)") + nested),
                    "record_kinds", "nests deeper than 8 levels")
        self.failed(edited(("schema zeno-fcis/shell-effects/1;", "schema zeno-fcis/shell-effects/2;")),
                    "record_schema", "lowers only to zeno-fcis/shell-effects/1")

    def test_parse_refusals_are_named(self):
        cases = [(("ports ReadState = 1, Commit = 2,", "ports ReadState = 1, Commit = 1,"), "Ports need distinct names and distinct tags"),
                 (("type Purpose = retry_jitter;", "type Purpose = retry_jitter | retry_jitter;"), "Variant Purpose repeats case"),
                 (("type Tick = {epoch: Any, tick: Any};", "type Tick = {epoch: Any, epoch: Any};"), "Record Tick repeats field"),
                 (("type Bounded = {version: Version, cap: Any};", "type Bounded = {version: Version, cap: Mode};"), "recursive"),
                 (("capability Jitter = 6:", "capability Jitter = 8:"), "numbered 0, 1, 2"),
                 (("answer Fallible = 2: ok(T) | err(Fault);", "answer Fallible = 2: ok(T) | err(T);"), "exactly one case carrying"),
                 (("type Hash = bytes 32;", "type Hash = bytes 32;\ntype Hash = bytes 32;"), "Type Hash is declared twice"),
                 (("type Any = int", "type Store = int 0..=1;\ntype Any = int"), "Type Store is declared twice"),
                 (("resource clock = 8;", "resource clock = 6;"), "one-bit values")]
        for (old, new), message in cases:
            with self.subTest(message=message):
                with self.assertRaises(Refusal) as caught:
                    edited((old, new))
                self.assertIn(message, str(caught.exception))
        with self.assertRaises(Refusal):
            effects.parse(EXAMPLE + "operation ReadState(store: Store) -> Outcome Snapshot;\n")


SLOTS = {"NAME": ["probe"], "SCHEMA": [effects.SCHEMA], "BIT": ["32"], "CAPABILITY": ["Probe"], "NUMBER": ["7"],
         "ANSWER": ["Probe"], "CASE": ["first", "second"], "TYPE": ["Hash", "Status"], "PORT": ["Probe", "Spare"],
         "TAG": ["11", "12"], "ID": ["10"], "LAW": ["probe-law"], "STATEMENT": ["a probe statement"],
         "FIELD": ["left", "right"], "CALL": ["Probe::call"], "LOW": ["-2"], "HIGH": ["5"], "CAP": ["3"], "LENGTH": ["32"],
         "VALUE": ["false"]}
OVERRIDES = {"capability": {"NAME": ["durable", "volatile"]}, "footprint": {"NAME": ["durable", "volatile"],
             "CAPABILITY": ["ReadStore"], "ID": ["3"]}, "answer": {"NUMBER": ["3"]}, "frame": {"ANSWER": ["Fallible"],
             "CASE": ["ok", "err"]}, "operation": {"ANSWER": ["Outcome"]}, "ports": {"PORT": ["ReadState", "Commit"],
             "TAG": ["1", "2"]}, "recursion": {"NAME": ["Probe"], "CASE": ["stop", "more"]},
             "typed_compare": {"NAME": ["answer"], "VALUE": ["sent"]}, "record": {"NAME": ["Probe"]},
             "variant": {"NAME": ["Probe"]}, "alias": {"NAME": ["Probe"]}, "type": {"NAME": ["Probe"]},
             "typed_var": {"TYPE": ["Sent"], "VALUE": ["unknown"]}, "typed_input": {"TYPE": ["Sent"]}}


def fill(construct, form):
    """Replace each slot occurrence by the next value of that slot, in order of appearance."""
    table = {**SLOTS, **OVERRIDES.get(construct.id, {})}
    used = {}
    def value(match):
        slot = match.group(0)
        index = used.get(slot, 0)
        used[slot] = index + 1
        return table[slot][min(index, len(table[slot]) - 1)]
    return re.sub(r"\b(" + "|".join(sorted(table, key=len, reverse=True)) + r")\b", value, form)


class GuideTests(unittest.TestCase):
    def test_every_construct_has_help(self):
        model = effects.parse(EXAMPLE)
        ids = set(guide.BY_ID)
        self.assertLessEqual(set(effects.statement_patterns()) | set(effects.statement_patterns(True)), ids)
        self.assertLessEqual({e["concept"] for e in guide.legend(model)["entries"]}, ids)
        self.assertLessEqual(set(adt.PRIMITIVES) | {"alias", "record", "variant"}, ids)
        self.assertLessEqual({"typed_var", "typed_input", "typed_header", "typed_compare"}, ids)
        for construct in guide.REGISTRY:
            with self.subTest(construct=construct.id):
                self.assertTrue(construct.meaning and construct.title and construct.symbolic and construct.english)
                result = guide.guide(model, "grammar:" + construct.id, "test")
                self.assertEqual((result["construct"], result["meaning"], result["schema"]),
                                 (construct.id, construct.meaning, "zal/help/2"))
        print(f"EFFECTS HELP DRIFT: {len(guide.REGISTRY)} constructs cover every effects, type and typed-machine statement")

    def accept(self, parse, *arguments):
        try:
            return parse(*arguments)
        except Refusal as error:
            self.fail(f"the parser refuses a registry form: {error}")

    def test_every_form_parses_as_its_construct(self):
        """Fill every registry form; the live parsers accept both surfaces as the same construct."""
        base = effects.parse(EXAMPLE)
        typed = typed2.parse(TYPED)
        for construct in guide.REGISTRY:
            symbolic, english = fill(construct, construct.symbolic), fill(construct, construct.english)
            with self.subTest(construct=construct.id, symbolic=symbolic, english=english):
                if construct.id in effects.statement_patterns():
                    self.assertTrue(re.fullmatch(effects.statement_patterns()[construct.id], symbolic))
                    self.assertTrue(re.fullmatch(effects.statement_patterns(True)[construct.id], english))
                    extra = {"frame": ("frame Fallible: ok applied, err unchanged;",
                                       "For answer Fallible, ok means the operation took effect, err means nothing changed.")}
                    pair = []
                    for surface, line in [(False, symbolic), (True, english)]:
                        lines = [x for x in base.render(surface).splitlines() if x != extra.get(construct.id, ("", ""))[surface]]
                        concept = [c for c, _ in effects.concept_lines(base, surface)]
                        if construct.id in effects.SINGLETONS:
                            lines = [line if c == construct.id else x for c, x in zip(concept, lines)]
                        else:
                            lines.append(line)
                        pair.append(self.accept(effects.parse, "\n".join(lines), "english" if surface else "symbolic"))
                    self.assertEqual(pair[0], pair[1])
                    self.assertIn(symbolic, pair[0].render().splitlines())
                elif construct.kind == "type" or construct.id == "recursion":
                    if construct.symbolic.startswith("type "):
                        decls = []
                        for surface, line in [(False, symbolic), (True, english)]:
                            match = re.fullmatch(adt.type_patterns(surface), line)
                            decls.append(self.accept(adt.parse_decl, match[1], match[2], surface))
                        self.assertEqual(decls[0], decls[1])
                        self.assertEqual(decls[0].form, construct.id if construct.id != "recursion" else "variant")
                        if construct.id == "recursion":
                            with self.assertRaises(Refusal) as caught:
                                adt.Types((decls[0],))
                            self.assertIn("recursive", str(caught.exception))
                    else:
                        parsed = []
                        for surface, text in [(False, symbolic), (True, english)]:
                            tokens = adt.Tokens(text, surface)
                            parsed.append(self.accept(adt.parse_ty, tokens))
                            self.accept(tokens.done)
                        self.assertEqual(parsed[0], parsed[1])
                        expected = "ref" if construct.id == "store" else construct.id
                        self.assertEqual(parsed[0].kind, expected)
                else:
                    pair = []
                    for surface, line in [(False, symbolic), (True, english)]:
                        lines = typed.render(surface).splitlines()
                        if construct.id == "typed_header":
                            lines[0] = line
                        elif construct.id == "typed_compare":
                            lines.append(f"rule probe_rule: pending + reply [{line}] -> accept pending;" if not surface else
                                         f"Rule probe_rule: when the state is pending and the event is reply, if {line}, "
                                         "move to pending.")
                        else:
                            lines.append(line)
                        pair.append(self.accept(typed2.parse, "\n".join(lines), "english" if surface else "symbolic"))
                    self.assertEqual(pair[0], pair[1])
                    if construct.id in {"typed_var", "typed_input"}:
                        self.assertIn(symbolic, pair[0].render().splitlines())

    def test_guide_resolves_objects_and_refuses_unknown_topics(self):
        model = effects.parse(EXAMPLE)
        result = guide.guide(model, "Commit")
        self.assertEqual((result["topic"], result["construct"]), ("port:Commit", "operation"))
        self.assertEqual([e["symbolic"].split()[0] for e in result["entries"]], ["operation", "footprint"])
        result = guide.guide(model, "Mode")
        self.assertEqual((result["topic"], result["cardinality"]["text"]), ("type:Mode", "2^127 + 2^63 + 1"))
        self.assertEqual(guide.guide(model, "exhaustiveness")["construct"], "frame")
        self.assertEqual(len(guide.guide(model)["constructs"]), len(guide.REGISTRY))
        for topic in ["Clock", "Transport", "nope", "grammar:nope"]:
            with self.assertRaises(Refusal):
                guide.guide(model, topic)
        self.assertEqual(guide.guide(model, "capability:Clock")["construct"], "capability")

    def test_explain_port_and_type(self):
        model = effects.parse(EXAMPLE)
        result = guide.explain(model, "port:Commit")
        self.assertEqual((result["tag"], result["within_capability"], result["record"]["identity"]), (2, True, PINNED))
        self.assertEqual([law["name"] for law in result["laws"]], ["commit-once", "commit-atomic"])
        self.assertEqual([(c["case"], c["frame"]) for c in result["answer"]["cases"]],
                         [("ok", "applied"), ("refused", "unchanged"), ("not_applied", "unchanged"), ("unknown", "either")])
        self.assertEqual(result["answer"]["cases"][0]["cardinality"]["text"], "2^320")
        exceeded = edited(("footprint ReadState: capability ReadStore, reads {durable, volatile}, writes {volatile}",
                           "footprint ReadState: capability ReadStore, reads {durable, volatile}, writes {durable, volatile}"))
        result = guide.explain(exceeded, "port:ReadState")
        self.assertEqual((result["within_capability"], result["check_failure"]["obligation"]), (False, "footprints"))
        self.assertIn("exceeds capability ReadStore", guide.human_text(result))
        self.assertEqual(guide.explain(model, "type:Snapshot")["operator"], "product")
        with self.assertRaises(Refusal):
            guide.explain(model, "port:Nope")


if __name__ == "__main__":
    unittest.main()
