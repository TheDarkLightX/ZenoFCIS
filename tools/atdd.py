#!/usr/bin/env python3
"""Validate and execute the closed V1 acceptance-scenario registry."""

from __future__ import annotations

import argparse
import os
import re
import shlex
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path

from commons_hook import failure, report

ROOT = Path(__file__).resolve().parents[1]
FEATURE_ROOT = ROOT / "acceptance" / "features"
ATDD_TAG = re.compile(r"@atdd-([a-z0-9]+(?:-[a-z0-9]+)*)")
ATDD_TAG_LINE = re.compile(r"^@atdd-([a-z0-9]+(?:-[a-z0-9]+)*)$")
SCENARIO = re.compile(r"^\s*Scenario:\s*(\S.*)$")
PASSED_TESTS = re.compile(r"^test result: ok\. (\d+) passed;", re.MULTILINE)
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


def historical_oracle(test_filter: str, *, exact: bool = False) -> tuple[str, ...]:
    """Run retained private assertions; this does not restore a public native API."""

    return (
        "cargo", "+1.97.1", "test", "--manifest-path", "verification/Cargo.toml",
        "-p", "zeno-fcis-kernel-laws", "--all-features", "--locked", "--offline",
        "--lib", test_filter, "--", *(("--exact",) if exact else ()), "--test-threads=1",
    )


SCENARIOS: dict[str, AcceptanceScenario] = {
    "zal-observation-contracts": AcceptanceScenario(
        "Check observation-limited strategies and complete finite paired histories",
        (("python3", "-B", "-m", "unittest", "discover", "-s", "integrations/zal", "-p", "test_observation*.py"),
         ("python3", "-B", "-m", "unittest", "discover", "-s", "integrations/zal", "-p", "test_relational_observation.py"),
         ("python3", "-B", "integrations/zal/observation_diagrams.py",
          "integrations/zal/examples/observations/delivery.zal", "--profile",
          "integrations/zal/examples/observations/delivery.partial.json", "--out",
          "docs/zal/diagrams/observations", "--name", "delivery_partial", "--check"),
         ("python3", "-B", "integrations/zal/observation_diagrams.py",
          "integrations/zal/examples/observations/delivery_idempotent.zal", "--profile",
          "integrations/zal/examples/observations/delivery_idempotent.partial.json", "--out",
          "docs/zal/diagrams/observations", "--name", "delivery_idempotent", "--check"),
         ("python3", "-B", "integrations/zal/observation_diagrams.py",
          "integrations/zal/examples/observations/relational_delayed_leak.zal", "--profile",
          "integrations/zal/examples/observations/relational_delayed_leak.json", "--mode", "relational",
          "--out", "docs/zal/diagrams/observations", "--name", "relational_leak", "--check")),
    ),
    "app-factory-journey": AcceptanceScenario(
        "Generate and run the checked core, shell, ingress and UI together",
        (("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "template"),
         ("cargo", "+1.97.1", "build", "--locked", "--offline", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis"),
         ("env", "ZF_BIN=" + str(ROOT / Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "debug/zeno-fcis"), "python3", "tools/test_template_rules.py"),
         ("python3", "tools/test_zenoui_text.py"),
         ("python3", "tools/test_zenoui_refusal.py"),
         ("python3", "tools/test_app_factory.py"),
         ("python3", "tools/test_check_app_factory.py"),
         ("python3", "tools/check_app_factory.py")),
    ),
    "quantitative-risk": AcceptanceScenario(
        "Count declared inputs and bounded states and compute both exact reward extremes",
        (("nice", "-n", "19", "ionice", "-c3", "cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-contract", "--test", "risk"),
         ("nice", "-n", "19", "ionice", "-c3", "cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-cli", "--test", "risk"),
         ("nice", "-n", "19", "ionice", "-c3", "python3", "tools/risk_reference.py", "--cli",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug" / "zeno-fcis"),
          "--scratch", str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "risk-reference"))),
    ),
    "distribution-laws": AcceptanceScenario(
        "Check exact finite distributions given uniform independent random bits",
        (("nice", "-n", "19", "ionice", "-c3", "cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-spec", "--test", "distribution"),
         ("nice", "-n", "19", "ionice", "-c3", "cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-contract", "--test", "distribution_laws"),
         ("nice", "-n", "19", "ionice", "-c3", "cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-cli", "--test", "distribution_laws"),
         ("nice", "-n", "19", "ionice", "-c3", "python3", "tools/distribution_reference.py", "--cli",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug" / "zeno-fcis"))),
    ),
    "command-experience": AcceptanceScenario(
        "Keep public guide, diagnostics, link parsing and optional telemetry checks active",
        (("cargo", "+1.97.1", "build", "--locked", "--offline", "-p", "zeno-fcis-cli"),
         ("env", "ZENO_FCIS_CLI=" + str(ROOT / Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "debug/zeno-fcis"), "python3", "tools/test_guide_cli.py"),
         ("env", "ZENO_FCIS_CLI=" + str(ROOT / Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "debug/zeno-fcis"), "python3", "integrations/mcp/test_guide_tools.py"),
         ("env", "ZENO_FCIS_CLI=" + str(ROOT / Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "debug/zeno-fcis"), "python3", "integrations/commons/test_guide_mcp.py"),
         ("python3", "tools/test_agent_diagnostics.py"),
         ("python3", "tools/test_replay_run.py"),
         ("python3", "tools/test_library_doc_links.py"),
         ("env", "ZENO_FCIS_CLI=" + str(ROOT / Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "debug/zeno-fcis"), "python3", "tools/test_telemetry.py"),
         ("python3", "integrations/commons/test_telemetry.py")),
    ),
    "commons-rust": AcceptanceScenario(
        "Check the Rust coordination store against the Python reference",
        (("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-commons"),
         ("cargo", "+1.97.1", "build", "--locked", "--offline", "-p", "zeno-fcis-cli"),
         ("env", "ZENO_FCIS_COMMONS_CLI=" + shlex.quote(str(ROOT / Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "debug/zeno-fcis")), "python3", "integrations/commons/test_commons.py"),
         ("env", "ZENO_FCIS_CLI=" + str(ROOT / Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "debug/zeno-fcis"), "ZENO_FCIS_COMMONS_CLI=" + shlex.quote(str(ROOT / Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "debug/zeno-fcis")), "python3", "integrations/commons/test_cross.py")),
    ),
    "contract-game-input": AcceptanceScenario(
        "Exercise native Authority game inputs and independent certificate replay",
        (("cargo", "+1.97.1", "build", "--locked", "--offline", "-p", "zeno-fcis-contract", "--example", "trace_game_input"),
         ("env", "ZENO_TRACE_GAME_NATIVE=" + str(ROOT / Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "debug/examples/trace_game_input"),
          "python3", "-B", "-m", "unittest", "discover", "-s", "tools/trace_game")),
    ),
    "zeno-commons": AcceptanceScenario(
        "Share factory state, negative memory and coordination through Zeno Commons",
        (("python3", "integrations/commons/test_commons.py"),
         ("python3", "tools/test_commons_hooks.py"),
         ("python3", "-I", "tools/check_commons_controls.py")),
    ),
    "construction-command": AcceptanceScenario(
        "Refuse unsupported families and preserve construction authority boundaries",
        (("cargo", "+1.97.1", "test", "--release", "-p", "zeno-fcis-cli", "--test", "construct", "--locked", "--offline"),
         ("python3", "tools/test_priority_reference.py")),
    ),
    "keyed-collections": AcceptanceScenario(
        "Check maps of any size by data independence over their named entries",
        (("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-spec", "--test", "collections"),
         ("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-contract", "--test", "collections"),
         ("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-cli", "--test", "collections"),
         ("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-shell-sqlite", "--test", "collections_store"),
         ("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-cli", "--test", "collections_runtime"),
         ("python3", "tools/check_collections_controls.py", "--remove-targets")),
    ),
    "contract-tables": AcceptanceScenario(
        "Expand fixed-size tables exactly into ordinary fields and laws",
        (("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-spec", "--test", "tables"),
         ("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-contract", "--test", "contract_tables"),
         ("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-cli", "--test", "contract_tables"),
         ("python3", "tools/check_table_controls.py", "--remove-targets")),
    ),
    # BEGIN version-space acceptance registrations
    "loop-version-space": AcceptanceScenario(
        "Count finite meanings, ask the owner and check the unique program",
        (("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-loop-doors", "--test", "version_space"),
         ("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-cli", "--test", "loop_version_space"),
         ("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "final_checker_rejects_a_lying_singleton"),
         ("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "final_law_checker_rejects_a_lying_singleton"),
         ("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "final_transform_checker_rejects_a_lying_singleton"),
         ("python3", "-B", "tools/test_version_space_acceptance.py"),
         ("python3", "tools/test_version_space_reference.py"),
         ("python3", "tools/version_space_reference.py", "--self-test")),
    ),
    "loop-version-space-owner": AcceptanceScenario(
        "Only confirmed owner tuple answers constrain a requirements workspace",
        (("python3", "-B", "-m", "unittest", "discover", "-s", "integrations/zal", "-p", "test_loop_versions.py"),
         ("cargo", "+1.97.1", "build", "--locked", "--offline", "-p", "zeno-fcis-cli"),
         ("python3", "-B", "integrations/zal/check_loop_versions.py", "--cli",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug" / "zeno-fcis"))),
    ),
    "loop-version-space-semantics": AcceptanceScenario(
        "Recover every finite owner with independent counts and partitions",
        (("cargo", "+1.97.1", "build", "--locked", "--offline", "-p", "zeno-fcis-cli"),
         ("python3", "-B", "tools/check_version_space.py", "--cli",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug" / "zeno-fcis"))),
    ),
    "loop-sketch-frozen-order": AcceptanceScenario(
        "Preserve the joint C5 completion within twenty thousand assignments",
        (("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-loop-doors", "--test", "sketch_budget"),),
    ),
    # END version-space acceptance registrations
    # Learn door: one additive registry block; compilation/controls run at acceptance.
    "loop-learn-meanings": AcceptanceScenario(
        "Enumerate vocabulary meanings and confirm the lowered examples with the existing evaluator",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-loop-doors", "--locked", "--offline", "--lib", "learn::tests"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--test", "loop_learn"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--bin", "zeno-fcis", "doors::learn::tests"),
         ("python3", "tools/test_loop_learn_reference.py"),
         ("python3", "tools/loop_learn_controls.py", "--self-test"),
         ("python3", "tools/loop_learn_controls.py", "--cli",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug" / "zeno-fcis"),
          "--out", str(Path(os.environ.get("TMPDIR", "/tmp")) / "loop-learn-controls.json"))),
    ),
    "zal-loop-learn-owner": AcceptanceScenario(
        "Round-trip learning acts and confirm examples only on the owner channel",
        (("python3", "-m", "unittest", "discover", "-s", "integrations/zal", "-p", "test_loop_learn.py"),),
    ),
    "scalar-arithmetic": AcceptanceScenario(
        "Execute explicit scalar arithmetic through authored rules and the Authority",
        (("python3", "tools/test_scalar_arithmetic_reference.py"),
         ("python3", "tools/test_signed_arithmetic.py"),
         ("python3", "tools/test_scalar_arithmetic_controls.py"),
         ("python3", "tools/test_check_scalar_arithmetic.py"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-synthesis", "--locked", "--offline", "--test", "scalar_arithmetic"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-contract", "--locked", "--offline", "--test", "arithmetic_rules")),
    ),
    "wide-value-applications": AcceptanceScenario(
        "Execute wide values and larger scalar contracts through checked publication",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-synthesis", "--locked", "--offline",
          "--test", "v2_numeric_capacity", "--test", "v2_numeric_execution",
          "--test", "v2_numeric_wide_arithmetic", "--test", "v2_original_compound",
          "--test", "v2_evaluator_identity"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-shell-sqlite", "--locked", "--offline",
          "--test", "v2_original_compound", "--test", "v2_original_zusd", "--test", "v2_original_zusd_authority"),
         ("python3", "tools/check_g3_fixture.py"),
         ("python3", "tools/check_g3_app.py"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline",
          "--test", "contract_capacity")),
    ),
    "delivery-accounting-laws": AcceptanceScenario(
        "Enforce bounded delivery counts and amounts on actual candidates",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked",
          "contract::delivery_tests::"),
         ("python3", "tools/check_delivery_laws.py")),
    ),
    "bounded-collections-pipes": AcceptanceScenario(
        "Bound immutable canonical collections and profile-bound FIFO messages",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-collections",
          "--all-features", "--locked", "--test", "bounded_contract"),),
    ),
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
        "Compile the V1 consumer through its documented V2 migration",
        (
            ("python3", "tools/check_v1_compatibility.py"),
            (
                "cargo",
                "+1.97.1",
                "run",
                "--manifest-path",
                "test-projects/external-consumer/Cargo.toml",
                "--locked",
            ),
        ),
    ),
    "project-bootstrap": AcceptanceScenario(
        "Generate a reviewable project starter",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-bootstrap", "--locked"),),
    ),
    "finite-synthesis": AcceptanceScenario(
        "Synthesize and replay one contract across languages",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-synthesis", "-p", "zeno-fcis-cli", "--locked"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "synthesis_javascript", "--locked", "--", "--ignored"),
         ("python3", "tools/check_synthesis.py")),
    ),
    "zal-dialogue": AcceptanceScenario(
        "Review finite behavior through deterministic language help and exact revisions",
        (("python3", "-B", "integrations/zal/check_ladder_temporal.py", "native-build"),
         ("python3", "-B", "integrations/zal/check_withdrawal_pilot.py", "native-build"),
         ("python3", "-B", "integrations/zal/check_effects.py", "native-build"),
         ("python3", "-B", "-m", "unittest", "discover", "-s", "integrations/zal", "-p", "test_*.py", "-v"),
         ("python3", "-B", "integrations/zal/check_ladder_temporal.py", "planted"),
         ("python3", "-B", "integrations/zal/check_zal_tools.py"),
         ("python3", "-B", "integrations/zal/check_withdrawal_pilot.py", "run"),
         ("python3", "-B", "integrations/zal/check_withdrawal_pilot.py", "planted"),
         ("node", "integrations/zal/test_workspace.cjs"),
         ("node", "integrations/zal/test_mcp_sdk.mjs"),
         ("python3", "integrations/zal/oracles/independent_oracle.py"),
         ("python3", "integrations/zal/oracles/formula_oracle.py"),
         ("python3", "integrations/zal/oracles/explanation_oracle.py"),
         ("python3", "integrations/zal/oracles/temporal_oracle.py"),
         ("python3", "-B", "integrations/zal/check_behavior2.py", "planted"),
         ("python3", "-B", "integrations/zal/check_effects.py", "planted"),
         ("python3", "-B", "integrations/zal/check_lowering.py"),
         ("python3", "integrations/zal/run.py", "qualify", "integrations/zal/examples/order.zal")),
    ),
    "core-component-families": AcceptanceScenario(
        "Replay every supported finite component instance and refuse bad family evidence",
        (("cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline",
          "--target", "x86_64-unknown-linux-gnu", "--target-dir",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")))),
         ("python3", "tools/check_core_components.py", "--finite-proof", "--cli",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) /
              "x86_64-unknown-linux-gnu" / "debug" / "zeno-fcis"))),
    ),
    "program-search": AcceptanceScenario(
        "Propose smaller programs that agree on every tuple and stop with no claim over budget",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-program-search", "--locked", "--offline"),
         ("cargo", "+1.97.1", "clippy", "-p", "zeno-fcis-program-search", "--all-targets",
          "--all-features", "--locked", "--offline", "--", "-D", "warnings"),
         ("cargo", "+1.97.1", "fmt", "-p", "zeno-fcis-program-search", "--", "--check"),
         ("python3", "tools/test_measure_program_search.py"),
         # A checker-backed subset: every reported program through `transform
         # check` and `transform replay`, and every planted fault refused.
         ("cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"),
         ("cargo", "+1.97.1", "build", "-p", "zeno-fcis-program-search", "--example",
          "search_program", "--locked", "--offline"),
         ("python3", "tools/measure_program_search.py", "--scratch",
          "--family", "bounded-counter", "--family", "finite-phase-machine",
          "--cli", str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug" / "zeno-fcis"),
          "--search", str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug"
                          / "examples" / "search_program"))),
    ),
    # BEGIN composable program loops (2.4.0): additive acceptance block.
    "loop-pipeline": AcceptanceScenario(
        "Complete and repair proposals before optimizing qualified programs",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline",
          "--test", "loop_composable", "pipeline_"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline",
          "--test", "loop_composable", "composable_export_preserves_an_existing_directory"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline",
          "--bin", "zeno-fcis", "pipeline::tests"),
         ("python3", "-m", "unittest", "discover", "-s", "integrations/zal", "-p", "test_loop_composable.py", "-v"),
         ("python3", "tools/loop_completion_controls.py", "--self-test"),
         ("python3", "tools/test_loop_completion_controls.py"),
         ("python3", "tools/loop_composable_controls.py", "--self-test")),
    ),
    "loop-strategy": AcceptanceScenario(
        "Freeze the winning development strategy before evaluation",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline",
          "--test", "loop_composable", "strategy_"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline",
          "--bin", "zeno-fcis", "strategy::tests"),
         ("python3", "tools/test_loop_composable_reference.py")),
    ),
    "loop-rules": AcceptanceScenario(
        "Recheck concrete learned rules and every rewritten whole program",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline",
          "--test", "loop_composable", "rules_"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-program-search", "--locked", "--offline", "concrete_"),
         ("python3", "tools/test_loop_composable_reference.py"),
         ("python3", "tools/loop_composable_reference.py", "--self-test")),
    ),
    # END composable program loops acceptance block.
    # BEGIN typed strategy algebra and modes: same strategy command.
    "loop-strategy-algebra": AcceptanceScenario(
        "Type-check closed plans and require conforming execution prefixes",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-loop-doors", "--locked", "--offline",
          "--lib", "strategy_algebra_"),
         ("python3", "tools/test_loop_strategy_reference.py")),
    ),
    "loop-strategy-cases": AcceptanceScenario(
        "Assemble learned cases and check the full domain before export",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-loop-doors", "--locked", "--offline",
          "--test", "strategy_modes", "strategy_cases_"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline",
          "--test", "loop_composable", "strategy_profiles_")),
    ),
    "loop-strategy-diagnose": AcceptanceScenario(
        "Propose minimal weak-fault explanations and the most balanced internal probe",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-loop-doors", "--locked", "--offline",
          "--test", "strategy_modes", "strategy_"),
         ("python3", "tools/loop_strategy_reference.py", "--self-test")),
    ),
    "loop-strategy-readings": AcceptanceScenario(
        "Ask the owner only when finite readings change a conditional answer",
        (("python3", "-m", "unittest", "discover", "-s", "integrations/zal", "-p", "test_loop_strategy_modes.py", "-v"),
         ("python3", "tools/loop_strategy_controls.py", "--self-test"),
         ("cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"),
         ("python3", "tools/loop_strategy_controls.py", "--scratch", "--cli",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug" / "zeno-fcis"))),
    ),
    # END typed strategy algebra and modes.
    # BEGIN bounded public-schema knowledge acceptance.
    "loop-strategy-knowledge": AcceptanceScenario(
        "Query bounded relations and propose only code-consistent typed plans",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-loop-doors", "--locked", "--offline",
          "--lib", "kb_"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline",
          "--test", "loop_strategy_knowledge"),
         ("python3", "-B", "tools/test_strategy_knowledge_reference.py"),
         ("python3", "-B", "-m", "unittest", "discover", "-s", "integrations/zal",
          "-p", "test_loop_strategy_knowledge.py")),
    ),
    "loop-strategy-knowledge-privacy": AcceptanceScenario(
        "Keep external user knowledge out of source and release packages",
        (("python3", "-B", "tools/check_knowledge_files.py"),
         ("python3", "-B", "tools/test_knowledge_files.py"),
         ("python3", "-B", "tools/check_release_privacy.py", "check")),
    ),
    "usage-knowledge": AcceptanceScenario(
        "Apply scoped curated advice, retain private feedback, and replan from observations",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-loop-doors", "--locked", "--offline", "--lib", "kb_usage_"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--bin", "zeno-fcis", "kb_usage_"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--test", "kb_usage"),
         ("python3", "-B", "tools/test_usage_knowledge_reference.py"),
         ("python3", "-B", "integrations/knowledge/test_import_sources.py"),
         ("python3", "-B", "-m", "unittest", "discover", "-s", "integrations/mcp", "-p", "test_knowledge_tools.py")),
    ),
    # END bounded knowledge acceptance.
    # Loop doors and the ZAL loop profile (2.4.0, stage 1): the doors propose,
    # `transform check` and its receipt replay decide.
    # BEGIN black-box capture registry (additive; merge this block into the current base)
    "black-box-capture-exact": AcceptanceScenario(
        "Accept capture only after full-domain transform check and exact receipt replay",
        (("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-loop-doors", "capture"),
         ("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "capture"),
         ("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-cli", "--test", "loop_capture"),
         ("python3", "tools/test_capture_reference.py"),
         ("python3", "tools/capture_reference.py", "--self-test")),
    ),
    "black-box-capture-refusals": AcceptanceScenario(
        "Reference failures refuse and pre-decision budgets stop inconclusive with no candidate attempt",
        (("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis",
          "loop_command::doors::capture::tests"),
         # The complete integration target gates Linux denials/scratch/inheritance
         # and runs the typed refusal test on unsupported platforms (no zero-test filter).
         ("cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-cli", "--test", "loop_capture")),
    ),
    "black-box-capture-zal": AcceptanceScenario(
        "Round trip paired capture acts and refuse owner/judge claims on the model channel",
        (("python3", "-m", "unittest", "discover", "-s", "integrations/zal", "-p", "test_capture_language.py", "-v"),
         ("python3", "-m", "unittest", "discover", "-s", "integrations/zal", "-p", "test_loop_workspace.py", "-v")),
    ),
    # END black-box capture registry
    "loop-doors-repair": AcceptanceScenario(
        "Explain a wrong expression with its failing region and ranked sites and return a checked repair",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-loop-doors", "--locked", "--offline"),
         ("cargo", "+1.97.1", "clippy", "-p", "zeno-fcis-loop-doors", "--all-targets", "--all-features",
          "--locked", "--offline", "--", "-D", "warnings"),
         ("cargo", "+1.97.1", "fmt", "-p", "zeno-fcis-loop-doors", "--", "--check"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--test", "loop_doors",
          "repair"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--bin", "zeno-fcis",
          "doors::tests")),
    ),
    "loop-doors-sketch": AcceptanceScenario(
        "Complete a sketch's holes and check the completion through the session",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-loop-doors", "--locked", "--offline", "sketch"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-program-search", "--locked", "--offline", "target::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--test", "loop_doors",
          "a_sketch_completes"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--test", "loop_doors",
          "a_sketch_search_stopped_by_its_budget")),
    ),
    "loop-doors-unrealizable": AcceptanceScenario(
        "Report an unrealizable sketch only with a witness input or an exhausted complete space",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-loop-doors", "--locked", "--offline", "unrealizable"),
         ("python3", "tools/test_loop_doors_reference.py"),
         ("python3", "tools/loop_doors_reference.py", "tools/test_data/loop_doors/fixtures.json", "--self-test")),
    ),
    "loop-doors-family": AcceptanceScenario(
        "Judge a family program on every listed instance and check its fit on instances it was not fitted on",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-loop-doors", "--locked", "--offline", "param"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--test", "loop_doors",
          "family"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--test", "loop_doors",
          "transfer")),
    ),
    "loop-doors-meanings": AcceptanceScenario(
        "Search a sketch's meanings and key programs by what they mean",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-loop-doors", "--locked", "--offline"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--test", "loop_doors",
          "meaning"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--test", "loop_cli"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--bin", "zeno-fcis",
          "every_contained_command_declares_its_tools_and_runtime")),
    ),
    "zal-loop-memory-by-meaning": AcceptanceScenario(
        "Answer a refuted program written another way from memory",
        (("python3", "-m", "unittest", "discover", "-s", "integrations/zal", "-p", "test_loop_workspace.py", "-v"),
         ("cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"),
         ("python3", "integrations/zal/check_loop_workspace.py", "--cli",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug" / "zeno-fcis"))),
    ),
    "loop-teach-back": AcceptanceScenario(
        "Judge restated rules exactly and summarise a complete set",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--test", "loop_judges",
          "rules_are_judged"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--test", "loop_judges",
          "equal_rule_keys"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--test", "loop_judges",
          "a_rule_the_confirmed_answers_contradict"),
         ("python3", "tools/test_loop_doors_reference.py"),
         ("python3", "tools/loop_doors_reference.py", "tools/test_data/loop_doors/fixtures.json", "--self-test")),
    ),
    "loop-decompose": AcceptanceScenario(
        "Project a decomposition, explain a missing read and blame a wrong case",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--test", "loop_judges",
          "decomposition"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--test", "loop_judges",
          "reads_come_from_earlier"),
         ("cargo", "+1.97.1", "clippy", "-p", "zeno-fcis-cli", "--all-targets", "--locked", "--offline", "--", "-D",
          "warnings")),
    ),
    "loop-decompose-compose": AcceptanceScenario(
        "Compose a decomposition with the composition checker's joint step and compare it with the core",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--test", "loop_compose"),
         ("python3", "tools/loop_judges_controls.py", "--self-test"),
         ("python3", "tools/test_loop_judges_controls.py")),
    ),
    "zal-loop-questions": AcceptanceScenario(
        "Ask the owner about the largest open region and record the answer as laws over exactly it",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--test", "loop_judges",
          "the_largest_open_region"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--test", "loop_judges",
          "a_negated_number"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked", "--offline", "--test", "loop_judges",
          "an_answer_whose_successor"),
         ("cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"),
         ("python3", "integrations/zal/check_loop_workspace.py", "--cli",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug" / "zeno-fcis"))),
    ),
    "zal-loop-invariant-cycle": AcceptanceScenario(
        "Return a refuted invariant's counterexample to induction and accept the revised one",
        (("cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"),
         ("python3", "integrations/zal/check_loop_workspace.py", "--cli",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug" / "zeno-fcis"))),
    ),
    "zal-loop-round-trip": AcceptanceScenario(
        "Parse every loop act back from either surface to the same typed act",
        (("python3", "-m", "unittest", "discover", "-s", "integrations/zal", "-p", "test_loop_language.py", "-v"),),
    ),
    "zal-loop-model-cannot-answer": AcceptanceScenario(
        "Refuse an answer from the model and offer no answer or say tool",
        (("python3", "-m", "unittest", "discover", "-s", "integrations/zal", "-p", "test_loop_workspace.py", "-v"),),
    ),
    "zal-loop-owner-answers": AcceptanceScenario(
        "Answer the model's question and state a rule through the owner's terminal",
        (("cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"),
         ("python3", "integrations/zal/check_loop_workspace.py", "--cli",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug" / "zeno-fcis"))),
    ),
    "coupled-applications": AcceptanceScenario(
        "Check six coupled applications against independent references and refuse planted composition defects",
        # The independent references' own checks against their briefs come first:
        # unlike the checker's tests, they do not depend on current certificates.
        (("python3", "-m", "unittest", "discover", "-s", "tools/test_data/core_apps", "-p", "test_*_reference.py"),
         ("python3", "tools/test_check_core_app.py"),
         *(("python3", "tools/check_core_app.py", "--app", app, "--static")
           for app in ("warehouse-reservation", "expense-approval", "reliable-jobs",
                       "document-release", "api-quota", "equipment-booking"))),
    ),
    "ocaml-scalar-consumer": AcceptanceScenario(
        "Consume actual generated Rust applications from the pinned OCaml client",
        (("python3", "tools/test_qualify_ocaml_consumer.py"),
         ("cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"),
         ("python3", "tools/qualify_ocaml_consumer.py", "--cli",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug" / "zeno-fcis"))),
    ),
    "bounded-completion": AcceptanceScenario(
        "Verify finite exits and prepare bounded chunks without publication authority",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-synthesis", "--test", "completion", "--locked"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-synthesis", "--lib", "finite::execution_v2::continuation::tests::", "--locked"),
         historical_oracle("oracle::tests::preparation::"),
         historical_oracle("oracle::tests::checked_continuation::"),
         historical_oracle("oracle::tests::bounded_completion::original_bounded_completion_example_runs", exact=True),
         ("cargo", "+1.97.1", "check", "-p", "zeno-fcis", "--no-default-features", "--features", "synthesis", "--locked")),
    ),
    "claim-substance": AcceptanceScenario(
        "Report laws and claims that cannot constrain any transition",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-spec", "--lib", "--locked", "substance"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "cli_adopter_flow", "--locked", "substan"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "cli_adopter_flow", "--locked", "rc3_cli_formal_outcomes_and_retention_are_process_level")),
    ),
    "system-properties": AcceptanceScenario(
        "Check properties against the exact finite transition program",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-synthesis", "--lib", "--locked", "system::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-formal-tools", "--lib", "--locked", "system::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-formal-tools", "--test", "system_contract", "--locked"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked", "counter_system_properties")),
    ),
    "inductive-claims": AcceptanceScenario(
        "Prove invariants by induction over the laws the authority enforces",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-spec", "--lib", "--locked", "induction::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-formal-tools", "--lib", "--locked", "inductive"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-formal-tools", "--lib", "--locked", "a_split_step_uses_each_decision_kinds_own_laws"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-formal-tools", "--lib", "--locked", "replay_confirms_models_where_strict_evaluation_has_no_value"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-formal-tools", "--lib", "--locked", "undefined_results_have_their_own_record_and_name"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-formal-tools", "--lib", "--locked", "declared_"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-spec", "--lib", "--locked", "declared_domains_come_from_variants_bools_and_declared_ranges"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-spec", "--lib", "--locked", "type_ranges_are_inclusive_and_declared_only_on_ints"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-spec", "--lib", "--locked", "a_declared_range_is_part_of_the_canonical_project"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-bootstrap", "--test", "schema_lowering", "--locked", "a_declared_range_is_the_binding_of_its_int"),
         historical_oracle("oracle::authority::tests::admission_checks_the_command_and_context_against_its_own_schema", exact=True),
         historical_oracle("oracle::authority::tests::genesis_is_checked_against_its_own_schema", exact=True),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis", "--test", "program_api", "--locked"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-synthesis", "--test", "v2_authority", "--test", "v2_catalog", "--locked"),
         historical_oracle("oracle::laws::tests::step_assumptions_must_be_enforced_on_the_decisions_they_are_assumed_on", exact=True),
         historical_oracle("oracle::laws::tests::a_manifest_must_enforce_the_scopes_its_project_declares", exact=True),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked", "exit_classes_are_stable")),
    ),
    "reserved-domains": AcceptanceScenario(
        "Keep project commitment domains out of the library namespace",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-project", "--lib", "--locked", "reserved_domain_tests"),
         historical_oracle("oracle::authority::tests::project_state_domains_cannot_enter_the_reserved_namespace", exact=True),
         historical_oracle("oracle::adapter_zenodex::zusd::tests::precondition_hash_matches_the_explicit_value_domain", exact=True)),
    ),
    "effect-spellings": AcceptanceScenario(
        "Reject effect spellings that bypass qualified-path rules",
        (("python3", "tools/check_assurance.py", "--self-test"),),
    ),
    "law-paths": AcceptanceScenario(
        "Report law and claim paths that name no declared type or field",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-spec", "--lib", "--locked", "paths::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "cli_adopter_flow", "--locked", "law_and_claim_paths")),
    ),
    "kernel-laws": AcceptanceScenario(
        "Check kernel laws exhaustively over small stated domains",
        (("cargo", "+1.97.1", "test", "--manifest-path", "verification/Cargo.toml", "--locked", "holds_on_every_small_input"),),
    ),
    "determinism": AcceptanceScenario(
        "Detect nondeterminism in decision code by static rules and repeated execution",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked", "purity"),
         historical_oracle("oracle::authority::tests::probe_run_counts_must_compare_at_least_two_executions", exact=True),
         historical_oracle("oracle::authority::tests::probed_execution_returns_the_decision_that_every_run_agreed_on", exact=True),
         historical_oracle("oracle::authority::tests::probed_rejections_are_compared_too", exact=True),
         historical_oracle("oracle::tests::probe_divergence::"),
         ("cargo", "+1.97.1", "run", "-q", "-p", "zeno-fcis-cli", "--locked", "--", "purity",
          "crates/zeno-fcis-cli/templates/durable-counter/src/program.rs",
          "crates/zeno-fcis-cli/templates/durable-counter/src/laws.rs",
          "crates/zeno-fcis-cli/templates/durable-counter/synthesized/transition.rs",
          "crates/zeno-fcis-cli/templates/account-lockout/src/program.rs",
          "crates/zeno-fcis-cli/templates/account-lockout/src/laws.rs",
          "crates/zeno-fcis-cli/templates/order-fulfillment/src/program.rs",
          "crates/zeno-fcis-cli/templates/order-fulfillment/src/laws.rs",
          "crates/zeno-fcis-cli/templates/inventory-reservation/src/program.rs",
          "crates/zeno-fcis-cli/templates/inventory-reservation/src/laws.rs",
          "crates/zeno-fcis-cli/templates/inventory-reservation/synthesized/transition.rs",
          "crates/zeno-fcis-cli/templates/compliance-gateway/src/program.rs",
          "crates/zeno-fcis-cli/templates/compliance-gateway/src/laws.rs",
          "crates/zeno-fcis-cli/templates/compliance-gateway/src/rules.rs",
          "crates/zeno-fcis-cli/templates/compliance-gateway/synthesized/transition.rs",
          "crates/zeno-fcis-cli/templates/withdrawal-queue/src/program.rs",
          "crates/zeno-fcis-cli/templates/withdrawal-queue/src/laws.rs",
          "crates/zeno-fcis-cli/templates/withdrawal-queue/src/controller.rs",
          "crates/zeno-fcis-cli/templates/withdrawal-queue/synthesized/transition.rs",
          "crates/zeno-fcis-cli/templates/agent-treasury-guard/src/program.rs",
          "crates/zeno-fcis-cli/templates/agent-treasury-guard/src/laws.rs",
          "crates/zeno-fcis-cli/templates/agent-treasury-guard/synthesized/transition.rs")),
    ),
    "example-templates": AcceptanceScenario(
        "Emit example applications whose laws all constrain their transitions",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "cli_adopter_flow", "--locked",
          "cli_example_templates"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked",
          "every_application_template")),
    ),
    "generated-contracts": AcceptanceScenario(
        "Regenerate each template's V2 contract from its declarations and rules",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked", "contract::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked", "binding::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "contract_generation", "--locked"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "contract_export", "--locked")),
    ),
    "checked-transform": AcceptanceScenario(
        "Accept a candidate program only with an exhaustive equivalence receipt",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked",
          "transform::tests::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "transform_cli", "--locked")),
    ),
    "parallel-transform-parity": AcceptanceScenario(
        "Check large domains in parallel with the serial checker's exact verdicts",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked",
          "transform_parallel::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-shell-sqlite", "--lib", "--locked",
          "equivalence::checker::ranges::"),
         ("python3", "tools/test_containment_selection.py"),
         ("python3", "tools/check_transform_parallel.py")),
    ),
    "checked-optimizer": AcceptanceScenario(
        "Propose smaller programs and accept only checked ones",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked",
          "optimize::tests::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "optimize_cli", "--locked")),
    ),
    "neural-loop": AcceptanceScenario(
        "Improve a program only through checked proposals in the bounded loop",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked",
          "neural_loop::tests::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked",
          "loop_command::tests::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "loop_cli", "--locked"),
         ("python3", "integrations/mcp/test_transform_tools.py")),
    ),
    "contract-draft": AcceptanceScenario(
        "Draft three contracts through explicitly supplied decision labels",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked",
          "draft_", "--", "--test-threads=1"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "contract_draft", "--locked",
          "--", "--include-ignored", "--test-threads=1"),
         ("python3", "integrations/mcp/test_draft_tools.py")),
    ),
    "contract-review": AcceptanceScenario(
        "Review a contract with distinguishing examples before it is trusted",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked",
          "contract::review::tests::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "contract_review", "--locked"),
         ("python3", "tools/numeric_symbolic_solver_tests.py")),
    ),
    "contract-refinement": AcceptanceScenario(
        "Check all raw reservation pairs and detect refinement faults",
        (("cargo", "+1.97.1", "test", "--release", "-p", "zeno-fcis-contract", "--lib", "--locked", "--offline",
          "refinement::"),
         ("cargo", "+1.97.1", "test", "--release", "-p", "zeno-fcis-contract", "--test", "contract_refinement",
          "--locked", "--offline"),
         ("cargo", "+1.97.1", "test", "--release", "-p", "zeno-fcis-cli", "--test", "contract_refine_check",
          "--locked", "--offline"),
         ("python3", "tools/test_refinement_reference.py"),
         ("python3", "tools/make_refinement_example.py", "--check"),
         ("python3", "tools/check_contract_refinement.py", "--self-test"),
         ("cargo", "+1.97.1", "test", "--release", "-p", "zeno-fcis-contract", "--test", "compiler_roots",
          "--locked", "--offline")),
    ),
    "specification-gaps": AcceptanceScenario(
        "Find the decisions and decision tables the declared laws admit",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked",
          "contract::review::judge_tests::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked",
          "contract::review::run_properties::tests::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked",
          "contract::review::decision_space::tests::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked",
          "loop_command::doors::judges::"),
         ("python3", "tools/test_contract_judges_reference.py",
          "Judges.test_determined_requires_the_unique_outcome_to_match_native_execution"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "contract_judges", "--locked"),
         ("env", "ZENO_FCIS_CLI=" + str(ROOT / Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "debug/zeno-fcis"),
          "python3", "tools/test_judge_doc_commands.py")),
    ),
    "core-profile": AcceptanceScenario(
        "Agree with an independent reference on all 77 core-component instances",
        (("python3", "tools/test_contract_judges_reference.py"),
         ("cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline",
          "--target", "x86_64-unknown-linux-gnu", "--target-dir",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")))),
         ("python3", "tools/check_contract_judges.py", "--controls", "--cli",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) /
              "x86_64-unknown-linux-gnu" / "debug" / "zeno-fcis")),
         ("python3", "tools/check_contract_judges.py", "--self-test", "--cli",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) /
              "x86_64-unknown-linux-gnu" / "debug" / "zeno-fcis"))),
    ),
    "composed-domain-ladder": AcceptanceScenario(
        "Prove the ten-core ladder without enumerating its product",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-contract", "--lib", "--locked", "--offline",
          "checked_composition::obligations_tests::the_ten_core_ladder_is_proved_without_its_product"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "contract_check_composition_domain",
          "--locked", "--offline", "the_ten_core_ladder_is_proved_without_enumerating_its_product")),
    ),
    "composed-domain-refusal": AcceptanceScenario(
        "Refuse the wide coupling before evaluating anything",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-contract", "--lib", "--locked", "--offline", "--",
          "checked_composition::obligations_tests::the_wide_coupling_is_refused_before_evaluation",
          "checked_composition::obligations_tests::a_lowered_total_is_refused_before_evaluation"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "contract_check_composition_domain",
          "--locked", "--offline", "the_wide_coupling_is_refused_before_evaluation")),
    ),
    "composed-domain-faults": AcceptanceScenario(
        "Refuse or violate every planted composition fault",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-contract", "--lib", "--locked", "--offline", "--",
          "checked_composition::domain_tests::", "checked_composition::joint_tests::",
          "checked_composition::obligations_tests::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-contract", "--doc", "--locked", "--offline",
          "JointComposition"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "contract_check_composition_domain",
          "--locked", "--offline"),
         ("python3", "tools/test_composition_domain_reference.py")),
    ),
    "composed-domain-stateless": AcceptanceScenario(
        "Accept a stateless guard under the composed-domain profile and refuse it under version one",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-contract", "--lib", "--locked", "--offline", "--",
          "checked_composition::domain_tests::version_", "checked_composition::domain_tests::a_contract_with_zero_state",
          "checked_composition::joint_tests::a_read_only", "checked_composition::obligations_tests::the_guarded"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "contract_check_composition_domain",
          "--locked", "--offline", "a_read_only_guard_is_accepted_under_three_and_refused_under_one")),
    ),
    "composed-domain-reference": AcceptanceScenario(
        "Agree with an independent reference on fresh-seed random compositions",
        (("python3", "tools/test_check_composed_domain.py"),
         ("python3", "tools/test_composed_domain_runtime_controls.py", "--out",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "composed-domain-runtime-atdd.json")),
         ("python3", "tools/test_composition_domain_reference.py"),
         ("cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"),
         ("python3", "tools/composition_domain_soundness.py", "fixtures", "--check", "--cli",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug" / "zeno-fcis")),
         ("python3", "tools/composition_domain_soundness.py", "faults", "--cli",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug" / "zeno-fcis")),
         ("python3", "tools/composition_domain_soundness.py", "run", "--plain", "40", "--wired", "80",
          "--out", str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "composition-domain-atdd"),
          "--cli", str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug" / "zeno-fcis")),
         ("python3", "tools/composition_domain_soundness.py", "summary", "--min-complete", "120",
          "--out", str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "composition-domain-atdd")),
         ("python3", "tools/composition_domain_soundness.py", "self-test", "--plain", "20", "--wired", "20",
          "--count", "12", "--out",
          str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "composition-domain-atdd-self-test"),
          "--cli", str(Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug" / "zeno-fcis"))),
    ),
    "symbolic-per-case-checks": AcceptanceScenario(
        "Check rule cases with pinned solvers beyond the enumeration cap",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked",
          "symbolic::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "symbolic_cli", "--locked"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-formal-tools", "--locked",
          "smt_sessions_"),
         ("python3", "tools/test_symbolic_solver_tests.py"),
         ("python3", "tools/symbolic_solver_tests.py"),
         ("python3", "tools/check_symbolic_certificates.py")),
    ),
    "tau-finite-kit": AcceptanceScenario(
        "Translate finite ladders and retain exact independent check inputs",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked", "--offline",
          "contract::tau::tests::", "--", "--test-threads=1"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked", "--offline",
          "tau_command::tests::", "--", "--test-threads=1"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "tau_kit", "--locked", "--offline",
          "--", "--test-threads=1"),
         ("python3", "-B", "integrations/tau/test_kit.py")),
    ),
    "checked-upgrade": AcceptanceScenario(
        "Adopt a checked candidate and upgrade a live store under its lineage",
        (("python3", "tools/check_historical_24_copy.py"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-synthesis", "--locked", "--offline",
          "--test", "historical_i64", "--test", "historical_copy", "--test", "historical_context"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-contract", "--locked", "--offline",
          "--test", "historical_24"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked",
          "contract::tests::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "contract_adopt", "--locked"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-shell-sqlite", "--lib", "--locked",
          "v2::upgrade::tests::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-shell-sqlite", "--lib", "--locked",
          "v2::equivalence::tests::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-shell-sqlite", "--test", "historical_24", "--locked"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked",
          "transform::tests::every_receipt_field_is_bound", "--", "--exact"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked",
          "transform::tests::the_shells_comparison_agrees"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-shell-sqlite", "--test", "upgrade", "--locked",
          "--features", "crash-injection"),
         ("python3", "tools/check_contract_upgrade.py")),
    ),
    "rule-change-upgrade": AcceptanceScenario(
        "Change the rules of a live store after the owner reviews the change",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "contract_evolve", "--locked"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-shell-sqlite", "--test", "behaviour_change", "--locked"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-shell-sqlite", "--lib", "--locked",
          "v2::upgrade::tests::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-shell-sqlite", "--lib", "--locked",
          "v2::message_tests::"),
         ("python3", "tools/check_contract_evolve.py")),
    ),
    "data-migration-upgrade": AcceptanceScenario(
        "Migrate the data of a live store across a layout change",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "contract_migrate", "--locked"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-shell-sqlite", "--test", "data_migration", "--locked"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-shell-sqlite", "--lib", "--locked",
          "v2::migration::tests::"),
         ("python3", "tools/check_contract_migrate.py"),
         ("python3", "tools/check_migration_claims.py")),
    ),
    "contract-diff": AcceptanceScenario(
        "Classify a contract change and name every changed item",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--locked",
          "contract::diff::"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--test", "contract_diff", "--locked")),
    ),
    "app-journey": AcceptanceScenario(
        "Build, optimize, adopt and upgrade applications through their command lines alone",
        (("python3", "tools/test_check_app_journey.py"),
         ("python3", "tools/test_relay.py"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-shell-sqlite", "--test", "relay", "--locked"),
         ("python3", "tools/check_app_journey.py")),
    ),
    "operational-journey": AcceptanceScenario(
        "Operate, deliver, evolve, migrate and replay one live application",
        (("python3", "tools/test_relay_destination.py"),
         ("python3", "tools/test_check_operational_journey.py"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-shell-sqlite", "--lib", "--locked",
          "a_shared_simulation_lookup_does_not_wait_for_a_busy_memo"),
         ("python3", "tools/check_operational_journey.py")),
    ),
    "gate-evidence": AcceptanceScenario(
        "Publish gate evidence only for the unchanged committed revision",
        (("python3", "tools/test_record_gate_evidence.py"),),
    ),
    "zusd-lane-gaps": AcceptanceScenario(
        "Pin what the zeno language cannot state about the zUSD lane",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-spec", "--test", "zusd_lane_gaps", "--locked"),),
    ),
    "generated-application": AcceptanceScenario(
        "Run an authored application through durable authorization",
        (("python3", "tools/test_generated_application.py"),
         ("python3", "tools/check_generated_application.py")),
    ),
    "composed-program": AcceptanceScenario(
        "Retain historical composed-program reference checks",
        (historical_oracle("oracle::composed_program::tests::"),
         historical_oracle("oracle::tests::composed_construction::")),
    ),
    "production-authority": AcceptanceScenario(
        "Admit only catalog and invocation bound transitions",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-authority", "--test", "v2_schema_binding", "--locked"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-synthesis", "--test", "v2_authority", "--test", "v2_catalog", "--locked"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-synthesis", "--lib", "finite::execution_v2::catalog::tests::", "--locked"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis", "--test", "program_api", "--locked"),
         ("python3", "tools/test_check_api_refusals.py"),
         ("python3", "tools/check_api_refusals.py")),
    ),
    "authenticated-ingress": AcceptanceScenario(
        "Authenticate HTTP commands and recover one durable SQLite connection",
        (("python3", "tools/check_ingress.py", "--require-loopback"),
         ("cargo", "+1.97.1", "test", "-p", "zeno-fcis-ingress", "--locked", "--offline")),
    ),
    "sqlite-authority": AcceptanceScenario(
        "Persist an authorized transition and its exact outbox obligations",
        (("cargo", "+1.97.1", "test", "-p", "zeno-fcis-shell-sqlite", "--locked",
          "--features", "crash-injection"),
         ("python3", "tools/test_check_compile_fail.py"),
         ("python3", "tools/check_compile_fail.py")),
    ),
    "release-contract": AcceptanceScenario(
        "Run the local V1 release gate",
        (
            ("python3", "-B", "site/tests/test_socket_free_launcher.py"),
            ("python3", "tools/test_bls_boundary.py"),
            ("python3", "tools/test_cargo_source_scope.py"),
            ("python3", "-B", "-m", "unittest", "discover", "-s", "tools/trace_game", "-p", "test_*.py"),
            ("python3", "tools/check_assurance.py", "--self-test"),
            ("python3", "tools/test_resolved_assurance.py"),
            ("python3", "tools/check_assurance.py"),
            ("python3", "tools/check_library_docs.py"),
            # Negative controls for the publication gate `rc_package.py check` runs.
            ("python3", "tools/test_public_docs.py"),
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
            (
                "cargo",
                "+1.97.1",
                "bench",
                "--profile",
                "dev",
                "-p",
                "zeno-fcis-crypto",
                "-p",
                "zeno-fcis-collections",
                "--all-features",
                "--locked",
                "--",
                "--test",
            ),
        ),
    ),
    "lean-proof-tools": AcceptanceScenario(
        "Check Lean source closure, axiom auditing and proof-gate controls",
        (("python3", "tools/test_check_lean_proofs.py"),),
    ),
    "effect-policy-tools": AcceptanceScenario(
        "Check complete effect control spaces and payment policy products",
        (("python3", "tools/test_effect_control_space.py"),
         ("python3", "tools/test_payment_policy_product.py")),
    ),
    "meaning-client-exits": AcceptanceScenario(
        "Preserve verdict exit classes in meaning clients",
        (("python3", "tools/test_meaning_exit_clients.py"),),
    ),
    "service-runtime-tools": AcceptanceScenario(
        "Check generated services and their independent controls",
        (("python3", "tools/check_service_app.py"),
         ("python3", "tools/test_service_controls.py")),
    ),
    "two-run-reference": AcceptanceScenario(
        "Compare two-run properties with the independent finite reference",
        (("python3", "tools/test_two_run.py", "--binary", str(ROOT / Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "debug/zeno-fcis")),),
    ),
    "probity-guardrails": AcceptanceScenario(
        "Reject unsafe agent workflow actions deterministically",
        (("python3", "tools/check_probity.py"),),
    ),
    "verus-integration": AcceptanceScenario(
        "Check Verus evidence admission and shared runtime arithmetic boundaries",
        (("python3", "tools/test_check_verus.py"),
         ("python3", "tools/test_verus_coverage.py"),
         ("python3", "tools/test_verus_ids.py"),
         ("python3", "tools/test_qualification_controls.py"),
         ("python3", "tools/test_qualification_sources.py"),
         ("python3", "tools/test_v2_native_dependencies.py"),
         ("python3", "tools/test_check_arithmetic_proof.py"),
         ("python3", "tools/test_check_arithmetic_typed_proof.py"),
         ("python3", "tools/test_arithmetic_emitted_ast.py"),
         ("python3", "tools/test_check_collections_runtime.py"),
         ("python3", "tools/test_check_effect_reservation.py")),
    ),
    "finite-execution-proof": AcceptanceScenario(
        "Preserve complete finite execution and reject missing proof contracts",
        (("python3", "tools/test_check_finite_execution.py"),),
    ),
    "v2-metered-execution": AcceptanceScenario(
        "Keep V2 instruction charges private and retain exact refusal usage",
        (("python3", "tools/test_check_metered_execution.py"),),
    ),
    "v2-shared-finite-checker": AcceptanceScenario(
        "Bind both finite comparison callers to one checker and reject incomplete proof evidence",
        (("python3", "tools/test_check_checker.py"), ("python3", "tools/test_checker_native_controls.py")),
    ),
    "v2-canonical-byte-readers": AcceptanceScenario(
        "Keep exact V2 integer byte reads over the full offset and width domain",
        (("python3", "tools/test_check_canonical_bytes.py"),),
    ),
    "v2-protected-records": AcceptanceScenario(
        "Preserve exact protected record projection and retained refusal reports",
        (
            ("python3", "tools/test_check_protected_input.py"),
            ("cargo", "+1.97.1", "test", "--locked", "-p", "zeno-fcis-synthesis", "--test", "v2_protected_input"),
        ),
    ),
    "v2-record-execution": AcceptanceScenario(
        "Preserve original source/field ABI binding and shared Read/Byte/Step accounting",
        (
            ("python3", "tools/test_check_composition_v2.py", "RecordStageMigration"),
            ("cargo", "+1.97.1", "test", "--locked", "-p", "zeno-fcis-synthesis", "--lib", "finite::execution_v2::composition::record_tests"),
            ("cargo", "+1.97.1", "test", "--locked", "-p", "zeno-fcis-synthesis", "--test", "v2_record_execution"),
        ),
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
        (historical_oracle("oracle::spec::rc3_acceptance::rc3_temporal_modes", exact=True),),
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
            (
                "cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked",
                "cli_tool_timeout_is_execution_failure",
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
            (
                "cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked",
                "cli_project_",
            ),
            (
                "cargo", "+1.97.1", "test", "-p", "zeno-fcis-cli", "--locked",
                "--test", "cli_adopter_flow", "agent_",
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

    vacuous = "test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 9 filtered out\n"
    two_binaries = (
        "test result: ok. 2 passed; 0 failed; 0 ignored\n"
        "test result: ok. 0 passed; 0 failed; 0 ignored\n"
        "test result: ok. 3 passed; 0 failed; 0 ignored\n"
    )
    if passed_tests(vacuous) != 0 or passed_tests(two_binaries) != 5:
        raise AcceptanceError("self-test miscounted the tests a command passed")

    subprocess.run((sys.executable, "tools/test_atdd_registry.py"), cwd=ROOT, check=True)


def passed_tests(output: str) -> int:
    """Count the tests cargo reports as passed, over every test binary."""

    return sum(int(count) for count in PASSED_TESTS.findall(output))


def run_command(command: tuple[str, ...]) -> None:
    """Run one fixed command. A test command must pass at least one test,
    because a filter that matches nothing passes without checking anything."""

    environment = None
    resource_prefix = ("nice", "-n", "19", "ionice", "-c3")
    executable = command[len(resource_prefix):] if command[:len(resource_prefix)] == resource_prefix else command
    if executable[0:3] == ("cargo", "+1.97.1", "doc"):
        environment = {**os.environ, "RUSTDOCFLAGS": "-D warnings"}
    if executable[0:3] != ("cargo", "+1.97.1", "test"):
        subprocess.run(command, cwd=ROOT, check=True, env=environment)
        return
    completed = subprocess.run(
        command, cwd=ROOT, env=environment, stdout=subprocess.PIPE, text=True
    )
    sys.stdout.write(completed.stdout)
    sys.stdout.flush()
    if completed.returncode != 0:
        raise subprocess.CalledProcessError(completed.returncode, command)
    if passed_tests(completed.stdout) == 0:
        raise AcceptanceError(f"{shlex.join(command)} ran no tests")


def run_scenario(scenario_id: str) -> None:
    """Run one scenario's fixed commands without interpreting feature prose."""

    scenario = SCENARIOS[scenario_id]
    print(f"atdd: RUN {scenario_id}: {scenario.title}", flush=True)
    started = time.monotonic()
    command: tuple[str, ...] = ()
    try:
        for command in scenario.commands:
            print(f"atdd: EXEC {shlex.join(command)}", flush=True)
            run_command(command)
    except BaseException as error:
        report("atdd", "scenario-failed", scenario_id, {**failure(error), "command": shlex.join(command),
                                                        "seconds": round(time.monotonic() - started, 1)})
        raise
    report("atdd", "scenario-passed", scenario_id, {"commands": len(scenario.commands),
                                                   "seconds": round(time.monotonic() - started, 1)})
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
            print("atdd: self-test PASS (8 hostile or inert-prose mutations and the test counter checked)")
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
        run_command(("python3", "tools/test_atdd_registry.py"))
        run_command(("python3", "tools/test_scaffold_versions.py"))
        run_command(("python3", "tools/check_authority_v2.py", "--check-sources"))
        run_command(("cargo", "+1.97.1", "test", "--locked", "--offline", "-p",
                     "zeno-fcis-synthesis", "--test", "v2_evaluator_identity"))
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
