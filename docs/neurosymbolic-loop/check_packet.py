#!/usr/bin/env python3
"""Bounded documentation checks; no product, model, compiler or proof test."""
from pathlib import Path
import argparse
import hashlib
import importlib.util
import json
import posixpath
import re

BASE = Path(__file__).resolve().parent
RECEIPT = BASE / "documentation-checks.json"
SPEC_NAMES = [
    "00-contract.md", "01-loop.md", "02-checker-feedback.md",
    "03-resources-resume.md", "04-integration.md", "05-qualification-evaluation.md",
]
REQUIRED = [
    "README.md", "DESIGN.md", "NOTE.md", "CLAUDE_HANDOFF.md", "CHECKLIST.md",
    "paper.md", "paper.tex", "render_paper.py", "check_packet.py", "specs/INDEX.md",
] + ["specs/" + name for name in SPEC_NAMES]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference-docs", type=Path, required=True)
    args = parser.parse_args()
    failures, links, identifiers = [], [], []
    for name in REQUIRED:
        if not (BASE / name).is_file():
            failures.append("Missing " + name)
    for name, prefix, count in zip(SPEC_NAMES, ["NSC", "NSL", "NSF", "NSR", "NSM", "NSE"], [8, 8, 9, 10, 8, 10]):
        found = re.findall(r"^- \*\*(NS[A-Z]-\d{3}):\*\*", (BASE / "specs" / name).read_text(), re.M)
        expected = [f"{prefix}-{i:03d}" for i in range(1, count + 1)]
        if found != expected:
            failures.append("Requirement sequence " + name)
        identifiers.extend(found)
    if len(identifiers) != 53 or len(set(identifiers)) != 53:
        failures.append("Requirement ID count/uniqueness")
    for source in sorted(BASE.rglob("*.md")):
        for target in re.findall(r"\[[^\]]+\]\(([^)]+)\)", source.read_text()):
            if target.startswith(("http:", "https:", "#")):
                continue
            target = target.split("#", 1)[0]
            logical = posixpath.normpath("neurosymbolic-loop/" + source.relative_to(BASE).parent.as_posix() + "/" + target)
            if logical.startswith("neurosymbolic-loop/"):
                actual = BASE / logical[len("neurosymbolic-loop/"):]
            else:
                actual = args.reference_docs / logical
            ok = actual.is_file() or actual == RECEIPT
            links.append({"source": str(source.relative_to(BASE)), "target": target, "exists_or_receipt_to_write": ok})
            if not ok:
                failures.append("Broken local link " + str(source.relative_to(BASE)) + " -> " + target)
    spec = importlib.util.spec_from_file_location("paper_renderer", BASE / "render_paper.py")
    renderer = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(renderer)
    manuscript = (BASE / "paper.md").read_bytes()
    tex = (BASE / "paper.tex").read_text()
    regenerated_equal = renderer.render(manuscript) == tex
    if not regenerated_equal:
        failures.append("Markdown/TeX regeneration mismatch")
    assertions = {
        "named_author": r"\author{Dana Edwards}" in tex,
        "pdf_author_metadata": "pdfauthor={Dana Edwards}" in tex,
        "draft_date": "Draft---October 4, 2026" in tex,
        "markdown_digest_embedded": hashlib.sha256(manuscript).hexdigest() in tex,
        "factory_title": "Toward a Neurosymbolic Software Factory" in tex,
        "formal_core_foundation": "formally verified functional core" in tex,
        "incomplete_v2": "V2 qualification is still incomplete" in tex,
        "historical_pin": "a69ed8db594d95279a46bff0f65185ef67d51f98" in tex,
        "conditional_proofs": "paper-level arguments" in tex,
        "separate_evidence_table": "Current Python analysis/hash replay" in tex,
        "no_online_neural_study": "there is no online neural proposer" in tex,
    }
    failures.extend("Missing manuscript marker " + k for k, v in assertions.items() if not v)
    files = {}
    for name in REQUIRED + ["original-paper-extracted.txt"]:
        path = BASE / name
        if path.is_file():
            data = path.read_bytes()
            files[name] = {"bytes": len(data), "newline_count": data.count(b"\n"), "sha256": hashlib.sha256(data).hexdigest()}
    receipt = {
        "kind": "documentation-only-checks-v1", "date": "2026-10-04",
        "status": "PASS" if not failures else "FAIL", "failures": failures,
        "model_role": "gpt-6-astra/max; confirmed by parent spawn metadata",
        "required_files": len(REQUIRED), "requirement_ids": identifiers,
        "markdown_tex_regeneration_equal": regenerated_equal,
        "manuscript_markers": assertions, "local_links": links, "files": files,
        "limits": [
            "Text/link/digest checks do not qualify runtime implementation or prove semantic correctness.",
            "No Rust optimizer, native test, Lean/Verus proof, model experiment or product build was run by this task.",
            "Parent owns final PDF compilation/visual review and integrated evidence.",
            "Receipt itself is excluded from file hashes to avoid self-reference.",
        ],
    }
    RECEIPT.write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({"status": receipt["status"], "required_files": len(REQUIRED), "requirements": len(identifiers), "local_links": len(links), "regeneration_equal": regenerated_equal, "failures": failures, "line_counts": {name: entry["newline_count"] for name, entry in files.items()}}, indent=2))
    raise SystemExit(bool(failures))


if __name__ == "__main__":
    main()
