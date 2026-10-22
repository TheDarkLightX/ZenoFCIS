"""Guard the repaired gate bindings and refusal of successful empty test runs."""

import ast
import collections
import contextlib
import json
import os
import re
import shlex
import io
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path
from unittest import mock

import atdd
import miri_exclusions


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


def miri_rows(source):
    return json.loads(source.split("        include: ", 1)[1].split("    env:", 1)[0])


def run_miri_coverage_check(source, selected, inventories, honour_skip=True, ignored=None, test_args=""):
    """Run miri.yml's own coverage script against a fake Cargo and libtest.

    `inventories` maps a Cargo target such as `--lib` to its test names and
    `ignored` to its ignored names. The fake libtest applies `--exact`,
    filters, `--skip` and `--ignored` as libtest does. `test_args` is the
    MIRI_TEST_ARGS an earlier step wrote. Returns the MIRI_TEST_ARGS the script
    writes, or "" when it writes none.
    """
    script = textwrap.dedent(source.split("python3 - <<'PY'\n", 1)[1].split("\n          PY\n", 1)[0])
    synthesis = atdd.ROOT / "crates/zeno-fcis-synthesis"
    targets = [{"kind": ["lib"], "name": "zeno_fcis_synthesis", "test": True, "doctest": True}]
    targets += [{"kind": ["test"], "name": path.stem, "test": True} for path in sorted((synthesis / "tests").glob("*.rs"))]
    targets += [{"kind": ["example"], "name": path.stem, "test": False} for path in sorted((synthesis / "examples").glob("*.rs"))]
    metadata = json.dumps({"packages": [{"name": "zeno-fcis-synthesis", "targets": targets}]})

    def cargo(arguments, **_):
        if arguments[2] == "metadata":
            return subprocess.CompletedProcess(arguments, 0, metadata)
        separator = arguments.index("--")
        target = " ".join(arguments[arguments.index("zeno-fcis-synthesis") + 1:separator])
        flags, filters, skips = arguments[separator + 1:], [], []
        index = 0
        while index < len(flags):
            if flags[index] == "--skip":
                skips.append(flags[index + 1])
                index += 1
            elif flags[index] == "--format":
                index += 1
            elif not flags[index].startswith("--"):
                filters.append(flags[index])
            index += 1
        matches = (lambda f, n: f == n) if "--exact" in flags else (lambda f, n: f in n)
        names = [n for n in inventories[target]
                 if (not filters or any(matches(f, n) for f in filters))
                 and not (honour_skip and any(matches(s, n) for s in skips))
                 and ("--ignored" not in flags or n in (ignored or {}).get(target, ()))]
        return subprocess.CompletedProcess(arguments, 0, "".join(f"{n}: test\n" for n in names))

    with tempfile.TemporaryDirectory(prefix="zeno-fcis-miri-coverage-") as raw:
        root = Path(raw)
        (root / ".github/workflows").mkdir(parents=True)
        (root / ".github/workflows/miri.yml").write_text(source)
        (root / "crates/zeno-fcis-synthesis/tests").mkdir(parents=True)
        (root / "crates/zeno-fcis-synthesis/tests/completion.rs").write_text(
            (synthesis / "tests/completion.rs").read_text())
        environment = root / "github-env"
        environment.write_text("")
        with mock.patch.object(subprocess, "run", cargo), \
                mock.patch.dict(os.environ, {"SELECTED_GROUP": json.dumps(selected), "GITHUB_ENV": str(environment),
                                             "MIRI_TEST_ARGS": test_args}), \
                contextlib.chdir(root), contextlib.redirect_stdout(io.StringIO()):
            exec(compile(script, "miri.yml coverage check", "exec"), {"__name__": "__main__"})
        written = environment.read_text().splitlines()
    return written[0].removeprefix("MIRI_TEST_ARGS=") if written else ""


TEMPLATE_TESTS = "crates/zeno-fcis-synthesis/tests/v2_template_contracts.rs"
TEMPLATE_NATIVE_ONLY = {
    "all_original_small_domains_and_unlawful_order_states": ("assert_eq!(counts, [64, 864, 1728]);", 3),
    "account_complete_clock_boundaries_without_domain_narrowing": ("assert_eq!(count, 13122);", 1),
    "withdrawal_complete_raw_domain_and_certified_controller": ("assert_eq!(cases, 1_296_000);", 1),
    "treasury_guard_arithmetic_callbacks_and_unlawful_prestates": ("assert_eq!(counts, [103_680, 677_376, 82_944]);", 3),
}
# Corpus, native cases, bounded-profile test suffix and application module.
TEMPLATE_CORPORA = [("COUNTER_CORPUS", 64, "durable_counter", "counter"),
                    ("STOCK_CORPUS", 864, "inventory_reservation", "stock"),
                    ("ORDER_CORPUS", 1728, "order_fulfillment", "order"),
                    ("ACCOUNT_CORPUS", 13_122, "account_lockout", "account"),
                    ("WITHDRAWAL_CORPUS", 1_296_000, "withdrawal_queue", "vault"),
                    ("TREASURY_PROPOSALS", 103_680, "treasury_proposals", "treasury"),
                    ("TREASURY_CALLBACKS", 677_376, "treasury_callbacks", "treasury"),
                    ("TREASURY_GUARDS", 82_944, "treasury_guards", "treasury")]
TEMPLATE_PROFILE_TESTS = ["miri_bounded_profile_" + suffix for _, _, suffix, _ in TEMPLATE_CORPORA]
TEMPLATE_RETAINED = [('durable_counter', 'counter', 0, 12), ('inventory_reservation', 'stock', 1, 20), ('order_fulfillment', 'order', 2, 23), ('account_lockout', 'account', 3, 20), ('withdrawal_queue', 'vault', 4, 26), ('agent_treasury_guard', 'treasury', 5, 30)]
TEMPLATE_RETAINED_CALLS = {}
for suffix, module, index, count in TEMPLATE_RETAINED:
    if suffix in {"order_fulfillment", "withdrawal_queue", "agent_treasury_guard"}:
        for start in range(0, count, 5):
            end = min(start + 5, count)
            name = f"retained_examples_and_replay_{suffix}_{start:02}_{end - 1:02}"
            TEMPLATE_RETAINED_CALLS[name] = f"retained_examples_batch!({module}, {index}, {count}, {start}, {end});"
        TEMPLATE_RETAINED_CALLS["retained_genesis_and_replay_" + suffix] = f"retained_genesis!({module}, {index});"
    else:
        TEMPLATE_RETAINED_CALLS["retained_complete_examples_genesis_and_replay_" + suffix] = f"retained_examples_and_genesis!({module}, {index}, {count});"
TEMPLATE_RETAINED_TESTS = list(TEMPLATE_RETAINED_CALLS)


def template_profile_problems(source):
    """Static mirror of the bounded Miri profile's pins in v2_template_contracts.rs.

    The Rust tests enforce these at run time; this guard refuses source that
    drops a native count, an observation check, a pinned signature, dimension,
    boundary or bounded case, or adds a Miri or environment branch.
    """
    problems = []

    def body(name):
        found = re.findall(r"((?:^[ \t]*#\[[^\n]*\n)+)fn " + name + r"\(\) \{\n(.*?)^\}", source, re.M | re.S)
        if len(found) != 1 or "#[test]" not in found[0][0] or re.search(r"#\[\s*(?:ignore|cfg)", found[0][0]):
            problems.append(f"{name} must be one plain #[test]")
            return ""
        return found[0][1]

    for name, (count, corpora) in TEMPLATE_NATIVE_ONLY.items():
        text = body(name)
        if count not in text or not re.search(r"check_observations\(&[^;]*, false\);", text):
            problems.append(f"{name} must keep its native count and check every observation")
        if text.count("Observation::new(&") != corpora:
            problems.append(f"{name} must observe its {corpora} corpora")
    helper = re.findall(r"^fn check_bounded\(observed: Observation<'_>, cases: usize\) \{\n(.*?)^\}", source, re.M | re.S)
    if len(helper) != 1 or "check_observations(std::slice::from_ref(&observed), true);" not in helper[0] \
            or "assert_eq!(observed.cases, cases);" not in helper[0]:
        problems.append("the bounded profile must check its observations and pinned count")
    for name, expected in TEMPLATE_RETAINED_CALLS.items():
        if body(name).strip() != expected:
            problems.append(f"{name} must retain its application and exact example interval or genesis")
    helper = re.search(r"^fn examples_range\([\s\S]*?^\}", source, re.M)
    required = ["assert!(start < end && end <= example_count(index));", ".enumerate()", ".skip(start)", ".take(end - start)"]
    if helper is None or any(part not in helper.group(0) for part in required):
        problems.append("example ranges must execute their exact nonempty, in-bounds intervals")
    if "assert_eq!(example_count($index), $total);" not in source or "assert_eq!(count, $end - $start);" not in source:
        problems.append("example batches must pin their full corpus and exact executed count")
    cases = []
    for const, size, suffix, module in TEMPLATE_CORPORA:
        block = re.search(r"^const " + const + r": Corpus = Corpus \{\n(.*?)^\};", source, re.M | re.S)
        pinned = re.fullmatch(r"\s*check_bounded\(\s*with_app!\(" + module + r", \|a, f\| bounded\(&" + const
                              + r", a, f\)\),\s*(\d+),?\s*\);\s*", body("miri_bounded_profile_" + suffix))
        if block is None or pinned is None:
            problems.append(f"{const} must be defined and bounded")
            cases.append(-1)
            continue
        block = block.group(1)
        dimensions = [[int(v.replace("_", "")) for v in values.split(",") if v.strip()]
                      for _, values in re.findall(r'\(\s*"(\w+)",\s*&\[([^\]]*)\],?\s*\)', block)]
        product = 1
        for values in dimensions:
            product *= len(values)
        declared = re.search(r"\bsize: ([\d_]+),", block)
        if not dimensions or product != size or declared is None or int(declared.group(1).replace("_", "")) != size \
                or any(values != sorted(set(values)) for values in dimensions):
            problems.append(f"{const} dimensions must pin its {size} native cases")
        sections = {key: block.split(key + ": &[", 1)[1] if key + ": &[" in block else ""
                    for key in ("outcomes", "boundaries", "profile")}
        if not re.match(r'\s*"', sections["outcomes"]):
            problems.append(f"{const} must pin its outcome signatures")
        if not re.match(r'\s*\(\s*"', sections["boundaries"]):
            problems.append(f"{const} must name boundary witnesses")
        profile = [[int(v) for v in case.split(",") if v.strip()]
                   for case in re.findall(r"&\[([-\d, \n]*)\]", sections["profile"])]
        if not profile or profile != sorted(profile) or len({tuple(c) for c in profile}) != len(profile) \
                or any(len(c) != len(dimensions) or any(v not in d for v, d in zip(c, dimensions)) for c in profile):
            problems.append(f"{const} bounded cases must be ascending, distinct native cases")
        if int(pinned.group(1)) != len(profile):
            problems.append(f"{const} bounded count must be pinned as {len(profile)}")
        cases.append(len(profile))
    if sum(cases) != 92:
        problems.append(f"the bounded profile must keep 92 cases, not {cases}")
    if re.search(r"cfg!?\(\s*(?:not\(\s*)?miri|cfg_attr\(\s*(?:not\(\s*)?miri|env::var", source):
        problems.append("template tests must not branch on Miri or the environment")
    return problems


class RepairedRegistryTests(unittest.TestCase):
    def test_crash_refinement_suites_enable_crash_injection(self):
        # Default store builds hide the interruption APIs. Both acceptance
        # routes must opt in so their atomicity controls cannot silently skip.
        for scenario, target in (("checked-upgrade", "upgrade"), ("sqlite-authority", None)):
            commands = [
                command for command in atdd.SCENARIOS[scenario].commands
                if command[:3] == ("cargo", "+1.97.1", "test")
                and "-p" in command
                and command[command.index("-p") + 1] == "zeno-fcis-shell-sqlite"
                and (target is None or "--test" in command
                     and command[command.index("--test") + 1] == target)
            ]
            with self.subTest(scenario=scenario):
                self.assertEqual(len(commands), 1)
                command = commands[0]
                self.assertIn("--features", command)
                features = command[command.index("--features") + 1].split(",")
                self.assertIn("crash-injection", features)
                if target is None:
                    self.assertNotIn("--lib", command)
                    self.assertNotIn("--test", command)

    def test_registry_failure_stops_run_before_expensive_checks(self):
        command = ("python3", "tools/test_atdd_registry.py")
        with mock.patch.object(atdd, "parse_args", return_value=mock.Mock(command="run", all=True)), \
                mock.patch.object(atdd, "run_command", side_effect=subprocess.CalledProcessError(1, command)) as run, \
                mock.patch.object(atdd, "run_scenario") as scenarios, \
                contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(atdd.main(), 1)
        run.assert_called_once_with(command)
        scenarios.assert_not_called()

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
        self.assertEqual(len(template), 32)
        self.assertEqual(sum("test" not in row for row in template), 1)
        self.assertTrue(all("--skip" not in row["target"] for row in template))

    def test_miri_split_targets_run_each_listed_test_exactly_once(self):
        source = (atdd.ROOT / ".github/workflows/miri.yml").read_text()
        rows = miri_rows(source)
        library = [row for row in rows if row.get("target") == "--lib"]
        remainder = [row for row in library if "test" not in row]
        isolated = [row["test"] for row in library if "test" in row]
        self.assertEqual([row["group"] for row in remainder], ["synthesis-library"])
        self.assertEqual(len(isolated), 9)
        # Each isolated name is one real library unit test, so CI cannot be
        # first to discover a misspelt selector.
        sources = "\n".join(path.read_text() for path in
                            (atdd.ROOT / "crates/zeno-fcis-synthesis/src").rglob("*.rs"))
        for name in isolated:
            leaf = name.rsplit("::", 1)[1]
            self.assertEqual(len(re.findall(r"#\[test\]\s+fn " + leaf + r"\(", sources)), 1, name)
        graphs = [row["test"] for row in rows if row.get("target") == "--test completion" and "test" in row]
        inventories = {"--lib": sorted(isolated + ["finite::common::first", "finite::common::second"]),
                       "--test completion": sorted(graphs + ["common_completion"])}
        expected = "-- " + shlex.join(["--exact"] + [a for name in isolated for a in ("--skip", name)])
        self.assertEqual(run_miri_coverage_check(source, remainder[0], inventories), expected)
        for row in library[1:]:
            self.assertEqual(run_miri_coverage_check(source, row, inventories),
                             "-- " + shlex.join(["--exact", row["test"]]))
        completion = next(row for row in rows if row["group"] == "synthesis-completion")
        self.assertEqual(run_miri_coverage_check(source, completion, inventories),
                         "-- " + shlex.join(["--exact"] + [a for name in graphs for a in ("--skip", name)]))
        self.assertEqual(run_miri_coverage_check(source, next(row for row in rows if row["group"] == "synthesis-finite"),
                                                 inventories), "")

        # Planted controls: each must be refused by the workflow's own check.
        with self.assertRaisesRegex(AssertionError, "split selector coverage differs"):
            run_miri_coverage_check(source, remainder[0], inventories, honour_skip=False)
        renamed = dict(inventories, **{"--lib": [n for n in inventories["--lib"] if n != isolated[0]]})
        for row in (remainder[0], library[1]):
            with self.assertRaisesRegex(AssertionError, "empty test inventory"):
                run_miri_coverage_check(source, row, renamed)

        def planted(row):
            anchor = json.dumps(library[1]) + ",\n"
            changed = source.replace(anchor, anchor + "          " + json.dumps(row) + ",\n", 1)
            self.assertNotEqual(changed, source)
            return changed
        with self.assertRaisesRegex(AssertionError, "duplicate exact test row"):
            run_miri_coverage_check(planted(dict(library[1], group="synthesis-library-again")), remainder[0], inventories)
        with self.assertRaisesRegex(AssertionError, "split target needs one remainder row"):
            run_miri_coverage_check(planted(dict(library[1], group="stray", target="--test completon")),
                                    remainder[0], inventories)
        with self.assertRaisesRegex(AssertionError, "exact tests need miri test"):
            run_miri_coverage_check(planted(dict(library[1], group="run", test="other", command="run")),
                                    remainder[0], inventories)

        # A remainder with pinned exclusions composes them with its exact-test
        # skips: the exclusion step's arguments stay first and unchanged, and each
        # exclusion must name a listed test that no group runs.
        excluded = dict(completion, miri_skip=["native_only_completion"])
        skipping = source.replace(json.dumps(completion), json.dumps(excluded), 1)
        self.assertNotEqual(skipping, source)
        exclusion = miri_exclusions.miri_test_arguments(excluded)
        inventories = dict(inventories, **{"--test completion": sorted(graphs + ["common_completion",
                                                                                  "native_only_completion"])})
        self.assertEqual(run_miri_coverage_check(skipping, excluded, inventories, test_args=exclusion),
                         exclusion + " " + shlex.join([a for name in graphs for a in ("--skip", name)]))
        discarding = skipping.replace("row.get('miri_skip', []) + isolated", "isolated", 1)
        self.assertNotEqual(discarding, skipping)
        for workflow, args in ((discarding, exclusion), (skipping, ""), (skipping, exclusion + " --skip extra")):
            with self.assertRaisesRegex(AssertionError, "composed Miri arguments must retain every exclusion"):
                run_miri_coverage_check(workflow, excluded, inventories, test_args=args)
        invented = dict(completion, miri_skip=["invented_completion"])
        with self.assertRaisesRegex(AssertionError, "split target tests must run exactly once"):
            run_miri_coverage_check(source.replace(json.dumps(completion), json.dumps(invented), 1), invented,
                                    inventories, test_args=miri_exclusions.miri_test_arguments(invented))

    def test_miri_original_compound_runs_each_test_in_its_own_group(self):
        # Interpreted alone, two of the three tests each take about an hour,
        # so each runs in its own group under the hosted per-group limit.
        source = (atdd.ROOT / ".github/workflows/miri.yml").read_text()
        target = "--test v2_original_compound"
        rows = [row for row in miri_rows(source) if row.get("target") == target]
        remainder = [row for row in rows if "test" not in row]
        exact = [row for row in rows if "test" in row]
        self.assertEqual(len(remainder), 1)
        self.assertTrue(all("miri_skip" not in row for row in rows))
        rust = (atdd.ROOT / "crates/zeno-fcis-synthesis/tests/v2_original_compound.rs").read_text()
        inventory = sorted(re.findall(r"#\[test\]\s*fn (\w+)\(", rust))
        self.assertEqual(len(inventory), 3)
        isolated = [row["test"] for row in exact]
        self.assertEqual(sorted(isolated), [
            "original_compounds_patch_carry_forward_and_replay_without_reencoding_loss",
            "original_parser_denied_reads_and_steps_remain_refusals",
        ])
        inventories = {target: inventory}
        self.assertEqual(run_miri_coverage_check(source, remainder[0], inventories),
                         "-- " + shlex.join(["--exact"] + [a for name in isolated for a in ("--skip", name)]))
        for row in exact:
            self.assertEqual(run_miri_coverage_check(source, row, inventories),
                             "-- " + shlex.join(["--exact", row["test"]]))
        # Planted control: a remainder that ignored its skips would run the
        # isolated tests twice, which the workflow's own check refuses.
        with self.assertRaisesRegex(AssertionError, "split selector coverage differs"):
            run_miri_coverage_check(source, remainder[0], inventories, honour_skip=False)

    def test_miri_template_partitions_run_bounded_profile_and_skip_only_native_corpora(self):
        source = (atdd.ROOT / ".github/workflows/miri.yml").read_text()
        rows = miri_rows(source)
        target = "--test v2_template_contracts"
        template = [row for row in rows if row.get("target") == target]
        remainder = next(row for row in template if "test" not in row)
        exact = [row for row in template if "test" in row]
        native_only = list(TEMPLATE_NATIVE_ONLY)
        self.assertEqual(remainder["miri_skip"], native_only)
        self.assertTrue(all("miri_skip" not in row for row in exact))
        isolated = [row["test"] for row in exact]
        self.assertEqual(isolated, TEMPLATE_RETAINED_TESTS + TEMPLATE_PROFILE_TESTS)
        entries = miri_exclusions.load_exclusions()
        self.assertEqual([e["test"] for e in entries if e["group"] == remainder["group"]], native_only)
        self.assertEqual(len(entries), 5)
        rust = (atdd.ROOT / TEMPLATE_TESTS).read_text()
        self.assertEqual(template_profile_problems(rust), [])
        sum_native = sum(size for _, size, _, _ in TEMPLATE_CORPORA)
        self.assertEqual(sum_native, 2_175_778)

        inventory = sorted(re.findall(r"#\[test\]\s*(?:#\[ignore[^\n]*\n\s*)?fn (\w+)\(", rust))
        self.assertEqual(len(inventory), 38)
        inventories = {target: inventory}
        ignored = {target: []}
        arguments = miri_exclusions.miri_test_arguments(remainder)

        def coverage(row=remainder, workflow=source, args=None, names=inventories, skip=True, ignore=ignored):
            if args is None:
                args = miri_exclusions.miri_test_arguments(row)
            return run_miri_coverage_check(workflow, row, names, honour_skip=skip, ignored=ignore, test_args=args)
        # The remainder keeps the four native-only skips first, then skips the
        # thirty-one exact-test groups; each exact-test group runs its one test.
        self.assertEqual(coverage(), arguments + " " + shlex.join([a for name in isolated for a in ("--skip", name)]))
        for row in exact:
            self.assertEqual(coverage(row), "-- " + shlex.join(["--exact", row["test"]]))

        # Planted controls: each must be refused by the workflow's own check.
        def with_rows(changed_rows):
            head, rest = source.split("        include: ", 1)
            return head + "        include: " + json.dumps(changed_rows) + "\n    env:" + rest.split("    env:", 1)[1]

        def replaced(old, new):
            return [new if row == old else row for row in rows]
        wrong = dict(remainder, miri_skip=native_only[:3] + ["policy_schema_law_and_meter_mutations_refuse"])
        undeclared = dict(remainder, miri_skip=native_only + ["policy_schema_law_and_meter_mutations_refuse"])
        absent = {k: v for k, v in remainder.items() if k != "miri_skip"}
        repeated = dict(remainder, miri_skip=native_only + native_only[:1])
        for row in (wrong, undeclared, absent, repeated):
            with self.assertRaisesRegex(AssertionError, "template native-only workloads differ"):
                coverage(row, with_rows(replaced(remainder, row)))
        for row in (wrong, undeclared, absent):
            with self.assertRaises(miri_exclusions.ExclusionError):
                miri_exclusions.check_static(atdd.ROOT, entries, replaced(remainder, row))
        moved = dict(exact[1], miri_skip=native_only[:1])
        with self.assertRaisesRegex(AssertionError, "only a remainder row can carry Miri exclusions"):
            coverage(moved, with_rows(replaced(exact[1], moved)))
        native_group = dict(exact[1], group="synthesis-v2-template-native", test=native_only[0])
        with self.assertRaisesRegex(AssertionError, "an exact test row selects a Miri exclusion"):
            coverage(workflow=with_rows(rows + [native_group]))
        with self.assertRaisesRegex(AssertionError, "duplicate exact test row"):
            coverage(workflow=with_rows(rows + [dict(exact[1], group="synthesis-v2-template-again")]))
        for changed in ([row for row in rows if row != exact[1]], [row for row in rows if row != exact[0]],
                        rows + [dict(exact[1], group="synthesis-v2-template-invented", test="invented_profile")]):
            with self.assertRaisesRegex(AssertionError, "template exact-test groups differ"):
                coverage(workflow=with_rows(changed))
        for args in ("", arguments + " --skip miri_bounded_profile_durable_counter",
                     arguments.replace("--exact ", "")):
            with self.assertRaisesRegex(AssertionError, "skip exactly the native-only corpora"):
                coverage(args=args)
        with self.assertRaisesRegex(AssertionError, "skip exactly the native-only corpora"):
            coverage(exact[1], args=arguments)
        with self.assertRaisesRegex(AssertionError, "template Miri remainder must run"):
            coverage(skip=False)
        discarding = source.replace("row.get('miri_skip', []) + isolated", "isolated", 1)
        self.assertNotEqual(discarding, source)
        with self.assertRaisesRegex(AssertionError, "template Miri remainder must run"):
            coverage(workflow=discarding)
        for names in ([n for n in inventory if n != TEMPLATE_PROFILE_TESTS[0]],
                      [n for n in inventory if n != "compiled_contracts_reproduce_their_committed_policy_bytes"],
                      inventory + ["invented_test"]):
            for row in (remainder, exact[1]):
                with self.assertRaisesRegex(AssertionError, "template semantic inventory differs"):
                    coverage(row, names={target: names})
        for row in (remainder, exact[1]):
            for ignored_name in (TEMPLATE_PROFILE_TESTS[0], "compiled_contracts_reproduce_their_committed_policy_bytes"):
                with self.assertRaisesRegex(AssertionError, "must not be ignored"):
                    coverage(row, ignore={target: [ignored_name]})

        # Planted source controls for the pinned counts, signatures, values,
        # boundaries and bounded cases.
        def problems(old, new, count=1):
            self.assertEqual(rust.count(old), count, old)
            return template_profile_problems(rust.replace(old, new, 1))
        self.assertTrue(problems("assert_eq!(cases, 1_296_000);", "assert_eq!(cases, 1_295_999);"))
        self.assertTrue(problems("check_observations(&[observed], false);", "", 2))
        self.assertTrue(problems("    size: 13_122,", "    size: 13_121,"))
        self.assertTrue(problems('("caller", &[190, 191, 192, 193]),', '("caller", &[190, 191, 192]),'))
        self.assertTrue(problems('outcomes: &["accept", "failure 202", "reject 200", "reject 201"],', "outcomes: &[],"))
        self.assertTrue(problems('    boundaries: &[\n        ("allowed increment reaches 3"',
                                 '    boundaries: &[],\n    unnamed: &[\n        ("allowed increment reaches 3"'))
        self.assertTrue(problems("        &[0, 1, 120, 1],\n", ""))
        self.assertTrue(problems("        &[0, 1, 120, 1],\n", "        &[0, 1, 122, 1],\n"))
        self.assertTrue(problems("bounded(&WITHDRAWAL_CORPUS, a, f)),\n        10,", "bounded(&WITHDRAWAL_CORPUS, a, f)),\n        9,"))
        self.assertTrue(problems("bounded(&ORDER_CORPUS, a, f)), 11);", "bounded(&ORDER_CORPUS, a, f)), 12);"))
        self.assertTrue(problems("with_app!(vault, |a, f| bounded(&WITHDRAWAL_CORPUS",
                                 "with_app!(account, |a, f| bounded(&WITHDRAWAL_CORPUS"))
        self.assertTrue(problems("        &[0, 180, 0, 181, 1, 0, 0, 170, 162, 170, 1, 193, 0],\n", ""))
        self.assertTrue(problems("    check_observations(std::slice::from_ref(&observed), true);\n", ""))
        self.assertTrue(problems("    assert_eq!(observed.cases, cases);\n", ""))
        self.assertTrue(problems("#[test]\nfn miri_bounded_profile_withdrawal_queue",
                                 "#[test]\n#[ignore]\nfn miri_bounded_profile_withdrawal_queue"))
        self.assertTrue(problems("fn miri_bounded_profile_withdrawal_queue() {\n",
                                 "fn miri_bounded_profile_withdrawal_queue() {\n    if cfg!(miri) {}\n"))
        self.assertTrue(problems("fn miri_bounded_profile_treasury_guards()", "fn miri_bounded_profile_treasury_guard()"))
        self.assertTrue(problems("retained_examples_batch!(treasury, 5, 30, 25, 30);",
                                 "retained_examples_batch!(treasury, 5, 29, 25, 30);"))
        self.assertTrue(problems("retained_examples_batch!(order, 2, 23, 5, 10);",
                                 "retained_examples_batch!(order, 2, 23, 4, 10);"))
        self.assertTrue(problems("retained_examples_batch!(vault, 4, 26, 25, 26);",
                                 "retained_examples_batch!(vault, 4, 26, 24, 26);"))
        self.assertTrue(problems("retained_genesis!(vault, 4);", "retained_genesis!(stock, 4);"))
        self.assertTrue(problems("retained_genesis!(treasury, 5);", "retained_genesis!(order, 5);"))
        self.assertTrue(problems("        .skip(start)\n", "        .skip(start + 1)\n"))
        self.assertTrue(problems("        .take(end - start)\n", "        .take(end - start - 1)\n"))
        self.assertTrue(problems("    assert!(start < end && end <= example_count(index));\n", ""))
        self.assertTrue(problems("        assert_eq!(example_count($index), $total);\n", ""))
        self.assertTrue(problems("        assert_eq!(count, $end - $start);\n", ""))
        self.assertTrue(problems("retained_examples_and_genesis!(account, 3, 20);",
                                 "retained_examples_and_genesis!(stock, 3, 20);"))
        self.assertTrue(problems("#[test]\nfn retained_complete_examples_genesis_and_replay_durable_counter",
                                 "#[test]\n#[ignore]\nfn retained_complete_examples_genesis_and_replay_durable_counter"))

    def test_current_authority_is_bound_to_real_admission_and_public_api_tests(self):
        commands = atdd.SCENARIOS["production-authority"].commands
        targets = [c[i + 1] for c in commands for i, a in enumerate(c) if a == "--test"]
        self.assertCountEqual(targets, ["v2_schema_binding", "v2_authority", "v2_catalog", "program_api"])
        self.assertTrue(any("finite::execution_v2::catalog::tests::" in c for c in commands))
        self.assertTrue(any("finite::execution_v2::continuation::tests::" in c
                            for c in atdd.SCENARIOS["bounded-completion"].commands))


    def test_the_app_journey_runs_its_own_tests_before_the_journey(self):
        commands = atdd.SCENARIOS["app-journey"].commands
        self.assertEqual(commands, (
            ("python3", "tools/test_check_app_journey.py"),
            ("python3", "tools/test_relay.py"),
            ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-shell-sqlite", "--test", "relay",
             "--locked"),
            ("python3", "tools/check_app_journey.py")))
        for command in commands:
            if command[0] == "python3":
                self.assertTrue((atdd.ROOT / command[1]).is_file(), command)
        self.assertTrue((atdd.ROOT / "crates/zeno-fcis-shell-sqlite/tests/relay.rs").is_file())


# Every test_*.py in the repository that git does not ignore must be named by a
# whole-file command in an ATDD scenario or a workflow `run:` step; triggers and
# step conditions are not checked. A file run only inside another command is
# listed with that host: `runs` means some command invokes the host with the
# listed leading arguments and the host runs the file; `imports` that the host
# test file imports every TestCase the file defines, so the host's
# unittest.main() runs them.
RUN_INSIDE = {
    "tools/test_atdd_registry.py": ("tools/atdd.py", "runs", ("run",),
                                    "`atdd.py run` and `atdd.py self-test` run it first"),
    "tools/test_scaffold_versions.py": ("tools/atdd.py", "runs", ("run",),
                                        "`atdd.py run` runs it before any scenario"),
    "tools/test_release_privacy.py": ("tools/rc_package.py", "runs", ("self-test",),
                                      "`rc_package.py self-test` runs it"),
    "tools/test_check_installed_cli.py": ("tools/rc_package.py", "runs", ("self-test",),
                                         "`rc_package.py self-test` runs it"),
    "tools/test_check_release_custody.py": ("tools/rc_package.py", "runs", ("self-test",),
                                           "`rc_package.py self-test` runs it"),
    "tools/test_release_hosts.py": ("tools/rc_package.py", "runs", ("self-test",),
                                   "`rc_package.py self-test` runs it"),
    "tools/test_core_components.py": ("tools/check_core_components.py", "runs", ("--finite-proof",),
                                      "`--finite-proof` runs it on the staged qualification source"),
    "tools/test_core_family_proofs.py": ("tools/check_core_components.py", "runs", ("--finite-proof",),
                                         "`--finite-proof` runs it on the staged qualification source"),
    "tools/test_negative_retention.py": ("tools/test_check_finite_execution.py", "imports", (),
                                         "NegativeRetentionTests"),
    "tools/test_verus_contract_controls.py": ("tools/test_check_envelope_frame_v2.py", "imports", (),
                                              "ContractClassifier"),
    "site/tests/test_wasm.py": ("site/build.py", "runs", (), "its `check` stage runs it; pages.yml runs every stage"),
    "site/tests/test_browser.py": ("site/build.py", "runs", (),
                                   "its `browser` stage runs it; pages.yml runs every stage"),
}
# Test files that no scenario or workflow runs, each with its reviewed reason.
NOT_RUN = {
    "integrations/mcp/test_server.py":
        "manual: needs the pinned MCP SDK and ZENO_FCIS_CLI, as docs/LLM_SYNTHESIS.md shows",
    "integrations/elite_archive/tests/test_archive.py":
        "manual, as integrations/elite_archive/README.md shows; wiring it into CI is separate pending work",
}
# A workflow must run every scenario, or scenario commands are not CI runs.
ATDD_RUN_ALL = ["python3", "tools/atdd.py", "run", "--all"]


def python_runs(tokens):
    """(script, arguments) for each `python3 [options] FILE ...` in one token
    list, and ("discover", arguments) for each `python3 -m unittest discover`."""
    runs, segment = [], []
    for token in [*tokens, ";"]:
        if token not in ("&&", "||", ";", "|"):
            segment.append(token)
            continue
        starts = [index for index, part in enumerate(segment) if os.path.basename(part) in ("python", "python3")]
        rest = segment[starts[0] + 1:] if starts else []
        while rest and rest[0].startswith("-") and rest[0] not in ("-m", "-c", "-"):
            rest = rest[2:] if rest[0] in ("-W", "-X") else rest[1:]
        if rest[:3] == ["-m", "unittest", "discover"]:
            runs.append(("discover", rest[3:]))
        elif rest and rest[0].endswith(".py"):
            runs.append((rest[0].removeprefix("$GITHUB_WORKSPACE/").removeprefix("./"), rest[1:]))
        segment = []
    return runs


def discovered(root, arguments):
    """The files `python3 -m unittest discover ARGUMENTS` loads from its start directory."""
    named, positional, index = {}, [], 0
    while index < len(arguments):
        if arguments[index] in ("-s", "--start-directory", "-p", "--pattern", "-t", "--top-level-directory"):
            named[arguments[index].lstrip("-")[0]] = arguments[index + 1]
            index += 2
            continue
        if not arguments[index].startswith("-"):
            positional.append(arguments[index])
        index += 1
    start = named.get("s", positional[0] if positional else ".")
    pattern = named.get("p", positional[1] if len(positional) > 1 else "test*.py")
    return {str(path.relative_to(root)) for path in (root / start).glob(pattern) if path.is_file()}


def uncommented(line):
    """A YAML line without its comment: `#` at the start or after a space, outside quotes."""
    quote = None
    for index, character in enumerate(line):
        if quote:
            quote = None if character == quote else quote
        elif character in "'\"":
            quote = character
        elif character == "#" and (index == 0 or line[index - 1] in " \t"):
            return line[:index].rstrip()
    return line.rstrip()


SCALAR_PAIR = re.compile(r"\s*([A-Za-z_][\w-]*):\s+(?:([^\s'\"{}\[\]&*!|>%@`][^'\"{}\[\]]*?)"
                         r"|\"([^\"\\]*)\"|'([^']*)')\s*")


def flow_mapping(body):
    """The `key: value, ...` of a one-line flow mapping of plain or quoted scalars."""
    mapping = {}
    for item in body.split(","):
        pair = SCALAR_PAIR.fullmatch(item)
        if not pair or pair[1] in mapping:
            raise ValueError(f"unsupported matrix row: {{{body}}}")
        mapping[pair[1]] = next(value for value in pair.groups()[1:] if value is not None)
    return mapping


def matrix_rows(text):
    """The rows of a workflow's one `matrix:` block, read without a YAML parser:
    one-line `- {key: value, ...}` rows under `include:`, minus every row an
    `exclude:` row matches. Comments are ignored and any other shape is refused,
    not guessed. GitHub applies `exclude` before `include`, so subtracting can
    only report a file that still runs, never count one that does not."""
    lines = [uncommented(line) for line in text.splitlines()]
    starts = [index for index, line in enumerate(lines) if re.match(r"\s*matrix:", line)]
    if len(starts) != 1 or lines[starts[0]].strip() != "matrix:":
        raise ValueError("expected exactly one `matrix:` block whose rows follow on their own lines")
    indent = len(lines[starts[0]]) - len(lines[starts[0]].lstrip())
    rows, section, depth_of_section = {"include": [], "exclude": []}, None, 0
    for line in lines[starts[0] + 1:]:
        if not line.strip():
            continue
        depth = len(line) - len(line.lstrip())
        if depth <= indent:
            break
        key, row = re.fullmatch(r"\s*(include|exclude):", line), re.fullmatch(r"\s*-\s+\{(.*)\}", line)
        if key:
            section, depth_of_section = key[1], depth
        elif row and section and depth >= depth_of_section:
            rows[section].append(flow_mapping(row[1]))
        else:
            raise ValueError(f"unsupported matrix line: {line.strip()}")
    return [row for row in rows["include"] if not any(excluded.items() <= row.items() for excluded in rows["exclude"])]


def workflow_commands(text):
    """Token lists of every shell line in a workflow's `run:` steps. `${NAME}`
    expands over the matrix rows when an uncommented `NAME: ${{ matrix.KEY }}`
    line binds it."""
    bound = dict(re.findall(r"^[ \t]*(\w+):[ \t]*\$\{\{\s*matrix\.(\w+)\s*\}\}[ \t]*(?:#.*)?$", text, re.M))
    rows = None
    lines, commands, index = text.splitlines(), [], 0
    while index < len(lines):
        line, index = lines[index], index + 1
        match = re.match(r"\s*(?:-\s+)?run:\s*(.*)$", line)
        if not match:
            continue
        column, body = line.index("run:"), [match[1].strip()]
        if body[0][:1] in ("|", ">"):
            folded, body = body[0][:1] == ">", []
            while index < len(lines) and (not lines[index].strip()
                                          or len(lines[index]) - len(lines[index].lstrip()) > column):
                body.append(lines[index].strip())
                index += 1
            body = [" ".join(body)] if folded else body
        for shell in re.sub(r"\\\n", " ", "\n".join(body)).splitlines():
            try:
                tokens = shlex.split(shell, comments=True)
            except ValueError:
                continue  # here-document data, not a command
            expanded = [tokens]
            for name, key in bound.items():
                if any(f"${{{name}}}" in token for token in tokens):
                    rows = matrix_rows(text) if rows is None else rows
                    values = [row[key] for row in rows if key in row]
                    expanded = [[token.replace(f"${{{name}}}", value) for token in each]
                                for each in expanded for value in values]
            commands += expanded
    return commands


def workflow_lines(root):
    commands = []
    for path in sorted([*(root / ".github/workflows").glob("*.yml"), *(root / ".github/actions").glob("*/action.yml")]):
        commands += workflow_commands(path.read_text(encoding="utf-8"))
    return commands


def every_command(root):
    commands = [list(command) for scenario in atdd.SCENARIOS.values() for command in scenario.commands]
    return commands + workflow_lines(root)


def k_filter(argument):
    """unittest's `-k` filter: separate (`-k NAME`), attached (`-kNAME`) or
    grouped with other short options (`-vk NAME`)."""
    return argument.startswith("-") and not argument.startswith("--") and "k" in argument[1:]


def selects_all(arguments):
    """No later positional argument or `-k` filter selects only some of a file's tests."""
    return all(argument.startswith("-") and not k_filter(argument) for argument in arguments)


# unittest.main() options that neither select tests nor let failures pass.
MAIN_OPTIONS = {"verbosity", "failfast", "buffer", "catchbreak", "warnings", "tb_locals", "durations"}


def runs_all_tests(path):
    """Whether `python3 PATH` runs every test case: its `if __name__ == "__main__":`
    block calls unittest.main() with no positional argument and no option that
    selects tests or hides failures (defaultTest, argv, module, testLoader,
    testRunner, exit)."""
    try:
        tree = ast.parse(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, SyntaxError):
        return False
    return any(isinstance(statement, ast.Expr) and isinstance(statement.value, ast.Call)
               and ast.unparse(statement.value.func) == "unittest.main" and not statement.value.args
               and all(keyword.arg in MAIN_OPTIONS for keyword in statement.value.keywords)
               for node in tree.body
               if isinstance(node, ast.If) and ast.unparse(node.test) == "__name__ == '__main__'"
               for statement in node.body)


def runs_of(root, commands):
    """Test files some command runs whole, and every (script, arguments) any command runs."""
    whole, invoked = set(), set()
    for tokens in commands:
        for script, arguments in python_runs(tokens):
            if script == "discover":
                if not any(map(k_filter, arguments)):
                    whole |= discovered(root, arguments)
                continue
            invoked.add((script, tuple(arguments)))
            # A later positional argument or a `-k` filter selects only some of a test
            # file's tests, and only an unrestricted unittest.main() runs them all.
            if selects_all(arguments) and runs_all_tests(root / script):
                whole.add(script)
    return whole, invoked


def case_classes(path):
    tree = ast.parse(path.read_text(encoding="utf-8"))
    return {node.name for node in tree.body if isinstance(node, ast.ClassDef) and any(
        (base.attr if isinstance(base, ast.Attribute) else getattr(base, "id", None)) == "TestCase"
        for base in node.bases)}


def imported(path, module):
    tree = ast.parse(path.read_text(encoding="utf-8"))
    return {alias.name for node in tree.body if isinstance(node, ast.ImportFrom) and node.module == module
            for alias in node.names}


def repository_tests(root):
    """Every test_*.py git tracks or would track: the index plus untracked files it does not ignore."""
    listed = subprocess.run(["git", "--no-optional-locks", "ls-files", "--cached", "--others", "--exclude-standard",
                             "-z"], cwd=root, check=True, capture_output=True).stdout.decode().split("\0")
    return sorted({name for name in listed
                   if Path(name).name.startswith("test_") and name.endswith(".py") and (root / name).is_file()})


def registry_problems(root, commands, inside=RUN_INSIDE, not_run=NOT_RUN, workflows=None):
    """Why a test file may silently not run; an empty list means none can.
    `workflows`, when given, are the workflow command lines."""
    tests = repository_tests(root)
    whole, invoked = runs_of(root, commands)
    problems = [f"{name}: listed, but there is no such test file"
                for name in sorted((set(inside) | set(not_run)) - set(tests))]
    problems += [f"{name}: listed both as run inside a host and as not run" for name in sorted(set(inside) & set(not_run))]
    if workflows is not None and ATDD_RUN_ALL not in workflows:
        problems.append(f"no workflow runs `{shlex.join(ATDD_RUN_ALL)}`, so no scenario command is a CI run")
    for name in tests:
        if name in whole:
            if name in inside or name in not_run:
                problems.append(f"{name}: runs whole in a scenario or workflow; remove its registry entry")
        elif name in inside:
            host, kind, prefix, _ = inside[name]
            if kind == "runs":
                listed = (any(script == host and arguments[:len(prefix)] == prefix for script, arguments in invoked)
                          and Path(name).name in (root / host).read_text(encoding="utf-8"))
            else:
                cases = case_classes(root / name)
                listed = host in whole and bool(cases) and cases <= imported(root / host, Path(name).stem)
            if not listed:
                problems.append(f"{name}: {host} does not {kind.removesuffix('s')} it as listed")
        elif name not in not_run:
            if any(script == name and selects_all(arguments) for script, arguments in invoked):
                problems.append(f"{name}: a command runs it directly, but its `__main__` block does not call an "
                                "unrestricted unittest.main()")
            else:
                problems.append(f"{name}: no ATDD scenario or workflow runs it; wire it, or list it in NOT_RUN "
                                "with a reviewed reason")
    return problems


PLANTED_TEST = ("import unittest\n\n\nclass Cases(unittest.TestCase):\n    def test_case(self):\n        pass\n\n\n"
                "if __name__ == '__main__':\n    unittest.main()\n")
RUNS_EVERY_SCENARIO = "jobs:\n  acceptance:\n    steps:\n      - run: python3 tools/atdd.py run --all\n"


def plant(root, files):
    for name, text in files.items():
        (root / name).parent.mkdir(parents=True, exist_ok=True)
        (root / name).write_text(text)


# Planted gaps, one per reviewed hole, each in a fresh git work tree: the files a
# case writes, its commands and workflows, the problems the registry must report,
# and the files no problem may name.
def planted_disabled_matrix_rows(root):
    """A commented-out matrix row and an excluded one are not runs."""
    plant(root, {f"tools/test_{name}.py": PLANTED_TEST for name in ("alpha", "beta", "gamma")})
    workflow = textwrap.dedent("""\
        jobs:
          check:
            strategy:
              matrix:
                include:
                  - {gate: one, script: alpha.py}  # the only row that runs
                  # - {gate: two, script: beta.py}
                  - {gate: three, script: gamma.py}
                exclude:
                  - {gate: three}
            env:
              SCRIPT: ${{ matrix.script }}
            steps:
              - run: python3 "tools/test_${SCRIPT}"
        """)
    return {"commands": [], "workflows": [workflow, RUNS_EVERY_SCENARIO], "inside": {}, "not_run": {},
            "refused": ["tools/test_beta.py: no ATDD scenario", "tools/test_gamma.py: no ATDD scenario"],
            "kept": ["tools/test_alpha.py"]}


def planted_host_without_its_arguments(root):
    """A host run without the entry's leading arguments does not run the file."""
    plant(root, {"tools/test_beta.py": PLANTED_TEST, "tools/test_delta.py": PLANTED_TEST,
                 "tools/host.py": "SUITES = {'self-test': 'test_beta.py', 'check': 'test_delta.py'}\n"})
    return {"commands": [["python3", "tools/host.py", "check", "--all"]], "workflows": [RUNS_EVERY_SCENARIO],
            "inside": {"tools/test_beta.py": ("tools/host.py", "runs", ("self-test",), ""),
                       "tools/test_delta.py": ("tools/host.py", "runs", ("check",), "")},
            "not_run": {}, "refused": ["tools/test_beta.py: tools/host.py does not run it as listed"],
            "kept": ["tools/test_delta.py"]}


def planted_k_filters(root):
    """A `-k` filter, attached, grouped or given to discover, runs only part of a file."""
    plant(root, {**{f"tools/test_{name}.py": PLANTED_TEST for name in ("alpha", "beta", "gamma")},
                 "tools/test_data/apps/test_delta.py": PLANTED_TEST})
    return {"commands": [["python3", "tools/test_alpha.py", "-kfast"],
                         ["python3", "tools/test_beta.py", "-v", "-bkfast"],
                         ["python3", "-m", "unittest", "discover", "-s", "tools/test_data/apps", "-p", "test_*.py",
                          "-k", "fast"],
                         ["python3", "tools/test_gamma.py", "-v"]],
            "workflows": [RUNS_EVERY_SCENARIO], "inside": {}, "not_run": {},
            "refused": ["tools/test_alpha.py: no ATDD scenario", "tools/test_beta.py: no ATDD scenario",
                        "tools/test_data/apps/test_delta.py: no ATDD scenario"],
            "kept": ["tools/test_gamma.py"]}


def planted_restricted_mains(root):
    """A file run directly runs all its tests only through an unrestricted unittest.main()."""
    cases = PLANTED_TEST.split("\n\n\nif __name__", 1)[0] + "\n"
    mains = {"alpha": "", "beta": "unittest.main(defaultTest='Cases')", "gamma": "unittest.main(argv=['x', 'Cases'])",
             "epsilon": "unittest.main(exit=False)"}
    plant(root, {f"tools/test_{name}.py": cases + (f"\n\nif __name__ == '__main__':\n    {main}\n" if main else "")
                 for name, main in mains.items()})
    plant(root, {"tools/test_delta.py": PLANTED_TEST})
    return {"commands": [["python3", f"tools/test_{name}.py"] for name in [*mains, "delta"]],
            "workflows": [RUNS_EVERY_SCENARIO], "inside": {}, "not_run": {},
            "refused": [f"tools/test_{name}.py: a command runs it directly, but its `__main__` block"
                        for name in mains],
            "kept": ["tools/test_delta.py"]}


def planted_no_full_scenario_run(root):
    """Scenario commands are CI runs only while a workflow runs every scenario."""
    plant(root, {"tools/test_alpha.py": PLANTED_TEST})
    workflow = textwrap.dedent("""\
        jobs:
          acceptance:
            steps:
              - run: python3 tools/atdd.py self-test
              - run: python3 tools/atdd.py run --scenario smoke
        """)
    return {"commands": [["python3", "tools/test_alpha.py"]], "workflows": [workflow], "inside": {}, "not_run": {},
            "refused": ["no workflow runs `python3 tools/atdd.py run --all`"], "kept": ["tools/test_alpha.py"]}


def planted_tests_outside_tools(root):
    """Every test file git would track is in scope, not only those under tools/; ignored ones are not."""
    plant(root, {"tools/test_alpha.py": PLANTED_TEST, "integrations/demo/test_orphan.py": PLANTED_TEST,
                 "site/tests/test_page.py": PLANTED_TEST, ".gitignore": "generated/\n",
                 "generated/test_output.py": PLANTED_TEST})
    return {"commands": [["python3", "tools/test_alpha.py"]], "workflows": [RUNS_EVERY_SCENARIO], "inside": {},
            "not_run": {}, "refused": ["integrations/demo/test_orphan.py: no ATDD scenario",
                                       "site/tests/test_page.py: no ATDD scenario"],
            "kept": ["tools/test_alpha.py", "generated/test_output.py"]}


PLANTED = (planted_disabled_matrix_rows, planted_host_without_its_arguments, planted_k_filters,
           planted_restricted_mains, planted_no_full_scenario_run, planted_tests_outside_tools)


def planted_problems(root, case):
    """The registry's report for one planted case."""
    lines = [line for text in case["workflows"] for line in workflow_commands(text)]
    return "\n".join(registry_problems(root, [*case["commands"], *lines], case["inside"], case["not_run"],
                                       workflows=lines))


class TestFileRegistryTests(unittest.TestCase):
    def test_every_tool_test_runs_in_a_scenario_or_workflow(self):
        commands, workflows = every_command(atdd.ROOT), workflow_lines(atdd.ROOT)
        self.assertEqual(registry_problems(atdd.ROOT, commands, workflows=workflows), [])
        whole, _ = runs_of(atdd.ROOT, commands)
        # One run of each kind: an ATDD argv, a workflow discover, and a matrix-expanded verus.yml step.
        self.assertTrue({"tools/test_check_core_app.py", "tools/test_evaluator_identity.py",
                         "tools/test_check_account_graph_v2.py"} <= whole)

    def test_the_test_registry_refuses_planted_gaps(self):
        workflow = textwrap.dedent("""\
            on:
              pull_request:
                paths: ["tools/test_beta.py"]
            jobs:
              check:
                strategy:
                  matrix:
                    include:
                      - {gate: one, script: alpha.py}
                env:
                  SCRIPT: ${{ matrix.script }}
                steps:
                  - run: |
                      python3 -B "$GITHUB_WORKSPACE/tools/host.py" --mode x \\
                        --flag
                    env:
                      UNRELATED: python3 tools/test_beta.py
                  - run: python3 "tools/test_${SCRIPT}"
            """)
        main = "\nif __name__ == '__main__':\n    unittest.main()\n"
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-test-registry-") as raw:
            root = Path(raw)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            for name in ("tools/test_alpha.py", "tools/test_beta.py", "tools/test_gamma.py",
                         "tools/test_data/apps/test_delta.py"):
                (root / name).parent.mkdir(parents=True, exist_ok=True)
                (root / name).write_text("import unittest\nclass Cases(unittest.TestCase):\n    pass\n" + main)
            (root / "tools/host.py").write_text("SUITE = 'test_beta.py'\n")
            (root / "tools/test_gamma.py").write_text("import unittest\nfrom test_alpha import Cases\n" + main)
            inside = {"tools/test_beta.py": ("tools/host.py", "runs", ("--mode", "x"), "")}
            host_run, alpha_run = workflow_commands(workflow)
            discover = ["python3", "-m", "unittest", "discover", "-s", "tools/test_data/apps", "-p", "test_*.py"]
            gamma_run = ["python3", "tools/test_gamma.py"]
            commands = [host_run, alpha_run, discover, gamma_run]
            self.assertEqual(registry_problems(root, commands, inside, {}), [])

            def problems(commands=commands, inside=inside, not_run=None):
                return "\n".join(registry_problems(root, commands, inside, not_run or {}))

            # A path filter or an environment value is not a run, so without the
            # matrix step the file it expands to is reported.
            self.assertIn("tools/test_alpha.py: no ATDD scenario or workflow runs it",
                          problems([host_run, discover, gamma_run]))
            self.assertIn("tools/test_beta.py: tools/host.py does not run it", problems([alpha_run, discover, gamma_run]))
            # A filter after the file runs only part of it.
            self.assertIn("tools/test_gamma.py: no ATDD",
                          problems([host_run, alpha_run, discover, [*gamma_run, "Cases"]]))
            self.assertIn("tools/test_data/apps/test_delta.py: no ATDD", problems([host_run, alpha_run, gamma_run]))
            # Entries stay exact: stale and unknown entries are reported.
            self.assertIn("tools/test_gamma.py: runs whole", problems(not_run={"tools/test_gamma.py": "stale"}))
            self.assertIn("tools/test_missing.py: listed, but there is no such test file",
                          problems(not_run={"tools/test_missing.py": "gone"}))
            # Run inside a host only while the host still runs or imports it.
            imports = {**inside, "tools/test_alpha.py": ("tools/test_gamma.py", "imports", (), "Cases")}
            self.assertEqual(problems([host_run, discover, gamma_run], imports), "")
            (root / "tools/host.py").write_text("SUITE = 'other.py'\n")
            self.assertIn("tools/test_beta.py: tools/host.py does not run it", problems())
            (root / "tools/test_gamma.py").write_text("import unittest\nfrom test_alpha import helper\n" + main)
            self.assertIn("tools/test_alpha.py: tools/test_gamma.py does not import it",
                          problems([host_run, discover, gamma_run], imports))

    def test_the_test_registry_refuses_each_reviewed_gap(self):
        for planted in PLANTED:
            with self.subTest(planted.__name__), \
                    tempfile.TemporaryDirectory(prefix="zeno-fcis-test-registry-") as raw:
                root = Path(raw)
                subprocess.run(["git", "init", "-q"], cwd=root, check=True)
                case = planted(root)
                problems = planted_problems(root, case)
                for refused in case["refused"]:
                    self.assertIn(refused, problems)
                for kept in case["kept"]:
                    self.assertNotIn(kept, problems)
                # Wiring the reported gap the honest way leaves nothing to report.
                if planted is planted_no_full_scenario_run:
                    case["workflows"].append(RUNS_EVERY_SCENARIO)
                    self.assertEqual(planted_problems(root, case), "")

    def test_the_matrix_reader_refuses_shapes_it_does_not_read(self):
        def workflow(matrix):
            return ("jobs:\n  check:\n    strategy:\n      matrix:\n" + textwrap.indent(matrix, " " * 8)
                    + "    env:\n      SCRIPT: ${{ matrix.script }}\n    steps:\n"
                    "      - run: python3 \"tools/test_${SCRIPT}\"\n")

        self.assertEqual(workflow_commands(workflow("include:\n  - {gate: one, script: 'a.py'}  # quoted\n")),
                         [["python3", "tools/test_a.py"]])
        for unread in ("include:\n  - gate: one\n    script: a.py\n",       # a block mapping row
                       "script: [a.py, b.py]\n",                            # a matrix variable
                       'include: [{"gate": "one", "script": "a.py"}]\n',   # a flow sequence
                       "include:\n  - {gate: one, script: {nested: a.py}}\n",
                       "include:\n  - {gate: one, script: a.py, script: b.py}\n"):
            with self.subTest(unread):
                with self.assertRaises(ValueError):
                    workflow_commands(workflow(unread))
        two = workflow("include:\n  - {script: a.py}\n").replace("jobs:\n", "jobs:\n  other:\n    strategy:\n"
                                                                 "      matrix:\n        include: []\n", 1)
        with self.assertRaises(ValueError):
            workflow_commands(two)
        # The matrix is read only when a bound name is used, so other shapes elsewhere stay unread.
        self.assertEqual(workflow_commands("jobs:\n  check:\n    strategy:\n      matrix:\n        include: [\n"
                                           "          {\"group\": \"one\"}]\n    steps:\n      - run: python3 x.py\n"),
                         [["python3", "x.py"]])


if __name__ == "__main__":
    unittest.main()
