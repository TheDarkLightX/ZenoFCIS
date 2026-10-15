#!/usr/bin/env python3
"""Parser/protocol regression tests; none are Tau or SMT qualification."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


def module(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


importer, runner = module("importer"), module("runner")
SPEC = """type Amount = bv[3].
type Ledger = {available:Amount}.
type Command = {amount:Amount}.
i1:Ledger := in console.
i2:Command := in console.
i3:bv[1] := in console.
o1:Ledger := out console.
o2:bv[1] := out console.
always (i3[t] = {1}:bv[1])
 ? (o1[t].available = i1[t].available + i2[t].amount && o2[t] = {1}:bv[1])
 : (o1[t].available = i1[t].available && o2[t] = {0}:bv[1]).
"""


class ImportTests(unittest.TestCase):
    def test_packaged_helpers_equal_the_standalone_mirrors(self):
        crate = Path(__file__).resolve().parents[2] / "crates/zeno-fcis-cli"
        for name in ("importer.py", "runner.py"):
            self.assertEqual((crate / "tau" / name).read_bytes(),
                             Path(__file__).with_name(name).read_bytes(), name)

    def test_cli_package_contains_both_embedded_helpers(self):
        import subprocess
        root = Path(__file__).resolve().parents[2]
        result = subprocess.run(["cargo", "+1.97.1", "package", "--list", "--locked", "--offline",
                                 "--allow-dirty", "-p", "zeno-fcis-cli"], cwd=root,
                                text=True, capture_output=True, timeout=60, check=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue({"tau/importer.py", "tau/runner.py"} <= set(result.stdout.splitlines()))

    def test_tau_formula_boundary_preserves_smt_limit_and_refuses_oversize(self):
        query = {"id": "boundary", "kind": "planted-control", "expected": "F",
                 "tau": "T" + " " * (runner.MAX_TAU_QUERY - 1),
                 "smt": "(set-logic ALL)\n(assert true)\n(check-sat)\n"}
        bundle = {"schema": runner.SCHEMA, "authority": "none", "queries": [query]}
        self.assertEqual(runner.validated(bundle), [query])
        self.assertEqual(runner.MAX_QUERY, 1 << 20)
        query["tau"] += " "
        with self.assertRaisesRegex(runner.Refused, "argv transport"):
            runner.validated(bundle)

    def test_tau_formula_boundary_starts_a_bounded_dummy_child(self):
        import sys
        # This checks only argv transport, not the Tau grammar or validity.
        formula = "T" + " " * (runner.MAX_TAU_QUERY - 1)
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            output = runner.execute([sys.executable, "-I", "-B", "-c",
                                     "import sys; print(len(sys.argv[1].encode()))",
                                     "valid " + formula], "", root, root / "argv.txt", 5, 128)
            self.assertIsNone(output["failure"])
            self.assertEqual(output["returncode"], 0)
            self.assertEqual(output["output"].strip(), str(runner.MAX_TAU_QUERY + 6))

    def test_exact_modular_translation_and_native_metadata(self):
        parsed = importer.import_spec(SPEC, {}, {})
        policy = json.loads(parsed["rules"])
        self.assertEqual([case["class"] for case in policy["cases"]], ["Accept", "Reject"])
        self.assertIn("+", policy["cases"][0]["post"]["120"])
        self.assertIn("- 8", policy["cases"][0]["post"]["120"])
        self.assertIn("choose", policy["cases"][0]["post"]["120"])
        self.assertEqual(parsed["metadata"]["scope"], "original-width-finite")
        self.assertEqual(parsed["metadata"]["mapping"]["i1.available"]["native"], "pre.100.120")

    def test_modular_subtraction_uses_a_bounded_correction(self):
        parsed = importer.import_spec(SPEC.replace(" + ", " - "), {}, {})
        expression = json.loads(parsed["rules"])["cases"][0]["post"]["120"]
        self.assertIn("+ 8", expression)
        self.assertIn(">=", expression)

    def test_scaling_is_explicit_and_refuses_other_large_constants(self):
        wide = SPEC.replace("bv[3]", "bv[256]")
        with self.assertRaises(importer.Refused):
            importer.import_spec(wide, {}, {})
        self.assertEqual(importer.import_spec(wide, {256: 3}, {})["metadata"]["scope"], "explicitly-scaled")
        self.assertIn("{7}:bv[3]", importer.render(("int", 7, 256, 3), {}, {}, "tau"))
        with self.assertRaises(importer.Refused):
            importer.import_spec(wide.replace("i2[t].amount", "{9}:bv[256]"), {256: 3}, {})

    def test_source_maximum_is_scaled_to_new_maximum(self):
        maximum = "{" + str((1 << 256) - 1) + "}:bv[256]"
        wide = SPEC.replace("bv[3]", "bv[256]").replace("i2[t].amount", maximum)
        expression = json.loads(importer.import_spec(wide, {256: 3}, {})["rules"])["cases"][0]["post"]["120"]
        self.assertIn("(7)", expression)

    def test_original_mixed_width_literals_refuse_before_narrowing(self):
        fixture = Path(__file__).with_name("finite-ladder.tau").read_text()
        bad = [fixture.replace("i1[t].available + i2[t].amount", "i1[t].available + {1}:bv[256]"),
               SPEC.replace("i1[t].available + i2[t].amount", "{1}:bv[256]"),
               SPEC.replace("i3[t] = {1}:bv[1]", "i3[t] = {1}:bv[256]")]
        for source, scaling in [(bad[0], {256: 3}), (bad[1], {256: 3}), (bad[2], {256: 1})]:
            with self.subTest(source=source), self.assertRaisesRegex(importer.Refused, "mixed-width"):
                importer.import_spec(source, scaling, {})

    def test_original_mixed_width_names_refuse_after_effective_widths_collapse(self):
        bad = [SPEC.replace("type Amount = bv[3].", "type Amount = bv[256].")
                   .replace("type Command = {amount:Amount}.", "type Command = {amount:bv[3]}."),
               SPEC.replace("type Command = {amount:Amount}.", "type Command = {amount:bv[256]}."),
               SPEC.replace("type Command = {amount:Amount}.", "type Command = {amount:bv[256]}.")
                   .replace("i3[t] = {1}:bv[1]", "i1[t].available = i2[t].amount")]
        for source in bad:
            with self.subTest(source=source), self.assertRaisesRegex(importer.Refused, "mixed-width"):
                importer.import_spec(source, {256: 3}, {})

    def test_valid_original_types_survive_explicit_narrowing(self):
        baseline = importer.import_spec(SPEC, {}, {})
        uniform = importer.import_spec(SPEC.replace("bv[3]", "bv[256]"), {256: 3}, {})
        self.assertEqual((uniform["project"], uniform["rules"]), (baseline["project"], baseline["rules"]))
        separate = SPEC.replace("type Command = {amount:Amount}.", "type Command = {amount:bv[256]}.")
        separate = separate.replace("i1[t].available + i2[t].amount", "i1[t].available + {1}:bv[3]")
        separate = separate.replace("i3[t] = {1}:bv[1]", "i3[t] = {1}:bv[1] && i2[t].amount = {1}:bv[256]")
        parsed = importer.import_spec(separate, {256: 3}, {})
        self.assertEqual(parsed["metadata"]["scope"], "explicitly-scaled")
        self.assertEqual(parsed["metadata"]["mapping"]["i1.available"]["original_width"], 3)
        self.assertEqual(parsed["metadata"]["mapping"]["i2.amount"]["original_width"], 256)

    def test_guards_and_all_outputs_are_required(self):
        bad = [SPEC.replace(" && o2[t] = {1}:bv[1]", ""),
               SPEC.replace("o2[t] = {1}:bv[1]", "o2[t] = {1}:bv[1] && o2[t] = {1}:bv[1]"),
               SPEC.replace("i3[t] =", "o2[t] ="),
               SPEC.replace("o1[t].available = i1[t].available &&", "o1[t].available = i2[t].amount &&"),
               SPEC.replace("o2[t] = {1}:bv[1]", "o2[t] = i3[t]"),
               SPEC.replace("o1[t].available = i1[t].available +", "o1[t].available = o1[t].available +")]
        for source in bad:
            with self.subTest(source=source), self.assertRaises(importer.Refused):
                importer.import_spec(source, {}, {})

    def test_unsupported_time_stream_control_and_arithmetic_refuse(self):
        bad = [SPEC.replace("i3[t]", "i3[t-1]"), SPEC.replace("i3[t]", "i3[t+1]"),
               SPEC.replace("always", "sometimes"), SPEC.replace(" + ", " * "),
               SPEC.replace("i3:", "i4:"), SPEC.replace("out console", "in console"),
               SPEC + "T.", SPEC.replace("i2[t].amount", "{1}:bv[2]"),
               SPEC.replace("{1}:bv[1]", "{2}:bv[1]"),
               SPEC.replace("i1[t].available + i2[t].amount", "(T ? i1[t].available : i2[t].amount)")]
        for source in bad:
            with self.subTest(source=source), self.assertRaises(importer.Refused):
                importer.import_spec(source, {}, {})

    def test_extra_alias_duplicate_stream_and_nested_records_refuse(self):
        bad = ["type Amount = bv[3].\n" + SPEC,
               SPEC.replace("i2:Command", "i1:Command"),
               SPEC.replace("{available:Amount}", "{available:{nested:Amount}}")]
        for source in bad:
            with self.subTest(source=source), self.assertRaises(importer.Refused):
                importer.import_spec(source, {}, {})

    def test_domain_limits_cannot_redefine_successor_semantics(self):
        with self.assertRaises(importer.Refused):
            importer.import_spec(SPEC, {}, {"i1.available": (0, 3)})
        with self.assertRaises(importer.Refused):
            importer.import_spec(SPEC, {}, {"i3": (1, 1)})
        with self.assertRaises(importer.Refused):
            importer.import_spec(SPEC, {}, {"o2": (0, 1)})
        self.assertEqual(importer.import_spec(SPEC, {}, {"i2.amount": (0, 3)})["metadata"]["mapping"]["i2.amount"]["max"], 3)

    def test_correspondence_binds_parsed_source_and_observation(self):
        metadata = json.loads(json.dumps(importer.import_spec(SPEC, {}, {})["metadata"]))
        # A serialization fixture, never an asserted library decision.
        observation = {"inputs": {"pre.100.120": "7", "command.101.140": "1", "context.102": "1"},
                       "class": "Accept", "post": {"120": "0"}, "outbox": []}
        queries = importer.correspondence(metadata, [observation])
        self.assertEqual([query["expected"] for query in queries], ["T", "F"])
        self.assertIn("{7}:bv[3]", queries[0]["tau"])
        self.assertIn("bvadd", queries[0]["smt"])
        self.assertIn("(_ bv0 3)", queries[0]["smt"])
        damaged = {**metadata, "source_sha256": "0" * 64}
        with self.assertRaises(importer.Refused):
            importer.correspondence(damaged, [observation])
        changed = json.loads(json.dumps(metadata))
        changed["mapping"]["i1.available"]["field"] = 121
        with self.assertRaises(importer.Refused):
            importer.correspondence(changed, [observation])
        with self.assertRaises(importer.Refused):
            importer.correspondence(metadata, [{**observation, "post": {"120": "8"}}])


class AllowanceProfileTests(unittest.TestCase):
    def owner(self):
        return Path(__file__).with_name("allowance-study.tau").read_text()

    def flat(self, width=3, native_codes=False):
        maximum = str((1 << 256) - 1)
        import re
        record = "always" + self.owner().split("always", 1)[1].rstrip().removesuffix(".")
        record = record.replace(maximum, "7").replace("bv[256]", f"bv[{width}]").replace("bv[8]", f"bv[{width}]")
        mapping = dict(zip(["i1[t].available", "i1[t].reserved", "i1[t].spent", "i2[t].kind", "i2[t].amount", "i2[t].actual", "i3[t]", "o1[t].available", "o1[t].reserved", "o1[t].spent", "o2[t]"],
                           ["i1[t]", "i2[t]", "i3[t]", "i4[t]", "i5[t]", "i6[t]", "i7[t]", "o1[t]", "o2[t]", "o3[t]", "o4[t]"]))
        source = re.sub("|".join(re.escape(key) for key in mapping), lambda match: mapping[match[0]], record)
        if native_codes:
            for code in range(4):
                source = source.replace(f"i4[t] = {{{code}}}", f"i4[t] = {{{140 + code}}}")
        return source

    def parse(self, source=None, scaling=None, domains=None):
        return importer.import_spec(self.owner() if source is None else source,
                                    {256: 3} if scaling is None else scaling, domains or {}, importer.ALLOWANCE_PROFILE)

    def test_owner_and_both_flat_layouts_bind_all_roles_and_annotations(self):
        for source, scaling, base in [(self.owner(), {256: 3}, 0), (self.flat(), {}, 0), (self.flat(8, True), {}, 140)]:
            with self.subTest(base=base, scaling=scaling):
                parsed = self.parse(source, scaling)
                policy, metadata = json.loads(parsed["rules"]), parsed["metadata"]
                self.assertEqual(len(policy["cases"]), 11)
                self.assertEqual(sum(case["class"] == "Accept" for case in policy["cases"]), 4)
                self.assertEqual([case["reason"] for case in policy["cases"] if case["class"] == "Reject"], list(range(200, 207)))
                self.assertEqual(len(policy["law_kinds"]), 4)
                self.assertEqual(len(metadata["mapping"]), 7)
                state = [row for row in metadata["mapping"].values() if "output" in row]
                self.assertEqual(len({row["output"] for row in state}), 3)
                kind = next(row for row in metadata["mapping"].values() if row["native"] == "command.101.120")
                self.assertEqual((kind["min"], kind["max"], kind["native_offset"]), (base, base + 3, 140 - base))
                self.assertEqual(policy["leaf_bindings"], {"105": ["Bool"]})
                self.assertIn("supplied authority", " ".join(metadata["added_semantics"]))
                self.assertIn("equivalence outside the finite profile", metadata["not_claimed"])

    def test_source_ast_generates_native_guards_and_successors(self):
        cases = json.loads(self.parse()["rules"])["cases"]
        self.assertEqual([case["when"] for case in cases], ["!authorized", "amount == 0",
            "!(available <= max - reserved && available + reserved <= max - spent)",
            "kind == 140 && actual == 0 && available + reserved + spent <= max - amount", "kind == 140",
            "kind == 141 && actual == 0 && available >= amount", "kind == 141",
            "kind == 142 && actual <= amount && reserved >= amount", "kind == 142",
            "actual == 0 && reserved >= amount", "true"])
        self.assertEqual(cases[7]["post"], {"110": "available + (amount - actual)", "111": "reserved - amount", "112": "spent + actual"})
        self.assertTrue(all(case["post"] == {} and case["outbox"] == [] for case in cases if case["class"] == "Reject"))

    def test_profile_is_opt_in_and_domain_scaling_is_explicit(self):
        with self.assertRaises(importer.Refused): self.parse(self.owner(), {})
        with self.assertRaises(importer.Refused): importer.import_spec(self.flat(), {}, {})
        with self.assertRaises(importer.Refused): importer.import_spec(self.owner(), {256: 3}, {}, "unknown")
        generic = importer.import_spec(self.owner(), {256: 3}, {"i2.kind": (0, 3), "i3": (0, 1)})
        self.assertEqual(len(json.loads(generic["rules"])["cases"]), 6)
        self.assertNotIn("profile", generic["metadata"])
        self.assertEqual(self.parse(domains={"i2.kind": (0, 3), "i3": (0, 1)})["metadata"]["scope"], "explicitly-scaled")
        for domains in ({"i1.available": (0, 6)}, {"i2.kind": (0, 4)}, {"i3": (0, 3)}, {"i3": (1, 1)}):
            with self.subTest(domains=domains), self.assertRaises(importer.Refused): self.parse(domains=domains)
        with self.assertRaises(importer.Refused): self.parse(scaling={256: 3, 8: 3})
        with self.assertRaises(importer.Refused): self.parse(self.owner().replace("{0}:bv[256]", "{0}:bv[3]"))
        self.assertEqual(self.parse("# independent spelling\n" + self.owner())["rules"], self.parse()["rules"])

    def test_changed_guards_posts_widths_and_incomplete_roles_refuse(self):
        source = self.flat()
        bad = [source.replace("i7[t] = {1}", "i7[t] = {0}"),
               source.replace("i1[t] >= i5[t]", "i1[t] > i5[t]"),
               source.replace("i1[t] + (i5[t] - i6[t])", "i1[t] + i5[t]"),
               source.replace("{7}", "{6}"), source.replace("{1}:bv[3]", "{2}:bv[3]"),
               source.replace("o3[t] = i3[t] && ", "", 1),
               source.replace("o4[t] = {1}", "o3[t] = i3[t] && o4[t] = {1}", 1),
               source.replace("i7[t]", "i8[t]"), source.replace("i7[t]", "i7[t-1]"),
               source.replace("i1[t]", "i1[t].free"), source + ". T.",
               source.replace("bv[3]", "bv[4]"), source.replace("i4[t] = {3}", "i4[t] = {4}")]
        for changed in bad:
            with self.subTest(source=changed), self.assertRaises(importer.Refused): self.parse(changed, {})

    def test_correspondence_reparses_profile_roles_and_closed_enum_conversion(self):
        metadata = json.loads(json.dumps(self.parse(self.flat(), {})["metadata"]))
        # Serialization fixture only; it asserts no library decision or verdict.
        observation = {"inputs": {"pre.100.110": "1", "pre.100.111": "0", "pre.100.112": "0",
            "command.101.120": "140", "command.101.121": "1", "command.101.122": "0", "context.102.130": "1"},
            "class": "Accept", "post": {"110": "2", "111": "0", "112": "0"}, "outbox": []}
        queries = importer.correspondence(metadata, [observation])
        self.assertEqual([query["expected"] for query in queries], ["T", "F"])
        self.assertNotIn("{140}", queries[0]["tau"])
        for key, value in [("native_offset", 0), ("output", "o3")]:
            changed = json.loads(json.dumps(metadata))
            changed["mapping"]["i4" if key == "native_offset" else "i1"][key] = value
            with self.assertRaises(importer.Refused): importer.correspondence(changed, [])
        changed = json.loads(json.dumps(metadata)); changed["decision_output"] = "o2"
        with self.assertRaises(importer.Refused): importer.correspondence(changed, [])
        wrong = {**observation, "inputs": {**observation["inputs"], "command.101.120": "144"}}
        with self.assertRaises(importer.Refused): importer.correspondence(metadata, [wrong])


class ProtocolTests(unittest.TestCase):
    def bundle(self, formula="F"):
        return {"schema": runner.SCHEMA, "authority": "none", "queries": [
            {"id": "planted-false", "kind": "planted-control", "expected": "F", "tau": formula,
             "smt": "(set-logic QF_BV)\n(assert true)\n(check-sat)\n"}]}

    def test_query_period_and_commands_are_rejected_before_execution(self):
        for formula in ("F.", "F.   ", "F. valid T", "T.field", "F\nquit", "valid F", "F;quit", "F # comment"):
            with self.subTest(formula=formula), self.assertRaises(runner.Refused):
                runner.validated(self.bundle(formula))
        self.assertEqual(len(runner.validated(self.bundle())), 1)

    def test_duplicate_ids_keys_and_missing_controls_refuse(self):
        bundle = self.bundle()
        bundle["queries"] *= 2
        with self.assertRaises(runner.Refused):
            runner.validated(bundle)
        with self.assertRaises(runner.Refused):
            json.loads('{"a":0,"a":1}', object_pairs_hook=runner.unique_object)
        bundle = self.bundle()
        bundle["queries"][0]["expected"] = "T"
        with self.assertRaises(runner.Refused):
            runner.validated(bundle)

    def test_t_is_never_success_without_exact_unsat(self):
        for answer in (None, "unknown", "sat"):
            self.assertNotEqual(runner.classify("T", "T", answer), "corroborated")
        self.assertEqual(runner.classify("T", "T", "unsat"), "corroborated")
        self.assertEqual(runner.classify("F", "F", "sat"), "control-rejected")
        self.assertEqual(runner.classify("F", "T", "sat"), "disagreement")
        self.assertEqual(runner.classify("F", "T", "unsat"), "control-failed")
        self.assertEqual(runner.classify("T", "F", "unsat"), "disagreement")

    def test_ambiguous_failed_and_unknown_transcripts_do_not_pass(self):
        self.assertEqual(runner.answer("tau", "%1: T\n", 0, None), "T")
        self.assertEqual(runner.answer("tau", ": F\n", 0, None), "F")
        for output, code, failure in (("T\nF\n", 0, None), ("T\n", 1, None),
                                      ("T\n", 0, "timeout"), ("syntax error\nT\n", 0, None)):
            self.assertIsNone(runner.answer("tau", output, code, failure))
        self.assertEqual(runner.answer("smt", "unknown\n", 0, None), "unknown")
        self.assertIsNone(runner.answer("smt", "unsat\nsat\n", 0, None))

    def test_the_entire_terminal_transcript_must_be_one_supported_verdict(self):
        for kind, output in (("tau", "unknown\nT\n"), ("tau", "UNKNOWN\n%1: T\n"),
                             ("tau", "not implemented\nT\n"), ("tau", "unexpected\n: T\n"),
                             ("tau", "T\nwarning: result incomplete\n"), ("tau", "T\nT\n"),
                             ("tau", "unknown\rT\n"), ("tau", "\x1b[2K T\n"),
                             ("tau", "T\x00\n"), ("tau", "T\u00a0\n"), ("smt", "unknown\nunsat\n"),
                             ("smt", "not implemented\nunsat\n"), ("smt", "unsat\n(error bad)\n"),
                             ("smt", "unsat\nunsat\n")):
            with self.subTest(kind=kind, output=output):
                result = runner.answer(kind, output, 0, None)
                self.assertIsNone(result)
                self.assertEqual(runner.classify("T", result if kind == "tau" else "T",
                                                 result if kind == "smt" else "unsat"), "incomplete")
        for kind, output, expected in (("tau", "\n T \n\n", "T"), ("tau", "%1: F\r\n", "F"),
                                      ("tau", "\x1b[32m: T\x1b[0m\n", "T"),
                                      ("smt", "\nunsat\r\n", "unsat"), ("smt", "sat\n", "sat")):
            with self.subTest(kind=kind, output=output):
                self.assertEqual(runner.answer(kind, output, 0, None), expected)

    def test_run_one_keeps_mixed_tool_transcripts_incomplete(self):
        from unittest.mock import patch
        query = {"id": "transcript-fixture", "kind": "protocol-fixture", "expected": "T",
                 "tau": "T", "smt": "(set-logic ALL)\n(assert false)\n(check-sat)\n"}
        # Mock transport only: no native decision or tool qualification is supplied.
        def transport(output):
            return {"returncode": 0, "failure": None, "seconds": 0,
                    "output_sha256": "fixture", "output": output}
        for tau_text, smt_text in (("unknown\nT\n", "unsat\n"),
                                  ("not implemented\nT\n", "unsat\n"),
                                  ("T\n", "unknown\nunsat\n"),
                                  ("T\n", "unsat\n(error bad)\n")):
            with self.subTest(tau_text=tau_text, smt_text=smt_text), tempfile.TemporaryDirectory() as root:
                with patch.object(runner, "execute", side_effect=[transport(tau_text), transport(smt_text)]):
                    row = runner.run_one(query, Path("unused-tau"), Path("unused-smt"), "z3", Path(root), 1, 128)
                self.assertEqual(row["status"], "incomplete")

    def test_run_one_disables_tau_benchmarks_without_accepting_noisy_output(self):
        from unittest.mock import patch
        query = {"id": "benchmark-fixture", "kind": "protocol-fixture", "expected": "T",
                 "tau": "T", "smt": "(set-logic ALL)\n(assert false)\n(check-sat)\n"}
        # Transport fixtures test argv and parsing only, never tool qualification.
        def transport(output):
            return {"returncode": 0, "failure": None, "seconds": 0,
                    "output_sha256": "fixture", "output": output}
        outputs = (("%1: T\n", "unsat\n", "corroborated"),
                   ("%1: F\n", "sat\n", "refuted"),
                   ("\x1b[39mcalculate_fixed_points:\x1b[0m 5 µs\n%1: T\n", "unsat\n", "incomplete"),
                   ("normalize: 1 ms\n%1: T\n", "unsat\n", "incomplete"))
        for tau_text, smt_text, expected in outputs:
            with self.subTest(tau_text=tau_text), tempfile.TemporaryDirectory() as root:
                with patch.object(runner, "execute", side_effect=[transport(tau_text), transport(smt_text)]) as execute:
                    row = runner.run_one(query, Path("unused-tau"), Path("unused-smt"), "z3", Path(root), 1, 128)
                command = execute.call_args_list[0].args[0]
                self.assertEqual(command.count("--benchmarks"), 1)
                self.assertEqual(command[command.index("--benchmarks") + 1], "false")
                self.assertEqual(command[-2:], ["--evaluate", "valid T"])
                self.assertEqual(row["status"], expected)
                if expected == "incomplete":
                    self.assertIsNone(row["tau"]["answer"])

    def test_smt_terminal_command_is_parsed_not_counted_in_text(self):
        bad = ["; (check-sat)\n(set-logic QF_BV)\n(assert true)\n",
               '(set-logic QF_BV)\n(echo "unsat")\n; (check-sat)\n(exit)\n',
               '(set-logic QF_BV)\n(echo "(check-sat)")\n',
               '(set-logic QF_BV)\n(check-sat)\n(echo "unsat")\n',
               "(set-logic QF_BV)\n(assert (check-sat))\n",
               "(set-logic QF_BV)\n(check-sat)\n( check-sat )\n",
               "(set-logic QF_BV)\n(check-sat)\n(exit)\n",
               "(set-logic QF_BV)\n(check-sat)\n(get-model)\n",
               "(set-logic QF_BV)\n(assert true)\n(check-sat))\n",
               "(set-logic QF_BV)\n(assert true\n(check-sat)\n",
               "outside\n(set-logic QF_BV)\n(check-sat)\n",
               "(set-logic QF_BV)\n(check-sat assuming)\n"]
        for source in bad:
            bundle = self.bundle()
            bundle["queries"][0]["smt"] = source
            with self.subTest(source=source), self.assertRaises(runner.Refused):
                runner.validated(bundle)

    def test_smt_output_redirection_and_other_solver_commands_refuse(self):
        for command in ('(set-option :regular-output-channel "stdout")',
                        '(set-option :diagnostic-output-channel "stdout")',
                        "(set-option :regular-output-channel stdout)",
                        "(set-option :diagnostic-output-channel stdout)",
                        "(set-option :print-success true)", "(echo unsat)", "(exit)",
                        "(push 1)", "(reset)", "(check-sat-assuming ())",
                        "(declare-const |quoted name| Int)"):
            source = f"(set-logic ALL)\n{command}\n(assert false)\n(check-sat)\n"
            with self.subTest(command=command), self.assertRaises(runner.Refused):
                runner.validate_smt(source)

    def test_smt_emitted_native_and_source_profiles_remain_supported(self):
        native = """; native Script header, not a terminal command: (check-sat)
(set-logic ALL)
(set-option :produce-models true)
(declare-const x0 Int)
(assert (and (<= 0 x0) (<= x0 7)))
(define-fun p0 () Bool (= x0 1))
(define-fun p1 () Int (ite p0 (- 0 1) (+ x0 1)))
(assert (not (= p1 0)))
(check-sat)
; terminal comments do not add commands
"""
        runner.validate_smt(native)
        runner.validate_smt(runner.false_t_diagnostic()["smt"])
        metadata = importer.import_spec(SPEC, {}, {})["metadata"]
        # Serialization fixture only: these are not asserted library decisions.
        observation = {"inputs": {"pre.100.120": "7", "command.101.140": "1", "context.102": "1"},
                       "class": "Accept", "post": {"120": "0"}, "outbox": []}
        for query in importer.correspondence(metadata, [observation]):
            runner.validate_smt(query["smt"])

    def test_smt_text_nesting_and_command_count_are_bounded(self):
        for source in (" " * (runner.MAX_QUERY + 1),
                       "; comment\x00\n(set-logic ALL)\n(check-sat)\n",
                       "; comment\x7f\n(set-logic ALL)\n(check-sat)\n",
                       "; comment\u2028(set-logic ALL)\n(check-sat)\n",
                       "(set-logic ALL)\n(assert " + "(" * runner.MAX_SMT_DEPTH + "true"
                       + ")" * runner.MAX_SMT_DEPTH + ")\n(check-sat)\n",
                       "(set-logic ALL)\n" + "(assert true)\n" * runner.MAX_SMT_COMMANDS + "(check-sat)\n"):
            with self.assertRaises(runner.Refused):
                runner.validate_smt(source)

    def test_original_false_t_diagnostic_is_always_nonvalid_expected(self):
        query = runner.false_t_diagnostic()
        self.assertEqual(query["expected"], "F")
        self.assertIn("all a:bv[8]", query["tau"])
        self.assertFalse(query["tau"].endswith("."))
        self.assertIn("bvuge d e", query["smt"])

    def test_timeout_and_output_cap_of_a_dummy_process(self):
        # Exercise process controls only; this dummy is never a solver.
        import sys
        with tempfile.TemporaryDirectory() as root:
            directory = Path(root)
            result = runner.execute([sys.executable, "-I", "-c", "import time;time.sleep(3)"], "", directory,
                                    directory / "timeout.txt", 0.1, 128)
            self.assertEqual(result["failure"], "timeout")
            result = runner.execute([sys.executable, "-I", "-c", "import sys;sys.stdout.write('x' * (2 << 20))"], "", directory,
                                    directory / "output.txt", 2, 128)
            self.assertEqual(result["failure"], "output-cap")
            self.assertLessEqual((directory / "output.txt").stat().st_size, runner.MAX_OUTPUT)


if __name__ == "__main__":
    unittest.main()
