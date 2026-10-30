"""Exact numeric meaning across the ZAL JSON readers: planted lexemes and a differential check.

Every reader that ingests ZAL JSON either keeps a number's exact decimal value or refuses it with a named
`json_numbers` reason. Accepted numbers agree three ways: the source lexeme, the in-memory model and its canonical
JSON output denote the same decimal. Readers that do not use the policy are listed below with the reason, so a new
reader cannot be added silently.
"""
from __future__ import annotations

import ast
from decimal import Decimal
import importlib.util
import json
import math
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(ROOT))

import json_numbers as N  # noqa: E402
import app_profile  # noqa: E402
import ladder  # noqa: E402
import loop_learn  # noqa: E402
import loop_strategy_modes  # noqa: E402
import loop_versions  # noqa: E402
import loop_workspace  # noqa: E402
import observations  # noqa: E402
import program_synthesis  # noqa: E402
import relational_observation  # noqa: E402
import report_numbers  # noqa: E402
import semantic_library  # noqa: E402
import terminal  # noqa: E402
import word_kernel  # noqa: E402
from finite_library import prototype  # noqa: E402
from zal_notation import core, normalizer  # noqa: E402

# Other modules put directories holding their own run.py first on sys.path, so load this CLI module by path.
_spec = importlib.util.spec_from_file_location("zal_run_cli", HERE / "run.py")
zal_run = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(zal_run)

SAFE = (1 << 53) - 1
ACCEPTED = ["0", "1", "-1", "9007199254740993", str(1 << 64), "123456789012345678901234567890", "1.0", "0.1",
            "1.50", "1e2", "1E2", "1e-07", "-0.0", "5e-324", "1.7976931348623157e308"]
REFUSED = {"-0": N.NEGATIVE_ZERO, "0.30000000000000001": N.INEXACT, "1e-400": N.INEXACT, "-1e-400": N.INEXACT,
           "1e400": N.NONFINITE, "-1e400": N.NONFINITE, "NaN": N.NONFINITE, "Infinity": N.NONFINITE,
           "-Infinity": N.NONFINITE, "9" * 4301: N.TOO_LONG, "1." + "0" * 4300: N.TOO_LONG}
MALFORMED = ["01", "+1", ".5", "1.", "0x10", "1e", "--1"]


def integral(lexeme):
    return not any(mark in lexeme for mark in ".eE")


def wrap(lexeme):
    return '{"v":' + lexeme + '}'


def file_reader(read):
    def reader(text):
        with tempfile.TemporaryDirectory(prefix="zal-json-numbers-") as folder:
            path = Path(folder) / "value.json"
            path.write_text(text, encoding="utf-8")
            return read(path)
    return reader


def contract_reader(text, read):
    """A contract directory whose v2/policy.json is the given text."""
    with tempfile.TemporaryDirectory(prefix="zal-json-numbers-contract-") as folder:
        root = Path(folder)
        (root / "v2").mkdir()
        (root / "project.zeno").write_text("", encoding="ascii")
        (root / "v2" / "policy.json").write_text(text, encoding="ascii")
        return read(root)


# Readers that return the parsed document, and keep every number whose exact value they can represent.
FULL = {
    "zal_notation.core.loads": lambda text: core.loads(text)["v"],
    "semantic_library.load": lambda text: semantic_library.load(text)["v"],
    "finite_library.prototype hooks": lambda text: json.loads(text, **prototype._number_hooks())["v"],
    "ladder hooks": lambda text: json.loads(text, **ladder._number_hooks())["v"],
    "loop_learn.strict_json": lambda text: loop_learn.strict_json(text)["v"],
    "run.user_json": lambda text: zal_run.user_json(text)["v"],
    "terminal.trace_inputs": lambda text: terminal.trace_inputs("[" + text + "]")[0]["v"],
}
# Readers that admit integers only; fraction lexemes keep each reader's own refusal.
INTEGER = {
    "loop_versions.strict_json": lambda text: loop_versions.strict_json(text)["v"],
    "observations.decode": lambda text: observations.decode(text)["v"],
    "relational_observation.load_profile": file_reader(lambda path: relational_observation.load_profile(path)[0]["v"]),
    "word_kernel.read_json": file_reader(lambda path: word_kernel.read_json(path)["v"]),
}
# Readers whose structure checks follow parsing; only their number refusals are observable here.
REFUSING = {
    "app_profile._parse_binding": lambda text: app_profile._parse_binding(None, text),
    "program_synthesis._binding": lambda text: program_synthesis._binding(text.encode(), None),
    "loop_strategy_modes.strict_problem": lambda text: loop_strategy_modes.strict_problem(text),
    "finite_library.prototype.loads": lambda text: prototype.loads(text),
    "ladder.load": file_reader(ladder.load),
    "ladder.file_theory": file_reader(ladder.file_theory),
    "loop_workspace.fields_of": lambda text: contract_reader(text, loop_workspace.fields_of),
}

# Every other json.load(s) call under integrations/zal, by "path:function", with why it does not use the policy.
UNCOVERED = {
    # Open: these ingest outside JSON and still use plain number parsing.
    "codex_transport.py:AppServer._read": "open: local agent app-server transport; a written plan exists",
    "category_profile/adapter.py:_capture": "own protocol: pinned category capsule",
    "category_profile/provider/build.py:main": "own protocol: pinned category capsule",
    "category_profile/provider/main.py:<module>": "own protocol: pinned category capsule",
    "category_profile/provider/profile.py:prepare_request": "own protocol: pinned category capsule",
    "category_profile/provider/profile.py:captured": "own protocol: pinned category capsule",
    "category_profile/provider/profile.py:binding": "own protocol: pinned category capsule",
    "category_profile/provider/profile.py:execute": "own protocol: pinned category capsule",
    "category_profile/provider/profile.py:main": "own protocol: pinned category capsule",
    "relational_profile/bridge.py:_scope_identity": "own exact handling: decimal tokens kept as typed strings",
    "relational_profile/bridge.py:_context_descriptor": "internal: compares the bundled registry",
    "relational_profile/profile.py:_load_manifest": "own protocol: pinned relational manifest",
    "run.py:ladder_json_refusal": "routing only: the ladder tool reads the file again",
    "zal_notation/syntax.py:parse_text.scan": "decodes a JSON string literal; no number can occur",
    "loop_doors.py:family_instances": "repository data under core-components",
    "ladder.py:<module>": "repository data: the bundled ladder registry",
    # Internal: copies or re-reads of data this process already holds or wrote in canonical form.
    "loop_learn.py:specification": "internal copy", "loop_learn.py:view": "internal copy",
    "loop_versions.py:handle": "internal copy", "observation_diagrams.py:render": "internal copy",
    "observation_diagrams.py:_observation_label": "internal: labels written by the adapter",
    "server.py:Workspace.view": "internal copy", "server.py:Workspace.notify": "internal copy",
    "workflow.py:owned_examples": "internal copy", "workflow.py:Session.__init__": "internal copy",
    "workflow.py:Session.example_comparison": "internal copy", "workflow.py:Session.record": "internal copy",
    "workflow.py:Session.view": "internal copy", "zal_notation/lenses.py:check": "internal: labels written by the projection",
    "zal_notation/lenses.py:SourceSnapshot.context": "internal copy", "zal_notation/normalizer.py:alpha_normalize": "internal copy",
    "app_profile.py:lower": "own output: source map written by the factory",
    "app_profile.py:main": "own output: source map written by the factory",
    "capture_workspace.py:CaptureWorkspace.load": "own output: persisted workspace",
    "loop_workspace.py:LoopWorkspace.load": "own output: persisted workspace",
    "loop_workspace.py:LoopWorkspace.init": "own output: workspace manifest and instances",
    "shared.py:SharedWorkspace.load": "own output: persisted workspace",
    "withdrawal_pilot.py:load_graph": "checked tool output: the native graph CLI",
    "check_lowering.py:run": "checked tool output", "factory.py:refine_check": "checked tool output",
    "loop_coverage.py:Judge.review": "checked tool output", "loop_doors.py:run": "checked tool output",
    "probe_codex.py:probe": "checked tool output", "withdrawal_pilot.py:native": "checked tool output",
    "check_loop_workspace.py:historical_incomplete": "developer check tool",
    "check_loop_workspace.py:drift_workspace": "developer check tool",
    "check_withdrawal_pilot.py:bind_source": "developer check tool", "check_withdrawal_pilot.py:planted": "developer check tool",
    "setup.py:claude_config": "developer setup tool",
}


def json_reads(path):
    """(function, covered) for each json.load(s) call; covered means it passes the policy's integer hook."""
    found = []

    def walk(node, where):
        for child in ast.iter_child_nodes(node):
            inner = where
            if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                inner = child.name if where == "<module>" else where + "." + child.name
            if (isinstance(child, ast.Call) and isinstance(child.func, ast.Attribute) and child.func.attr in ("load", "loads")
                    and isinstance(child.func.value, ast.Name) and child.func.value.id == "json"):
                # zal_notation/core.py and finite_library/prototype.py write the same policy inline as
                # _number_hooks(), because the category capsule pins their import closure.
                covered = any(k.arg == "parse_int" or (k.arg is None and ast.unparse(k.value).split("(")[0] in
                                                       ("json_numbers.hooks", "json_numbers.integer_only", "_number_hooks"))
                              for k in child.keywords)
                found.append((where, covered))
            walk(child, inner)
    walk(ast.parse(path.read_text(encoding="utf-8")), "<module>")
    return found


class Policy(unittest.TestCase):
    def loads(self, lexeme):
        return json.loads(wrap(lexeme), **N.hooks(N.raising(ValueError)))["v"]

    def test_accepted_numbers_keep_their_exact_decimal_value(self):
        for lexeme in ACCEPTED:
            with self.subTest(lexeme=lexeme):
                value = self.loads(lexeme)
                self.assertIs(type(value), int if integral(lexeme) else float)
                self.assertEqual(Decimal(json.dumps(value)), Decimal(lexeme))
        self.assertEqual(math.copysign(1, self.loads("-0.0")), -1.0)

    def test_planted_lexemes_refuse_with_their_reason(self):
        for lexeme, reason in REFUSED.items():
            with self.subTest(lexeme=lexeme[:20]), self.assertRaisesRegex(ValueError, "^" + reason + ": "):
                self.loads(lexeme)
        for lexeme in MALFORMED:
            with self.subTest(lexeme=lexeme), self.assertRaises(json.JSONDecodeError):
                self.loads(lexeme)


class IntegerOnly(unittest.TestCase):
    def loads(self, lexeme):
        return json.loads(wrap(lexeme), **N.integer_only(N.raising(ValueError), N.I128))["v"]

    def test_contract_policy_numbers_match_the_rust_rules_reader(self):
        for lexeme in ("0", "-1", str((1 << 127) - 1), str(-(1 << 127))):
            self.assertEqual(self.loads(lexeme), int(lexeme))
        cases = {"-0": N.NEGATIVE_ZERO, "1.0": N.NOT_INTEGER, "1e2": N.NOT_INTEGER, "0.5": N.NOT_INTEGER,
                 str(1 << 127): N.OUT_OF_RANGE, str(-(1 << 127) - 1): N.OUT_OF_RANGE, "NaN": N.NONFINITE,
                 "9" * 4301: N.TOO_LONG}
        for lexeme, reason in cases.items():
            with self.subTest(lexeme=lexeme[:20]), self.assertRaisesRegex(ValueError, "^" + reason + ": "):
                self.loads(lexeme)

    def test_policy_readers_refuse_by_name(self):
        for lexeme, reason in (("1.0", N.NOT_INTEGER), ("-0", N.NEGATIVE_ZERO), (str(1 << 127), N.OUT_OF_RANGE)):
            with self.subTest(lexeme=lexeme), self.assertRaisesRegex(ValueError, "v2/policy.json: " + reason):
                contract_reader(wrap(lexeme), loop_workspace.fields_of)


class Readers(unittest.TestCase):
    def test_full_readers_agree_with_source_and_canonical_form(self):
        for lexeme in ACCEPTED:
            values = {name: read(wrap(lexeme)) for name, read in FULL.items()}
            with self.subTest(lexeme=lexeme):
                first = next(iter(values.values()))
                for name, value in values.items():
                    self.assertIs(type(value), type(first), name)
                    self.assertEqual(json.dumps(value), json.dumps(first), name)
                self.assertEqual(Decimal(json.dumps(first)), Decimal(lexeme))
                token = normalizer.canonical_json(wrap(lexeme))["result"][5:-1]
                self.assertEqual(token, lexeme, "the exact-token reader keeps the source spelling")

    def test_integer_readers_agree_and_keep_their_fraction_refusal(self):
        for lexeme in ACCEPTED:
            for name, read in INTEGER.items():
                with self.subTest(reader=name, lexeme=lexeme):
                    if integral(lexeme):
                        self.assertEqual(read(wrap(lexeme)), int(lexeme))
                    else:
                        with self.assertRaises(ValueError):
                            read(wrap(lexeme))

    def test_every_reader_refuses_planted_lexemes_by_name(self):
        policy = {**dict.fromkeys(FULL, "full"), **dict.fromkeys(INTEGER, "integer"),
                  "app_profile._parse_binding": "full", "program_synthesis._binding": "integer",
                  "finite_library.prototype.loads": "full", "ladder.load": "full", "ladder.file_theory": "full",
                  "loop_workspace.fields_of": "policy",
                  "loop_strategy_modes.strict_problem": "integer"}
        readers = {**FULL, **INTEGER, **REFUSING}
        for lexeme, reason in REFUSED.items():
            for name, read in readers.items():
                # Integer-only readers refuse fractions and NaN/Infinity with their own existing reasons.
                whole = integral(lexeme) and lexeme[-1].isdigit()
                named = policy[name] in ("full", "policy") or whole
                expected = reason
                if policy[name] == "policy" and not whole:
                    # Contract policy readers admit integers only, as the Rust contract reader does.
                    expected = N.NONFINITE if lexeme.lstrip("-") in ("NaN", "Infinity") else N.NOT_INTEGER
                with self.subTest(reader=name, lexeme=lexeme[:20]):
                    with self.assertRaises(ValueError) as caught:
                        read(wrap(lexeme))
                    if named:
                        self.assertIn(expected + ": ", str(caught.exception))
        for lexeme in MALFORMED:
            for name, read in readers.items():
                with self.subTest(reader=name, lexeme=lexeme), self.assertRaises(ValueError):
                    read(wrap(lexeme))

    def test_mcp_requests_refuse_planted_numbers_by_name(self):
        with tempfile.TemporaryDirectory(prefix="zal-json-numbers-mcp-") as folder:
            lines = [json.dumps({"jsonrpc": "2.0", "id": 1, "method": "ping"})[:-1] + ',"params":{"v":' + lexeme + "}}"
                     for lexeme in ("1e400", "-0", "0.30000000000000001")]
            done = subprocess.run([sys.executable, "-B", str(HERE / "mcp.py"), "--workspace", str(Path(folder) / "ws.json")],
                                  input="\n".join(lines) + "\n", capture_output=True, text=True, timeout=60, check=False)
            replies = [json.loads(line) for line in done.stdout.splitlines()]
        self.assertEqual([reply["error"]["message"].split(":")[1].strip() for reply in replies],
                         [N.NONFINITE, N.NEGATIVE_ZERO, N.INEXACT])

    @unittest.skipUnless(shutil.which("node"), "the browser runtime differential needs node")
    def test_browser_runtime_agrees_except_beyond_the_safe_integer_range(self):
        script = ('const fs=require("fs");const rows=JSON.parse(fs.readFileSync(0,"utf8"));'
                  'process.stdout.write(JSON.stringify(rows.map(t=>JSON.stringify(JSON.parse(t)))));')
        done = subprocess.run(["node", "-e", script], input=json.dumps(ACCEPTED), capture_output=True, text=True,
                              timeout=60, check=True)
        diverged = [lexeme for lexeme, shown in zip(ACCEPTED, json.loads(done.stdout)) if Decimal(shown) != Decimal(lexeme)]
        beyond = [lexeme for lexeme in ACCEPTED if integral(lexeme) and abs(int(lexeme)) > SAFE]
        self.assertEqual(diverged, beyond)
        self.assertTrue(all(type(report_numbers.encode(int(lexeme))) is str for lexeme in beyond),
                        "the browser route receives integers beyond 2^53-1 as decimal strings")

    def test_every_json_reader_uses_the_policy_or_is_listed(self):
        uncovered, covered = set(), 0
        for path in sorted(HERE.rglob("*.py")):
            relative = path.relative_to(HERE).as_posix()
            if (path.name.startswith("test_") or "tests" in path.parts or "vendor" in path.parts
                    or "__pycache__" in path.parts or path.name == "json_numbers.py"):
                continue
            for where, ok in json_reads(path):
                covered += ok
                if not ok:
                    uncovered.add(f"{relative}:{where}")
        self.assertEqual(sorted(uncovered - set(UNCOVERED)), [], "a new JSON reader needs the policy or a listed reason")
        self.assertEqual(sorted(set(UNCOVERED) - uncovered), [], "a listed reader no longer exists or now uses the policy")
        self.assertGreaterEqual(covered, 19)


if __name__ == "__main__":
    unittest.main()
