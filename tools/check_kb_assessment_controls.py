#!/usr/bin/env python3
"""Build planted knowledge-engine violations in an isolated committed-source copy."""
from __future__ import annotations
import argparse
import difflib
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile

PREFIX = ["nice", "-n", "19", "ionice", "-c3"]
CLI = "crates/zeno-fcis-cli/src/"


def control(name, file, pattern, replacement, tests):
    return {"name": name, "file": CLI + file, "pattern": pattern, "replacement": replacement,
            "tests": tests if type(tests) is list else [tests]}


CONTROLS = [
    control("unpublished-work-discarded", "knowledge_store.rs", r"pub\(crate\) fn retain_unpublished\((.*?)\) -> Result<FileRef, &'static str> \{", "\\g<0> return Ok(FileRef {path: \"missing\".into(), sha256: \"a\".repeat(64)});", "knowledge_growth_tests::unpublished_work_survives_a_revision_race_without_layer_authority"),
    control("partial-refusal-work-dropped", "knowledge_assessment.rs", r"\.chain\(partial.iter\(\)\)", ".chain(partial.iter().take(0))", "knowledge_assessment::tests::partial_replay_work_survives_a_later_stop"),
    control("pareto-violation", "knowledge_assessment.rs", r"if regression \{", "if regression && false {", ["knowledge_assessment::tests::one_worse_task_cannot_be_hidden", "knowledge_assessment::tests::pareto_rejects_one_worse_measure"]),
    control("empty-evidence-promotion", "knowledge_assessment.rs", r'\(Decision::Inconclusive, "no-applicable-task"\)', '(Decision::Promote, "no-applicable-task")', "knowledge_assessment::tests::no_applicable_task_never_promotes"),
    control("wall-time-assessment", "knowledge_assessment.rs", r"if regression \{", "let wall_times=(2u64,1u64); regression=wall_times.1>wall_times.0; improvement=wall_times.1<wall_times.0; if regression {", "knowledge_assessment::tests::work_counters_determine_usefulness"),
    control("proposing-task-assessed", "knowledge_assessment.rs", r"\|\| proposing.contains\(&c.task\)", "", "knowledge_assessment::tests::proposing_task_is_never_held_out"),
    control("telemetry-promotion", "knowledge_store.rs", r"let telemetry = crate::knowledge_growth::telemetry\(&next\);", "for c in &mut next.candidates {if c.observed_work>0{c.state=State::Promoted;}} let telemetry=crate::knowledge_growth::telemetry(&next);", "knowledge_store::tests::corroboration_and_telemetry_cannot_promote"),
    control("changed-dependency-kept", "knowledge_store.rs", r"c.state != State::Refuted\s*&& c.assessment.is_some\(\)", "false && c.state != State::Refuted && c.assessment.is_some()", "knowledge_store::tests::dependency_change_retracts_only_justified_items"),
    control("partial-layer-publication", "knowledge_store.rs", r'if fault == Fault::Descriptor \{\s*return Err\("kb-simulated-failure"\);', 'if fault == Fault::Descriptor { std::fs::write(path,b"partial").map_err(|_|"kb-write")?; return Err("kb-simulated-failure");', "knowledge_store::tests::atomic_failure_preserves_complete_layer"),
    control("generalized-without-assessment", "knowledge_store.rs", r"pub\(crate\) fn check\(layer: &Layer\) -> Result<\(\), &'static str> \{", "pub(crate) fn check(layer: &Layer) -> Result<(), &'static str> { if layer.candidates.iter().any(|c|c.origin==\"isa-generalization\"){return Ok(());}", "knowledge_store::tests::generalized_and_mined_records_cannot_self_promote"),
    control("dependent-left-promoted", "knowledge_assessment.rs", r"if !c.parents.is_disjoint\(&pending\)", "if false && !c.parents.is_disjoint(&pending)", "knowledge_assessment::tests::dependent_retraction_is_minimal"),
    control("corroboration-promotion", "knowledge_store.rs", r"let telemetry = crate::knowledge_growth::telemetry\(&next\);", "for c in &mut next.candidates {if c.families.len()>1{c.state=State::Promoted;}} let telemetry=crate::knowledge_growth::telemetry(&next);", "knowledge_store::tests::corroboration_and_telemetry_cannot_promote"),
    control("duplicate-family-vote", "knowledge_workflow.rs", r"c.families.len\(\)", "c.families.len()+1", "knowledge_store::tests::duplicate_corroboration_family_has_one_queue_vote"),
    control("mined-direct-advice", "knowledge_workflow.rs", r"c.state == State::Promoted && applies", '(c.state == State::Promoted || c.origin=="failure-mining") && applies', "knowledge_store::tests::unassessed_sources_never_become_advice"),
    control("review-approval-promotes", "knowledge_assessment.rs", r"if annotations\s*\.iter\(\)\s*\.any\(\|a\| matches!", 'if annotations.iter().any(|a|a.text=="approved"){return Ok((Decision::Promote,"approval"));} if annotations.iter().any(|a| matches!', "knowledge_assessment::tests::reviewer_annotation_cannot_promote"),
    control("episode-no-verdict", "knowledge_growth.rs", r"\|\| self.verdict.is_none\(\)", "", "knowledge_growth::tests::episode_requires_verdict_and_receipt"),
    control("episode-no-receipt", "knowledge_growth.rs", r"\|\| self\s*\.receipt_sha256\s*\.as_ref\(\)\s*\.is_none_or\(\|s\| !digest\(s\)\)", "", "knowledge_growth::tests::episode_requires_verdict_and_receipt"),
    control("tier-one-cross-subject", "knowledge_growth.rs", r"crate::transform::sha256_hex\(&original\) != run.inputs", "false", "knowledge_growth::tests::tier_one_proof_is_bound_to_its_subject"),
    control("refuted-advice", "knowledge_workflow.rs", r"c.state == State::Promoted && applies", "(c.state == State::Promoted || c.state==State::Refuted) && applies", "knowledge_store::tests::refuted_record_is_never_advice"),
    control("proved-without-artifact", "knowledge_growth.rs", r"pub\(crate\) fn replay_proof\((.*?)\) -> Result<\(\), &'static str> \{", "\\g<0> return Ok(());", "knowledge_growth::tests::proved_status_requires_a_replaying_artifact"),
    control("profile-direct-advice", "knowledge_store.rs", r"let telemetry = crate::knowledge_growth::telemetry\(&next\);", "if !next.facts.is_empty(){for c in &mut next.candidates{c.state=State::Promoted;}} let telemetry=crate::knowledge_growth::telemetry(&next);", "knowledge_metrics::tests::profiles_are_not_strategy_advice"),
    control("reuse-without-replay", "knowledge_growth.rs", r"pub\(crate\) fn replay_for_reuse\((.*?)\) -> Result<\(\), &'static str> \{", "\\g<0> return Ok(());", "knowledge_growth::tests::reused_result_must_replay_receipt"),
    control("points-promotion", "knowledge_assessment.rs", r"pub\(crate\) fn decide\((.*?)\) -> Result<\(Decision, &'static str\), &'static str> \{", "\\g<0> let points=100; if points>=80{return Ok((Decision::Promote,\"points\"));}", "knowledge_assessment::tests::no_applicable_task_never_promotes"),
    control("par2-wall-time", "knowledge_metrics.rs", r"fn par2\(r: &Run\) -> u64 \{", "fn par2(r: &Run) -> u64 { let wall_time=1; return wall_time;", "knowledge_metrics::tests::par2_uses_deterministic_work"),
    control("marginal-on-proposing", "knowledge_metrics.rs", r"&& f.source == population", "", "knowledge_metrics::tests::marginal_contribution_uses_held_out_cases"),
    control("refusal-as-unsolved", "knowledge_metrics.rs", r"\.filter\(\|f\| f.run.verdict != Verdict::Refused\)", ".filter(|_| true)", "knowledge_metrics::tests::refused_runs_are_out_of_scope"),
    control("renamed-proposer-held-out", "knowledge_replay.rs", r"self.proposing_subjects.contains\(&t.subject.sha256\)", "false", "knowledge_replay::tests::renamed_proposing_subject_is_not_held_out"),
]


def command(arguments, cwd, env, log):
    result = subprocess.run(PREFIX + arguments, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                            capture_output=True, timeout=900, check=False)
    log.write_bytes(result.stdout + b"\n" + result.stderr)
    return result


def build(work, env, log):
    result = command(["cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-cli", "--bin", "zeno-fcis", "--no-run", "--message-format=json"], work, env, log)
    executable = None
    for line in result.stdout.splitlines():
        try:
            item = json.loads(line)
        except ValueError:
            continue
        if item.get("reason") == "compiler-artifact" and item.get("target", {}).get("name") == "zeno-fcis" and item.get("profile", {}).get("test"):
            executable = item.get("executable") or executable
    return result.returncode, executable


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--git-dir", type=Path, required=True)
    parser.add_argument("--target", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--only", nargs="+", choices=[c["name"] for c in CONTROLS])
    args = parser.parse_args()
    selected = [c for c in CONTROLS if args.only is None or c["name"] in args.only]
    args.out.mkdir(mode=0o700, parents=True, exist_ok=False)
    env = dict(os.environ, CARGO_TARGET_DIR=str(args.target.resolve()), CARGO_BUILD_JOBS="3", CARGO_INCREMENTAL="0")
    with tempfile.TemporaryDirectory(prefix="kb-controls-") as scratch:
        work = Path(scratch) / "source"
        work.mkdir()
        archive = Path(scratch) / "source.tar"
        with archive.open("wb") as file:
            subprocess.run(PREFIX + ["git", "--git-dir=" + str(args.git_dir.resolve()), "archive", "HEAD"], stdout=file, check=True)
        with tarfile.open(archive) as file:
            file.extractall(work, filter="data")
        code, executable = build(work, env, args.out / "baseline-build.log")
        if code or not executable:
            raise SystemExit("control baseline did not build")
        tests = sorted({test for spec in selected for test in spec["tests"]})
        for index, test in enumerate(tests):
            result = command([executable, test, "--exact"], work, env, args.out / f"baseline-{index}.log")
            if result.returncode or b"1 passed" not in result.stdout:
                raise SystemExit("control baseline test failed")
        rows = []
        for spec in selected:
            path = work / spec["file"]
            before = path.read_text()
            after, count = re.subn(spec["pattern"], spec["replacement"], before, count=1, flags=re.DOTALL)
            if count != 1:
                raise SystemExit("control patch did not match: " + spec["name"])
            patch = "".join(difflib.unified_diff(before.splitlines(True), after.splitlines(True), fromfile=spec["file"], tofile=spec["file"]))
            (args.out / (spec["name"] + ".patch")).write_text(patch)
            path.write_text(after)
            try:
                code, executable = build(work, env, args.out / (spec["name"] + "-build.log"))
                tests = []
                if code == 0 and executable:
                    for index, test in enumerate(spec["tests"]):
                        result = command([executable, test, "--exact"], work, env, args.out / (spec["name"] + f"-test-{index}.log"))
                        tests.append({"name": test, "exit": result.returncode, "failed_named_test": result.returncode != 0 and b"1 failed" in result.stdout and test.encode() in result.stdout})
                killed = code == 0 and bool(tests) and all(test["failed_named_test"] for test in tests)
                row = {"control": spec["name"], "build_exit": code, "tests": tests, "killed": killed, "patch_sha256": hashlib.sha256(patch.encode()).hexdigest()}
                rows.append(row)
                print(json.dumps(row), flush=True)
                (args.out / "controls.json").write_text(json.dumps(rows, indent=2) + "\n")
                if not killed:
                    raise SystemExit("control survived or did not build: " + spec["name"])
            finally:
                path.write_text(before)
    print(json.dumps({"status": "passed", "built_and_killed": len(rows)}), flush=True)


if __name__ == "__main__":
    main()
