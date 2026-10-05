"""Guard the repaired gate bindings and refusal of successful empty test runs."""

import collections
import contextlib
import json
import shlex
import io
import subprocess
import unittest
from unittest import mock

import atdd


REPAIRED = (
    "bounded-completion", "inductive-claims", "reserved-domains",
    "determinism", "composed-program", "production-authority",
)


def check_explicit_targets(command):
    """These selected Cargo commands use conventional integration/example paths."""
    if command[:2] != ("cargo", "+1.97.1") or "-p" not in command:
        return
    package = command[command.index("-p") + 1]
    for index, arg in enumerate(command):
        if arg in ("--test", "--example"):
            directory = "tests" if arg == "--test" else "examples"
            path = atdd.ROOT / "crates" / package / directory / (command[index + 1] + ".rs")
            if not path.is_file():
                raise atdd.AcceptanceError(f"missing explicit Cargo target: {path}")


class RepairedRegistryTests(unittest.TestCase):
    def test_repaired_commands_reference_existing_targets(self):
        for scenario in REPAIRED:
            for command in atdd.SCENARIOS[scenario].commands:
                with self.subTest(scenario=scenario, command=command):
                    check_explicit_targets(command)
        for package, flag, target in (
            ("zeno-fcis-synthesis", "--test", "preparation"),
            ("zeno-fcis", "--example", "bounded_completion"),
            ("zeno-fcis-authority", "--test", "probe_divergence"),
        ):
            with self.subTest(stale=target), self.assertRaises(atdd.AcceptanceError):
                check_explicit_targets(("cargo", "+1.97.1", "test", "-p", package, flag, target))

    def test_historical_suites_are_private_and_exact_tests_are_not_substrings(self):
        expected = {
            "bounded-completion": {
                "oracle::tests::preparation::",
                "oracle::tests::checked_continuation::",
                "oracle::tests::bounded_completion::original_bounded_completion_example_runs",
            },
            "inductive-claims": {
                "oracle::authority::tests::admission_checks_the_command_and_context_against_its_own_schema",
                "oracle::authority::tests::genesis_is_checked_against_its_own_schema",
                "oracle::laws::tests::step_assumptions_must_be_enforced_on_the_decisions_they_are_assumed_on",
                "oracle::laws::tests::a_manifest_must_enforce_the_scopes_its_project_declares",
            },
            "reserved-domains": {
                "oracle::authority::tests::project_state_domains_cannot_enter_the_reserved_namespace",
                "oracle::adapter_zenodex::zusd::tests::precondition_hash_matches_the_explicit_value_domain",
            },
            "determinism": {
                "oracle::authority::tests::probe_run_counts_must_compare_at_least_two_executions",
                "oracle::authority::tests::probed_execution_returns_the_decision_that_every_run_agreed_on",
                "oracle::authority::tests::probed_rejections_are_compared_too",
                "oracle::tests::probe_divergence::",
            },
            "composed-program": {
                "oracle::composed_program::tests::",
                "oracle::tests::composed_construction::",
            },
        }
        for scenario, filters in expected.items():
            commands = [c for c in atdd.SCENARIOS[scenario].commands if "verification/Cargo.toml" in c]
            actual = set()
            for command in commands:
                self.assertEqual(command[command.index("-p") + 1], "zeno-fcis-kernel-laws")
                test_filter = command[command.index("--lib") + 1]
                self.assertEqual("--exact" in command, not test_filter.endswith("::"))
                self.assertIn("--all-features", command)
                self.assertIn("--locked", command)
                actual.add(test_filter)
            self.assertEqual(actual, filters, scenario)

    def test_empty_filter_or_package_cannot_pass(self):
        empty = "test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 8 filtered out\n"
        for tail in (("--lib", "missing_filter"), ()):
            command = ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-authority", *tail)
            with mock.patch.object(atdd.subprocess, "run", return_value=subprocess.CompletedProcess(command, 0, empty)):
                with contextlib.redirect_stdout(io.StringIO()), self.assertRaisesRegex(atdd.AcceptanceError, "ran no tests"):
                    atdd.run_command(command)

    def test_failed_command_is_not_hidden_by_an_earlier_passing_binary(self):
        command = ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-synthesis")
        output = "test result: ok. 4 passed; 0 failed; 0 ignored\n"
        with mock.patch.object(atdd.subprocess, "run", return_value=subprocess.CompletedProcess(command, 101, output)):
            with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(subprocess.CalledProcessError):
                atdd.run_command(command)

    def test_miri_covers_all_current_synthesis_targets_without_template_filters(self):
        source = (atdd.ROOT / ".github/workflows/miri.yml").read_text()
        rows = json.loads(source.split("        include: ", 1)[1].split("    env:", 1)[0])
        rows = [row for row in rows if row["packages"] == "-p zeno-fcis-synthesis"]
        actual = [(row.get("command", "test"), *shlex.split(row["target"]))
                  for row in rows if "test" not in row]
        synthesis = atdd.ROOT / "crates/zeno-fcis-synthesis"
        # Examples have no test harness, so Miri interprets each one with `miri run`.
        expected = [("test", "--lib"), ("test", "--doc")] + [
            ("test", "--test", path.stem) for path in (synthesis / "tests").glob("*.rs")
        ] + [("run", "--example", path.stem) for path in (synthesis / "examples").glob("*.rs")]
        self.assertEqual(collections.Counter(actual), collections.Counter(expected))
        template = [row for row in rows if row["target"] == "--test v2_template_contracts"]
        self.assertEqual(len(template), 1)
        self.assertNotIn("test", template[0])
        self.assertNotIn("--skip", template[0]["target"])

    def test_current_authority_is_bound_to_real_admission_and_public_api_tests(self):
        commands = atdd.SCENARIOS["production-authority"].commands
        targets = [c[i + 1] for c in commands for i, a in enumerate(c) if a == "--test"]
        self.assertCountEqual(targets, ["v2_schema_binding", "v2_authority", "v2_catalog", "program_api"])
        self.assertTrue(any("finite::execution_v2::catalog::tests::" in c for c in commands))
        self.assertTrue(any("finite::execution_v2::continuation::tests::" in c
                            for c in atdd.SCENARIOS["bounded-completion"].commands))


if __name__ == "__main__":
    unittest.main()
