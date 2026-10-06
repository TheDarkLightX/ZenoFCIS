"""Guard the repaired gate bindings and refusal of successful empty test runs."""

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
TEMPLATE_CORPORA = [("COUNTER_CORPUS", 64), ("STOCK_CORPUS", 864), ("ORDER_CORPUS", 1728),
                    ("ACCOUNT_CORPUS", 13_122), ("WITHDRAWAL_CORPUS", 1_296_000),
                    ("TREASURY_PROPOSALS", 103_680), ("TREASURY_CALLBACKS", 677_376),
                    ("TREASURY_GUARDS", 82_944)]


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
    bounded = body("miri_bounded_template_domain_profiles")
    if "check_observations(&observed, true);" not in bounded:
        problems.append("the bounded profile must check its observations")
    pinned = re.search(r"assert_eq!\(counts, \[([\d, ]*)\]\);", bounded)
    cases = []
    for const, size in TEMPLATE_CORPORA:
        block = re.search(r"^const " + const + r": Corpus = Corpus \{\n(.*?)^\};", source, re.M | re.S)
        if block is None or "&" + const not in bounded:
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
        cases.append(len(profile))
    if pinned is None or [int(n) for n in pinned.group(1).split(",") if n.strip()] != cases:
        problems.append(f"bounded counts must be pinned as {cases}")
    if re.search(r"cfg!?\(\s*(?:not\(\s*)?miri|cfg_attr\(\s*(?:not\(\s*)?miri|env::var", source):
        problems.append("template tests must not branch on Miri or the environment")
    return problems


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

    def test_miri_split_targets_run_each_listed_test_exactly_once(self):
        source = (atdd.ROOT / ".github/workflows/miri.yml").read_text()
        rows = miri_rows(source)
        library = [row for row in rows if row.get("target") == "--lib"]
        remainder = [row for row in library if "test" not in row]
        isolated = [row["test"] for row in library if "test" in row]
        self.assertEqual([row["group"] for row in remainder], ["synthesis-library"])
        self.assertEqual(len(isolated), 6)
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

    def test_miri_template_runs_bounded_profile_and_skips_only_native_corpora(self):
        source = (atdd.ROOT / ".github/workflows/miri.yml").read_text()
        rows = miri_rows(source)
        template = next(row for row in rows if row.get("target") == "--test v2_template_contracts")
        native_only = list(TEMPLATE_NATIVE_ONLY)
        self.assertEqual(template["miri_skip"], native_only)
        entries = miri_exclusions.load_exclusions()
        self.assertEqual([e["test"] for e in entries if e["group"] == template["group"]], native_only)
        self.assertEqual(len(entries), 5)
        rust = (atdd.ROOT / TEMPLATE_TESTS).read_text()
        self.assertEqual(template_profile_problems(rust), [])
        sum_native = sum(size for _, size in TEMPLATE_CORPORA)
        self.assertEqual(sum_native, 2_175_778)

        inventory = sorted(re.findall(r"#\[test\]\s*(?:#\[ignore[^\n]*\n\s*)?fn (\w+)\(", rust))
        self.assertEqual(len(inventory), 9)
        target = "--test v2_template_contracts"
        inventories = {target: inventory}
        ignored = {target: ["emit_library_policy_artifacts"]}
        arguments = miri_exclusions.miri_test_arguments(template)

        def coverage(row=template, workflow=source, args=arguments, names=inventories, skip=True, ignore=ignored):
            return run_miri_coverage_check(workflow, row, names, honour_skip=skip, ignored=ignore, test_args=args)
        self.assertEqual(coverage(), "")

        # Planted controls: each must be refused by the workflow's own check.
        def planted(row):
            changed = source.replace(json.dumps(template), json.dumps(row), 1)
            self.assertNotEqual(changed, source)
            return changed
        wrong = dict(template, miri_skip=native_only[:3] + ["retained_complete_examples_genesis_and_replay"])
        undeclared = dict(template, miri_skip=native_only + ["policy_schema_law_and_meter_mutations_refuse"])
        absent = {k: v for k, v in template.items() if k != "miri_skip"}
        for row in (wrong, undeclared, absent):
            with self.assertRaisesRegex(AssertionError, "template native-only workloads differ"):
                coverage(row, planted(row), miri_exclusions.miri_test_arguments(row))
            with self.assertRaises(miri_exclusions.ExclusionError):
                miri_exclusions.check_static(atdd.ROOT, entries, miri_rows(planted(row)))
        for args in ("", arguments + " --skip miri_bounded_template_domain_profiles",
                     arguments.replace("--exact ", "")):
            with self.assertRaisesRegex(AssertionError, "skip exactly the native-only corpora"):
                coverage(args=args)
        with self.assertRaisesRegex(AssertionError, "template Miri selection must run"):
            coverage(skip=False)
        missing = {target: [n for n in inventory if n != "miri_bounded_template_domain_profiles"]}
        with self.assertRaisesRegex(AssertionError, "template semantic inventory differs"):
            coverage(names=missing)
        with self.assertRaisesRegex(AssertionError, "must not be ignored"):
            coverage(ignore={target: ["emit_library_policy_artifacts", "miri_bounded_template_domain_profiles"]})
        split = source.replace(json.dumps(template) + ",\n",
                               json.dumps(template) + ",\n          " + json.dumps(
                                   {"group": "synthesis-v2-template-bounded", "packages": "-p zeno-fcis-synthesis",
                                    "target": target, "test": "miri_bounded_template_domain_profiles"}) + ",\n", 1)
        self.assertNotEqual(split, source)
        with self.assertRaisesRegex(AssertionError, "split target cannot carry Miri exclusions"):
            coverage(workflow=split)

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
        self.assertTrue(problems("[5, 7, 11, 9, 10, 12, 22, 16]", "[5, 7, 11, 9, 9, 12, 22, 16]"))
        self.assertTrue(problems("        &[0, 180, 0, 181, 1, 0, 0, 170, 162, 170, 1, 193, 0],\n", ""))
        self.assertTrue(problems("check_observations(&observed, true);", ""))
        self.assertTrue(problems("#[test]\nfn miri_bounded_template_domain_profiles", "#[test]\n#[ignore]\nfn miri_bounded_template_domain_profiles"))
        self.assertTrue(problems("fn miri_bounded_template_domain_profiles() {\n",
                                 "fn miri_bounded_template_domain_profiles() {\n    if cfg!(miri) {}\n"))

    def test_current_authority_is_bound_to_real_admission_and_public_api_tests(self):
        commands = atdd.SCENARIOS["production-authority"].commands
        targets = [c[i + 1] for c in commands for i, a in enumerate(c) if a == "--test"]
        self.assertCountEqual(targets, ["v2_schema_binding", "v2_authority", "v2_catalog", "program_api"])
        self.assertTrue(any("finite::execution_v2::catalog::tests::" in c for c in commands))
        self.assertTrue(any("finite::execution_v2::continuation::tests::" in c
                            for c in atdd.SCENARIOS["bounded-completion"].commands))


if __name__ == "__main__":
    unittest.main()
