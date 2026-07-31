#!/usr/bin/env python3
"""Validate and execute the closed RC3 acceptance-scenario registry."""

from __future__ import annotations

import argparse
import os
import re
import shlex
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FEATURE_ROOT = ROOT / "acceptance" / "features"
ATDD_TAG = re.compile(r"@atdd-([a-z0-9]+(?:-[a-z0-9]+)*)")
ATDD_TAG_LINE = re.compile(r"^@atdd-([a-z0-9]+(?:-[a-z0-9]+)*)$")
SCENARIO = re.compile(r"^\s*Scenario:\s*(\S.*)$")
OTHER_GHERKIN_DECLARATION = re.compile(
    r"^\s*(?:Feature|Rule|Background|Scenario Outline|Scenario Template|Examples):"
)


class AcceptanceError(ValueError):
    """Raised when acceptance requirements or execution fail closed."""


@dataclass(frozen=True)
class AcceptanceScenario:
    """One reviewable acceptance scenario with fixed executable commands."""

    title: str
    commands: tuple[tuple[str, ...], ...]


SCENARIOS: dict[str, AcceptanceScenario] = {
    "minimal-core": AcceptanceScenario(
        "Run the immutable functional core example",
        (
            (
                "cargo",
                "+1.97.1",
                "run",
                "-p",
                "zeno-fcis",
                "--example",
                "minimal_core",
                "--locked",
            ),
        ),
    ),
    "checked-backend": AcceptanceScenario(
        "Construct a tool-neutral checked backend request",
        (
            (
                "cargo",
                "+1.97.1",
                "run",
                "-p",
                "zeno-fcis",
                "--example",
                "checked_backend",
                "--features",
                "backend",
                "--locked",
            ),
        ),
    ),
    "external-consumer": AcceptanceScenario(
        "Compile an isolated downstream consumer",
        (
            (
                "cargo",
                "+1.97.1",
                "check",
                "--manifest-path",
                "fixtures/external-consumer/Cargo.toml",
                "--locked",
            ),
        ),
    ),
    "project-bootstrap": AcceptanceScenario(
        "Generate a reviewable project starter",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-bootstrap", "--locked"),),
    ),
    "composed-program": AcceptanceScenario(
        "Execute fixed domain machines through one global composition",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-composed-program", "--locked"),),
    ),
    "production-authority": AcceptanceScenario(
        "Admit only catalog and invocation bound transitions",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-authority", "--locked"),),
    ),
    "sqlite-authority": AcceptanceScenario(
        "Persist an authorized transition and its exact outbox obligations",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-shell-sqlite", "--locked"),),
    ),
    "release-contract": AcceptanceScenario(
        "Run the local RC3 release gate",
        (
            ("python3", "tools/check_assurance.py", "--self-test"),
            ("python3", "tools/check_assurance.py"),
            ("python3", "tools/check_library_docs.py"),
            ("cargo", "+1.97.1", "fmt", "--all", "--", "--check"),
            (
                "cargo",
                "+1.97.1",
                "clippy",
                "--workspace",
                "--all-targets",
                "--all-features",
                "--locked",
                "--",
                "-D",
                "warnings",
            ),
            (
                "cargo",
                "+1.97.1",
                "test",
                "--workspace",
                "--all-features",
                "--locked",
            ),
            (
                "cargo",
                "+1.97.1",
                "test",
                "--workspace",
                "--doc",
                "--all-features",
                "--locked",
            ),
            (
                "cargo",
                "+1.97.1",
                "doc",
                "--workspace",
                "--all-features",
                "--locked",
                "--no-deps",
            ),
            ("python3", "tools/rc_package.py", "self-test"),
            ("python3", "tools/rc_package.py", "check"),
        ),
    ),
    "probity-guardrails": AcceptanceScenario(
        "Reject unsafe agent workflow actions deterministically",
        (("python3", "tools/check_probity.py"),),
    ),
    "security-hotspots": AcceptanceScenario(
        "Rank security hotspots without interpreting source as instructions",
        (
            ("python3", "tools/security_hotspots.py", "self-test"),
            ("python3", "tools/security_hotspots.py", "check"),
        ),
    ),
    "rc3-project-new": AcceptanceScenario(
        "Create a bounded project without overwriting files",
        (
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-cli",
                "--locked",
                "rc3_cli_new_refuses_existing_content",
            ),
        ),
    ),
    "rc3-mini-os-check": AcceptanceScenario(
        "Check the Mini Determinator project in one command",
        (
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-cli",
                "--locked",
                "rc3_cli_mini_determinator_check",
            ),
        ),
    ),
    "rc3-spec-canonical": AcceptanceScenario(
        "Produce identical typed AST bytes from equivalent source",
        (
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-spec",
                "--locked",
                "rc3_spec_canonical",
            ),
        ),
    ),
    "rc3-composition-diagnostics": AcceptanceScenario(
        "Report composition blockers completely and deterministically",
        (
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-spec",
                "--locked",
                "rc3_composition_diagnostics",
            ),
        ),
    ),
    "rc3-mini-os-replay": AcceptanceScenario(
        "Replay shared-nothing coordination independently of completion order",
        (
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-spec",
                "--locked",
                "rc3_mini_os_replay",
            ),
        ),
    ),
    "rc3-mini-os-conflict": AcceptanceScenario(
        "Reject conflicting private workspace merges without authority change",
        (
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-spec",
                "--locked",
                "rc3_mini_os_conflict",
            ),
        ),
    ),
    "rc3-temporal-modes": AcceptanceScenario(
        "Keep finite execution and unbounded proof obligations distinct",
        (
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-spec",
                "--locked",
                "rc3_temporal_modes",
            ),
        ),
    ),
    "rc3-formal-tools": AcceptanceScenario(
        "Bind formal output to the exact claim, runtime, and checked arithmetic",
        (
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-formal-tools",
                "--locked",
                "rc3_formal_tools_translation_parity",
            ),
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-formal-tools",
                "--locked",
                "rc3_run_record_encoding_is_injective_and_complete",
            ),
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-cli",
                "--locked",
                "rc3_cli_computes_a_portable_lean_inventory_for_tools_v2",
            ),
        ),
    ),
    "rc3-formal-fail-closed": AcceptanceScenario(
        "Block hostile formal outcomes and replay models before refutation",
        (
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-formal-tools",
                "--locked",
                "rc3_formal_fail_closed_and_model_replay",
            ),
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-formal-tools",
                "--locked",
                "rc3_smt_followup_cannot_contradict_the_decision",
            ),
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-cli",
                "--locked",
                "rc3_cli_formal_outcomes_and_retention_are_process_level",
            ),
        ),
    ),
    "rc3-input-inert": AcceptanceScenario(
        "Keep shell traversal environment and instruction syntax inert",
        (
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-spec",
                "--locked",
                "rc3_input_inert",
            ),
        ),
    ),
    "rc3-derived-views": AcceptanceScenario(
        "Render deterministic diagnostic graphs and explanations only",
        (
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-spec",
                "--locked",
                "rc3_derived_views",
            ),
        ),
    ),
    "rc3-generated-drift": AcceptanceScenario(
        "Regenerate source and manifests reproducibly and detect drift",
        (
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-cli",
                "--locked",
                "rc3_cli_generate_check",
            ),
        ),
    ),
    "rc3-resource-envelopes": AcceptanceScenario(
        "Stop deep parsing, huge horizons, and oversized exports within fixed limits",
        (
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-spec",
                "--locked",
                "rc3_parser_",
            ),
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-spec",
                "--locked",
                "rc3_finite_horizon_is_bounded_during_elaboration",
            ),
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-formal-tools",
                "--locked",
                "rc3_formal_export_limits_fail_closed_before_rendering",
            ),
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-formal-tools",
                "--locked",
                "rc3_lean_render_budget_is_live",
            ),
        ),
    ),
    "rc3-process-boundary": AcceptanceScenario(
        "Bind timeout, solver names, and execution to exact checked bytes",
        (
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-formal-tools",
                "--locked",
                "rc3_process_timeout_includes_blocked_stdin_delivery",
            ),
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-formal-tools",
                "--locked",
                "rc3_smt_predicate_symbols_are_injective",
            ),
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-formal-tools",
                "--locked",
                "rc3_private_executable_preserves_the_admitted_bytes",
            ),
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-formal-tools",
                "--locked",
                "rc3_untrusted_special_files_and_post_enumeration_swaps_are_blocked",
            ),
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-formal-tools",
                "--locked",
                "rc3_process_success_kills_descendants_after_collecting_output",
            ),
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-formal-tools",
                "--locked",
                "rc3_lean_runtime_mutation_during_version_probe_is_blocked",
            ),
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-formal-tools",
                "--locked",
                "rc3_retention_publishes_only_complete_bundles",
            ),
        ),
    ),
    "rc3-cli-json-contract": AcceptanceScenario(
        "Return versioned deterministic CLI JSON for valid and invalid projects",
        (
            (
                "cargo",
                "+1.97.1",
                "test",
                "-p",
                "zeno-fcis-cli",
                "--locked",
                "rc3_cli_invalid_json_diagnostics",
            ),
        ),
    ),
    "rc3-package-binary-inventory": AcceptanceScenario(
        "Package every declared binary in one unique checked archive",
        (("python3", "tools/rc_package.py", "self-test"),),
    ),
}


def parse_feature_text(text: str, label: str) -> dict[str, tuple[int, str]]:
    """Return unique ATDD IDs, source lines, and scenario titles."""

    found: dict[str, tuple[int, str]] = {}
    pending: list[str] = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        stripped = line.strip()
        if stripped.startswith("@"):
            matches = ATDD_TAG.findall(stripped)
            if matches and ATDD_TAG_LINE.fullmatch(stripped) is None:
                raise AcceptanceError(
                    f"{label}:{line_number}: @atdd-* tag line contains hostile or extra syntax"
                )
            pending = matches
            continue
        match = SCENARIO.match(line)
        if match is not None:
            if len(pending) != 1:
                raise AcceptanceError(
                    f"{label}:{line_number}: scenario requires exactly one @atdd-* tag"
                )
            scenario_id = pending[0]
            if scenario_id in found:
                raise AcceptanceError(f"{label}:{line_number}: duplicate {scenario_id}")
            found[scenario_id] = (line_number, match.group(1))
            pending = []
            continue
        if pending and OTHER_GHERKIN_DECLARATION.match(line):
            raise AcceptanceError(
                f"{label}:{line_number}: @atdd-* tag must bind the next Scenario"
            )
    return found


def feature_scenarios() -> dict[str, tuple[Path, int, str]]:
    """Parse every feature and reject cross-file duplicate scenario IDs."""

    paths = sorted(FEATURE_ROOT.glob("*.feature"))
    if not paths:
        raise AcceptanceError("no acceptance feature files found")
    found: dict[str, tuple[Path, int, str]] = {}
    for path in paths:
        relative = path.relative_to(ROOT)
        parsed = parse_feature_text(path.read_text(encoding="utf-8"), str(relative))
        for scenario_id, (line_number, title) in parsed.items():
            if scenario_id in found:
                prior = found[scenario_id]
                raise AcceptanceError(
                    f"{relative}:{line_number}: duplicate {scenario_id}; first in {prior[0]}"
                )
            found[scenario_id] = (relative, line_number, title)
    return found


def validate_registry() -> dict[str, tuple[Path, int, str]]:
    """Require exact set and title equality between Gherkin and executable registry."""

    found = feature_scenarios()
    missing = sorted(set(SCENARIOS).difference(found))
    unknown = sorted(set(found).difference(SCENARIOS))
    if missing or unknown:
        raise AcceptanceError(
            f"acceptance registry mismatch: missing={missing}, unknown={unknown}"
        )
    for scenario_id, (_, _, title) in found.items():
        expected = SCENARIOS[scenario_id].title
        if title != expected:
            raise AcceptanceError(
                f"{scenario_id}: feature title {title!r} does not equal registry {expected!r}"
            )
    return found


def self_test() -> None:
    """Prove the parser rejects missing, duplicate, and hidden scenario bindings."""

    hostile = {
        "missing tag": "Feature: x\n  Scenario: unbound\n    Then no\n",
        "duplicate tag": (
            "Feature: x\n  @atdd-a\n  Scenario: first\n  @atdd-a\n  Scenario: second\n"
        ),
        "multiple tags": "Feature: x\n  @atdd-a @atdd-b\n  Scenario: ambiguous\n",
        "feature tag inheritance": ("@atdd-a\nFeature: x\n  Scenario: inherited\n"),
        "rule tag inheritance": (
            "Feature: x\n  @atdd-a\n  Rule: tagged rule\n    Scenario: inherited\n"
        ),
        "hostile tag syntax": "Feature: x\n  @atdd-a;touch-owned\n  Scenario: injected\n",
    }
    for label, text in hostile.items():
        try:
            parse_feature_text(text, label)
        except AcceptanceError:
            continue
        raise AcceptanceError(f"self-test mutation survived: {label}")

    with tempfile.TemporaryDirectory(prefix="zeno-fcis-atdd-") as directory:
        path = Path(directory) / "unknown.feature"
        path.write_text(
            "Feature: x\n  @atdd-hidden\n  Scenario: hidden\n",
            encoding="utf-8",
        )
        parsed = parse_feature_text(path.read_text(encoding="utf-8"), str(path))
        if set(parsed).issubset(SCENARIOS):
            raise AcceptanceError("self-test hidden scenario was not detected")

    commands_before = SCENARIOS["minimal-core"].commands
    parse_feature_text(
        "Feature: inert prose\n"
        "  @atdd-minimal-core\n"
        "  Scenario: Run the immutable functional core example\n"
        "    Given cargo publish && touch owned\n",
        "inert prose",
    )
    if SCENARIOS["minimal-core"].commands != commands_before:
        raise AcceptanceError("feature prose altered a fixed command binding")


def run_scenario(scenario_id: str) -> None:
    """Run one scenario's fixed commands without interpreting feature prose."""

    scenario = SCENARIOS[scenario_id]
    print(f"atdd: RUN {scenario_id}: {scenario.title}", flush=True)
    for command in scenario.commands:
        print(f"atdd: EXEC {shlex.join(command)}", flush=True)
        environment = None
        if command[0:3] == ("cargo", "+1.97.1", "doc"):
            environment = {**os.environ, "RUSTDOCFLAGS": "-D warnings"}
        subprocess.run(command, cwd=ROOT, check=True, env=environment)
    print(f"atdd: PASS {scenario_id}", flush=True)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    subcommands = parser.add_subparsers(dest="command", required=True)
    subcommands.add_parser("self-test")
    subcommands.add_parser("check")
    subcommands.add_parser("list")
    run = subcommands.add_parser("run")
    choice = run.add_mutually_exclusive_group(required=True)
    choice.add_argument("--all", action="store_true")
    choice.add_argument("--scenario", choices=sorted(SCENARIOS))
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        if args.command == "self-test":
            self_test()
            print("atdd: self-test PASS (8 hostile or inert-prose mutations checked)")
            return 0
        found = validate_registry()
        if args.command == "check":
            print(f"atdd: registry PASS ({len(found)} scenarios)")
            return 0
        if args.command == "list":
            for scenario_id in sorted(found):
                path, line, title = found[scenario_id]
                print(f"{scenario_id}\t{path}:{line}\t{title}")
            return 0
        selected = sorted(SCENARIOS) if args.all else [args.scenario]
        for scenario_id in selected:
            run_scenario(scenario_id)
        print(f"atdd: complete PASS ({len(selected)} scenarios)")
        return 0
    except (AcceptanceError, subprocess.CalledProcessError) as error:
        print(f"atdd: FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
