#!/usr/bin/env python3
"""Boundary/claim-separation controls for feasibility records, not algorithm tests."""
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("feasibility_record_checker", ROOT / "tools/check_research_feasibility.py")
GATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GATE)


def draft():
    return GATE.load_record(ROOT / "experiments/feasibility-template.json")


def negative():
    return {"observation": "Candidate slower at measured sizes", "scope": "Declared finite workload only",
            "classification": "unresolved", "cause_status": "unknown",
            "explanation": "No causal isolation experiment has been performed",
            "evidence": [{"path": "samples.json", "sha256": "a" * 64, "kind": "measurement"}],
            "next_test": "Compare preprocessing separately with matched compiler and limits"}


def complete(root):
    """Synthetic metadata fixture; never represents qualified algorithm evidence."""
    p = root / "fixture.txt"
    p.write_text("Synthetic record-check fixture; no mathematical or performance claim.\n")
    digest = hashlib.sha256(p.read_bytes()).hexdigest()
    def refs(*kinds):
        return [{"path": p.name, "sha256": digest, "kind": kind} for kind in kinds]
    def bound(name, data):
        artifact = root / name
        artifact.write_bytes(data)
        return {"path": name, "sha256": hashlib.sha256(data).hexdigest(), "kind": "artifact"}
    x = draft()
    candidate = bound("candidate.dat", b"synthetic candidate")
    baseline = bound("baseline.dat", b"synthetic baseline")
    toolchain = {"description": "Synthetic fixture toolchain", "path": p.name, "sha256": digest}
    x["candidate"].update(name="synthetic-candidate", revision="1" * 40,
        artifact_sha256=candidate["sha256"], artifact=candidate, toolchain=dict(toolchain))
    x["baseline"].update(name="synthetic-baseline", revision="3" * 40,
        artifact_sha256=baseline["sha256"], artifact=baseline, toolchain=dict(toolchain),
        qualification_scope="Synthetic fixture scope", qualification_evidence=refs("qualification"),
        selection_rationale="Synthetic inventory review only", selection_evidence=refs("review"), inventory_as_of="2026-10-27T00:00:00Z")
    development = bound("development.dat", b"synthetic development case")
    held_out = bound("held-out.dat", b"synthetic held-out case")
    x["workloads"].update(representative=[{"id": "development", **{k: development[k] for k in ("sha256", "path")}}],
        held_out=[{"id": "held-out", **{k: held_out[k] for k in ("sha256", "path")}}], frozen_plan_evidence=refs("review"))
    kinds = {"correctness": ("replay",), "asymptotic_complexity": ("proof",),
             "concrete_resource_model": ("measurement",), "empirical_crossover": ("measurement",),
             "assurance_preservation": ("replay", "review")}
    for axis in GATE.AXES:
        x["components"][axis].update(status="supported", analysis="Synthetic declared assessment only",
                                      evidence=refs(*kinds[axis]), open_questions=[])
    x["disposition"] = "request-independent-review"
    return x


class FeasibilityRecordTests(unittest.TestCase):
    def test_template_remains_valid_incomplete_research(self):
        report = GATE.check(draft())
        self.assertFalse(report["ready_for_independent_review"])
        self.assertEqual(set(GATE.AXES), set(report["declared_axes"]))
        self.assertEqual("none", report["qualification_or_promotion_authority"])

    def test_each_missing_axis_is_refused(self):
        for axis in GATE.AXES:
            with self.subTest(axis=axis):
                x = draft(); del x["components"][axis]
                with self.assertRaisesRegex(ValueError, "five independent"):
                    GATE.check(x)

    def test_aggregate_score_cannot_replace_independent_components(self):
        x = draft(); x["overall_score"] = 100
        with self.assertRaisesRegex(ValueError, "unexpected/missing"):
            GATE.check(x)

    def test_declared_support_requires_applicable_evidence(self):
        x = draft(); x["components"]["correctness"]["status"] = "supported"
        with self.assertRaisesRegex(ValueError, "requires evidence"):
            GATE.check(x)
        x["components"]["correctness"]["evidence"] = negative()["evidence"]
        with self.assertRaisesRegex(ValueError, "applicable evidence kind"):
            GATE.check(x)

    def test_held_out_overlap_by_identity_or_bytes_is_refused(self):
        for role in ("representative", "used_for_selection"):
            for same_id in (True, False):
                with self.subTest(role=role, same_id=same_id):
                    x = draft()
                    x["workloads"][role] = [{"id": "seen", "sha256": "a" * 64, "path": None}]
                    x["workloads"]["held_out"] = [{"id": "seen" if same_id else "renamed", "sha256": ("b" if same_id else "a") * 64, "path": None}]
                    with self.assertRaisesRegex(ValueError, "held-out workload|conflicting content"):
                        GATE.check(x)

    def test_timing_only_fundamental_diagnosis_is_refused(self):
        x = draft(); item = negative(); item.update(classification="fundamental", cause_status="supported")
        x["negative_results"] = [item]
        with self.assertRaisesRegex(ValueError, "fundamental limitation"):
            GATE.check(x)

    def test_negative_result_is_retained_without_improvement(self):
        x = draft(); x["negative_results"] = [negative()]; x["disposition"] = "negative-result"
        x["components"]["empirical_crossover"].update(status="refuted", evidence=negative()["evidence"])
        report = GATE.check(x)
        self.assertEqual(1, report["negative_results_retained"])
        self.assertFalse(report["ready_for_independent_review"])
        x["negative_results"] = []
        with self.assertRaisesRegex(ValueError, "retained negative result"):
            GATE.check(x)

    def test_complete_bound_metadata_does_not_certify_claims(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); x = complete(root)
            report = GATE.check(x, root)
            self.assertTrue(report["ready_for_independent_review"])
            self.assertFalse(report["algorithm_claims_independently_evaluated"])
            self.assertEqual("none", report["qualification_or_promotion_authority"])
            self.assertFalse(GATE.check(x)["ready_for_independent_review"])

    def test_missing_qualified_selection_blocks_review(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); x = complete(root); x["baseline"]["selection_evidence"] = []
            report = GATE.check(x, root)
            self.assertFalse(report["ready_for_independent_review"])
            self.assertIn("strongest qualified applicable baseline and selection evidence", report["missing_review_prerequisites"])

    def test_evidence_byte_mutation_and_symlink_are_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); x = complete(root); p = root / "fixture.txt"; old = p.read_bytes()
            p.write_bytes(old + b"changed")
            with self.assertRaisesRegex(ValueError, "evidence bytes changed"):
                GATE.check(x, root)
            p.write_bytes(old); link = root / "link.txt"; link.symlink_to(p.name)
            x["components"]["correctness"]["evidence"][0]["path"] = link.name
            with self.assertRaisesRegex(ValueError, "symlink"):
                GATE.check(x, root)

    def test_path_escape_is_refused_without_reading_it(self):
        x = draft(); x["negative_results"] = [negative()]
        x["negative_results"][0]["evidence"][0]["path"] = "../unrelated.txt"
        with self.assertRaisesRegex(ValueError, "unsafe evidence path"):
            GATE.check(x)

    def test_backend_artifact_hash_is_bound_to_actual_bytes(self):
        for role in ("candidate", "baseline"):
            with self.subTest(role=role), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp); x = complete(root)
                x[role]["artifact_sha256"] = "9" * 64
                with self.assertRaisesRegex(ValueError, "artifact identity"):
                    GATE.check(x, root)
                x[role]["artifact"]["sha256"] = "9" * 64
                with self.assertRaisesRegex(ValueError, "evidence bytes changed"):
                    GATE.check(x, root)

    def test_workload_digest_is_bound_to_retained_bytes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); x = complete(root)
            x["workloads"]["held_out"][0]["sha256"] = "8" * 64
            with self.assertRaisesRegex(ValueError, "evidence bytes changed"):
                GATE.check(x, root)

    def test_stable_id_cannot_change_contents_across_development_roles(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); x = complete(root)
            row = dict(x["workloads"]["representative"][0])
            x["workloads"]["used_for_selection"] = [dict(row)]
            self.assertTrue(GATE.check(x, root)["ready_for_independent_review"])
            x["workloads"]["used_for_selection"][0]["sha256"] = "7" * 64
            with self.assertRaisesRegex(ValueError, "conflicting content"):
                GATE.check(x, root)

    def test_unbound_subject_or_toolchain_metadata_stays_incomplete(self):
        for role in ("candidate", "baseline"):
            for field in ("artifact", "toolchain"):
                with self.subTest(role=role, field=field), tempfile.TemporaryDirectory() as tmp:
                    root = Path(tmp); x = complete(root); x[role][field] = None
                    self.assertFalse(GATE.check(x, root)["ready_for_independent_review"])
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); x = complete(root)
            x["workloads"]["held_out"][0]["path"] = None
            self.assertFalse(GATE.check(x, root)["ready_for_independent_review"])
            x["candidate"]["toolchain"] = "Unrecorded"
            with self.assertRaisesRegex(ValueError, "toolchain descriptor"):
                GATE.check(x, root)

    def test_unknown_version_and_duplicate_fields_refuse(self):
        x = draft(); x["schema"] = "zeno-fcis/computational-feasibility/999"
        with self.assertRaisesRegex(ValueError, "unsupported assessment"):
            GATE.check(x)
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "duplicate.json"; p.write_text('{"schema":"a","schema":"b"}')
            with self.assertRaisesRegex(ValueError, "duplicate JSON"):
                GATE.load_record(p)

    def test_oversized_record_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "large.json"; p.write_bytes(b" " * (GATE.MAX_RECORD + 1))
            with self.assertRaisesRegex(ValueError, "exceeds 2 MiB"):
                GATE.load_record(p)

    def test_real_cli_distinguishes_valid_draft_from_review_ready(self):
        command = [sys.executable, "-I", "-B", str(ROOT / "tools/check_research_feasibility.py"), str(ROOT / "experiments/feasibility-template.json")]
        for flags, expected in (([], 0), (["--require-ready"], 1)):
            result = subprocess.run(command + flags, capture_output=True, text=True, timeout=10, check=False)
            self.assertEqual(expected, result.returncode, result.stderr)
            self.assertTrue(json.loads(result.stdout)["record_valid"])


if __name__ == "__main__":
    unittest.main()
