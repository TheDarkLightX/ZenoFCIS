#!/usr/bin/env python3
"""Experimental, non-authoritative MAP-Elites archive for ZenoFCIS finite programs.

Only ZenoFCIS `transform check`/`transform replay` can establish admissibility.
The archive neither implements program semantics nor grants application authority.
Python 3.10+, stdlib only. One writer per archive at a time.
"""
from __future__ import annotations

import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
from typing import Any, Iterator

SCHEMA = "zeno-fcis/elite-archive/0.1"
ASSURANCE = "candidate equivalent to pinned original over complete finite declared input domain; not proof of intended behavior"
RECEIPT_SCHEMA = "zeno-fcis/transform-receipt/1"
REPORT_SCHEMA = "zeno-fcis/transform-result/1"
MAX_PROGRAM_BYTES = 64 * 1024
MAX_SPEC_BYTES = 1024 * 1024
MAX_TOOL_BYTES = 512 * 1024 * 1024
MAX_STDOUT_BYTES = 1024 * 1024
MAX_EVENTS = 256
MAX_TUPLES = 1_000_000
MAX_CELLS = 1024
DEFAULT_TIMEOUT = 120


class ArchiveError(Exception):
    """Fail closed. Do not emit a positive verification verdict on this error."""


def digest(contents: bytes) -> str:
    return hashlib.sha256(contents).hexdigest()


def canonical(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False) + "\n").encode()


def bounded_file(path: Path, limit: int) -> bytes:
    """Reject symlinks, non-regular files and oversize inputs before opening."""
    if path.is_symlink():
        raise ArchiveError(f"symlinks are not supported: {path}")
    try:
        info = path.stat()
        if not stat.S_ISREG(info.st_mode) or info.st_size > limit:
            raise ArchiveError(f"not a regular file within {limit} bytes: {path}")
        with path.open("rb") as handle:
            result = handle.read(limit + 1)
    except OSError as exc:
        raise ArchiveError(f"cannot read {path}: {exc}") from exc
    if len(result) > limit:
        raise ArchiveError(f"file exceeds {limit} bytes: {path}")
    return result


def cli_binary(path: str | None) -> Path:
    chosen = path or os.environ.get("ZENO_FCIS_CLI") or shutil.which("zeno-fcis")
    if not chosen:
        raise ArchiveError("zeno-fcis CLI not found (pass --cli or set ZENO_FCIS_CLI)")
    result = Path(chosen).expanduser().resolve()
    if not result.is_file() or not os.access(result, os.X_OK):
        raise ArchiveError(f"CLI not executable: {result}")
    return result


def cli_sha256(cli: Path) -> str:
    """Bounded streaming hash; never loads a potentially large Rust binary."""
    if not cli.is_file() or cli.stat().st_size > MAX_TOOL_BYTES:
        raise ArchiveError(f"CLI must be a regular executable <= {MAX_TOOL_BYTES} bytes")
    hasher = hashlib.sha256()
    length = 0
    try:
        with cli.open("rb") as f:
            while chunk := f.read(1024 * 1024):
                length += len(chunk)
                if length > MAX_TOOL_BYTES:
                    raise ArchiveError("CLI exceeded maximum executable size")
                hasher.update(chunk)
    except OSError as exc:
        raise ArchiveError(f"cannot read CLI: {exc}") from exc
    return hasher.hexdigest()


def write_new(path: Path, contents: bytes) -> None:
    """Write once and fsync. Existing files cannot be overwritten or followed."""
    try:
        with path.open("xb") as file:
            file.write(contents)
            file.flush()
            os.fsync(file.fileno())
    except OSError as exc:
        raise ArchiveError(f"failed to write new file {path}: {exc}") from exc


def write_manifest(root: Path, manifest: dict[str, Any]) -> None:
    """Atomic replacement. Orphaned blobs after a crash do not enter the archive."""
    with tempfile.NamedTemporaryFile(mode="wb", dir=root, prefix=".state-", delete=False) as f:
        temp = Path(f.name)
        try:
            f.write(canonical(manifest))
            f.flush()
            os.fsync(f.fileno())
        except BaseException:
            temp.unlink(missing_ok=True)
            raise
    try:
        os.replace(temp, root / "archive.json")
    except BaseException:
        temp.unlink(missing_ok=True)
        raise


@contextlib.contextmanager
def lock(root: Path) -> Iterator[None]:
    """Portable exclusive directory lock. Stale locks require manual inspection."""
    guard = root / ".writer-lock"
    try:
        guard.mkdir()
    except FileExistsError as exc:
        raise ArchiveError("archive is locked; inspect active writers before removing .writer-lock") from exc
    try:
        yield
    finally:
        guard.rmdir()


def init_archive(root: Path, original_path: Path, cli: Path, spec_path: Path | None,
                 max_input_tuples: int, node_bin_width: int, byte_bin_width: int) -> dict[str, Any]:
    if not 1 <= max_input_tuples <= MAX_TUPLES:
        raise ArchiveError(f"max_input_tuples must be 1..{MAX_TUPLES}")
    if not 1 <= node_bin_width <= 256 or not 1 <= byte_bin_width <= 4096:
        raise ArchiveError("node bin width must be 1..256 and byte bin width 1..4096")
    original = bounded_file(original_path, MAX_PROGRAM_BYTES)
    if not original:
        raise ArchiveError("original program is empty")
    spec = bounded_file(spec_path, MAX_SPEC_BYTES) if spec_path else None
    checker_sha = cli_sha256(cli)
    if root.exists():
        raise ArchiveError(f"archive destination exists; refusing overwrite: {root}")
    root.mkdir(parents=True)
    (root / "candidates").mkdir()
    (root / "receipts").mkdir()
    write_new(root / "original.zcve", original)
    if spec is not None:
        write_new(root / "spec.snapshot", spec)
    state: dict[str, Any] = {
        "schema": SCHEMA,
        "authority": "none",
        "assurance": ASSURANCE,
        "original": {"sha256": digest(original), "bytes": len(original), "path": "original.zcve"},
        "specification": {"sha256": digest(spec) if spec is not None else None,
                          "path": "spec.snapshot" if spec is not None else None,
                          "review_status": "operator-supplied-unattested" if spec is not None else "not-supplied"},
        "checker": {"cli_sha256": checker_sha, "checker_name": cli.name,
                    "max_input_tuples": max_input_tuples},
        "descriptors": {"node_bin_width": node_bin_width,
                        "byte_bin_width": byte_bin_width, "max_bin": 31},
        "cells": {},
        "candidates": {},
        "attempts": [],
        "reviews": {},
    }
    write_manifest(root, state)
    return state


def read_manifest(root: Path) -> dict[str, Any]:
    raw = bounded_file(root / "archive.json", 2 * 1024 * 1024)
    try:
        state = json.loads(raw)
    except (ValueError, UnicodeDecodeError) as exc:
        raise ArchiveError("archive manifest is not valid JSON") from exc
    if not isinstance(state, dict) or state.get("schema") != SCHEMA or state.get("authority") != "none" or state.get("assurance") != ASSURANCE:
        raise ArchiveError("archive schema/authority/assurance mismatch")
    for key in ("original", "specification", "checker", "descriptors", "cells", "candidates", "attempts", "reviews"):
        if key not in state:
            raise ArchiveError(f"manifest missing field {key}")
    if not isinstance(state["cells"], dict) or len(state["cells"]) > MAX_CELLS:
        raise ArchiveError("invalid cell map")
    if not isinstance(state["attempts"], list) or len(state["attempts"]) > MAX_EVENTS:
        raise ArchiveError("invalid attempt ledger")
    original = bounded_file(root / "original.zcve", MAX_PROGRAM_BYTES)
    if digest(original) != state["original"].get("sha256") or len(original) != state["original"].get("bytes"):
        raise ArchiveError("pinned original has changed")
    spec_state = state["specification"]
    if spec_state["sha256"] is not None:
        spec_bytes = bounded_file(root / "spec.snapshot", MAX_SPEC_BYTES)
        if digest(spec_bytes) != spec_state["sha256"]:
            raise ArchiveError("specification snapshot has changed")
    if not 1 <= state["checker"].get("max_input_tuples", 0) <= MAX_TUPLES:
        raise ArchiveError("manifest domain cap is invalid")
    desc = state["descriptors"]
    if not 1 <= desc.get("node_bin_width", 0) <= 256 or not 1 <= desc.get("byte_bin_width", 0) <= 4096 or desc.get("max_bin") != 31:
        raise ArchiveError("manifest descriptor definition is invalid")
    if not isinstance(state["candidates"], dict):
        raise ArchiveError("invalid candidate records")
    if len(state["candidates"]) > MAX_EVENTS:
        raise ArchiveError("candidate count exceeded")
    for sha, record in state["candidates"].items():
        if sha != record.get("sha256") or not is_hash(sha):
            raise ArchiveError("invalid candidate reference")
        candidate = bounded_file(root / "candidates" / f"{sha}.zcve", MAX_PROGRAM_BYTES)
        receipt = bounded_file(root / "receipts" / f"{sha}.json", 64 * 1024)
        if digest(candidate) != sha or digest(receipt) != record.get("receipt_sha256"):
            raise ArchiveError("stored candidate or receipt has changed")
        verify_receipt_fields(state, record, receipt)
        if record["cell"] != cell_for(state, record["metrics"]):
            raise ArchiveError("stored candidate cell metadata has changed")
    expected_cells: dict[str, str] = {}
    for sha, record in state["candidates"].items():
        cell = record["cell"]
        previous = expected_cells.get(cell)
        if previous is None or rank(record["metrics"], sha) < rank(state["candidates"][previous]["metrics"], previous):
            expected_cells[cell] = sha
    if state["cells"] != expected_cells:
        raise ArchiveError("archive elites do not equal best recorded candidate per descriptor cell")
    if not isinstance(state["reviews"], dict):
        raise ArchiveError("invalid human review map")
    for sha, review in state["reviews"].items():
        if sha not in state["candidates"] or review.get("decision") not in ("preferred", "rejected", "needs-work"):
            raise ArchiveError("invalid human review reference")
        if not isinstance(review.get("reviewer"), str) or len(review["reviewer"]) > 80 or review.get("attestation") != "operator-supplied-unverified-identity":
            raise ArchiveError("invalid human reviewer provenance")
        if not isinstance(review.get("rationale"), str) or digest(review["rationale"].encode()) != review.get("rationale_sha256"):
            raise ArchiveError("human review rationale has changed")
    return state


def is_hash(s: Any) -> bool:
    return isinstance(s, str) and len(s) == 64 and all(c in "0123456789abcdef" for c in s)


def ensure_cli(state: dict[str, Any], cli: Path) -> None:
    if cli_sha256(cli) != state["checker"]["cli_sha256"]:
        raise ArchiveError("CLI binary differs from pinned checker; open a fresh archive for a new checker")


def positive_int(value: Any, label: str, maximum: int = 1_000_000_000) -> int:
    if type(value) is not int or not 1 <= value <= maximum:
        raise ArchiveError(f"invalid {label} in checker evidence")
    return value


def metrics_from_receipt(rec: dict[str, Any]) -> dict[str, int]:
    try:
        candidate = rec["candidate"]
        usage = rec["usage"]
        nodes = positive_int(candidate["nodes"], "candidate.nodes")
        byte_count = positive_int(candidate["bytes"], "candidate.bytes", MAX_PROGRAM_BYTES)
        steps = positive_int(usage["max_steps"]["candidate"], "usage.max_steps.candidate")
    except (KeyError, TypeError) as exc:
        raise ArchiveError("missing cost or usage fields in checker receipt") from exc
    return {"nodes": nodes, "bytes": byte_count, "max_steps": steps}


def cell_for(state: dict[str, Any], metrics: dict[str, int]) -> str:
    desc = state["descriptors"]
    n = min((metrics["nodes"] - 1) // desc["node_bin_width"], desc["max_bin"])
    b = min((metrics["bytes"] - 1) // desc["byte_bin_width"], desc["max_bin"])
    return f"n{n:02d}-b{b:02d}"


def rank(metrics: dict[str, int], sha: str) -> tuple[int, int, int, str]:
    """Deterministic fitness within a cell; lower is better."""
    return (metrics["max_steps"], metrics["bytes"], metrics["nodes"], sha)


def verify_receipt_fields(state: dict[str, Any], record: dict[str, Any], receipt: bytes) -> None:
    try:
        r = json.loads(receipt)
        if r["schema"] != RECEIPT_SCHEMA or r["verdict"] != "equivalent" or r["authority"] != "none":
            raise ArchiveError("receipt is not exhaustive equivalent evidence")
        if r["original"]["sha256"] != state["original"]["sha256"] or r["original"]["bytes"] != state["original"]["bytes"]:
            raise ArchiveError("receipt bound to another original")
        positive_int(r["original"]["nodes"], "original.nodes")
        if r["checker"]["semantics"] != "zeno-fcis/transform-check/1" or not is_hash(r["checker"]["evaluator_identity"]):
            raise ArchiveError("unsupported checker semantics/evaluator identity")
        if r["domain"]["enumeration"] != "ordered-product-last-input-fastest-v1":
            raise ArchiveError("receipt has unsupported domain enumeration")
        if r["candidate"]["sha256"] != record["sha256"]:
            raise ArchiveError("receipt bound to another candidate")
        if r["candidate"]["bytes"] != record["metrics"]["bytes"]:
            raise ArchiveError("receipt cost mismatch")
        if r["limits"]["input_tuples"] != state["checker"]["max_input_tuples"]:
            raise ArchiveError("receipt has different domain cap")
        domain_size = positive_int(r["domain"]["size"], "domain.size")
        if domain_size != r["inputs_checked"] or domain_size > state["checker"]["max_input_tuples"]:
            raise ArchiveError("receipt did not cover declared finite domain")
        if metrics_from_receipt(r) != record["metrics"]:
            raise ArchiveError("stored metrics disagree with checker receipt")
    except (KeyError, TypeError, ValueError) as exc:
        raise ArchiveError("invalid receipt binding or shape") from exc


def run_cli(cli: Path, args: list[str], timeout: int) -> tuple[int, dict[str, Any]]:
    try:
        completed = subprocess.run([str(cli), *args], stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, timeout=timeout, check=False)
    except (subprocess.TimeoutExpired, OSError) as exc:
        raise ArchiveError(f"checker unavailable or timed out: {type(exc).__name__}") from exc
    if len(completed.stdout) > MAX_STDOUT_BYTES:
        raise ArchiveError("checker response too large")
    try:
        report = json.loads(completed.stdout)
    except (ValueError, UnicodeDecodeError) as exc:
        raise ArchiveError("checker returned invalid JSON") from exc
    if not isinstance(report, dict):
        raise ArchiveError("checker response is not an object")
    return completed.returncode, report


def check_and_replay(root: Path, state: dict[str, Any], cli: Path,
                     candidate: bytes, timeout: int) -> tuple[dict[str, Any], bytes | None]:
    """A successful check AND replay, on the same copy, are mandatory."""
    with tempfile.TemporaryDirectory(prefix=".checking-", dir=root) as td:
        temp = Path(td)
        cand = temp / "proposal.zcve"
        receipt_path = temp / "receipt.json"
        write_new(cand, candidate)
        args = ["transform", "check", "--original", str(root / "original.zcve"),
                "--candidate", str(cand), "--max-input-tuples",
                str(state["checker"]["max_input_tuples"]), "--receipt", str(receipt_path)]
        exit_code, report = run_cli(cli, args, timeout)
        # No positive classification unless the authentic CLI binary reported a
        # complete equivalence and produced the binding receipt.
        if exit_code != 0 or report.get("schema") != REPORT_SCHEMA or report.get("status") != "equivalent" or report.get("authority") != "none":
            return {"accepted": False, "checker_status": report.get("status", "invalid-response"),
                    "exit_code": exit_code}, None
        detail = report.get("detail")
        if not isinstance(detail, dict) or not receipt_path.exists():
            raise ArchiveError("checker claimed equivalence without a receipt")
        receipt_bytes = bounded_file(receipt_path, 64 * 1024)
        if not receipt_bytes:
            raise ArchiveError("checker returned empty receipt")
        if digest(receipt_bytes) != detail.get("receipt_sha256"):
            raise ArchiveError("receipt digest differs from checker response")
        if report.get("original_sha256") != state["original"]["sha256"] or report.get("candidate_sha256") != digest(candidate):
            raise ArchiveError("checker response digest mismatch")
        try:
            structured = json.loads(receipt_bytes)
        except (ValueError, UnicodeDecodeError) as exc:
            raise ArchiveError("checker produced malformed receipt") from exc
        if detail.get("receipt") != structured:
            raise ArchiveError("checker receipt file and report differ")
        metrics = metrics_from_receipt(structured)
        record = {"sha256": digest(candidate), "receipt_sha256": digest(receipt_bytes),
                  "metrics": metrics, "cell": cell_for(state, metrics)}
        if metrics["bytes"] != len(candidate):
            raise ArchiveError("checker reported incorrect candidate byte count")
        verify_receipt_fields(state, record, receipt_bytes)
        replay_args = ["transform", "replay", "--receipt", str(receipt_path),
                       "--original", str(root / "original.zcve"), "--candidate", str(cand),
                       "--max-input-tuples", str(state["checker"]["max_input_tuples"])]
        replay_code, replay = run_cli(cli, replay_args, timeout)
        if replay_code != 0 or replay.get("schema") != REPORT_SCHEMA or replay.get("status") != "replayed" or replay.get("authority") != "none" or replay.get("detail", {}).get("receipt_sha256") != digest(receipt_bytes):
            raise ArchiveError("candidate was not successfully replayed")
        if digest(bounded_file(root / "original.zcve", MAX_PROGRAM_BYTES)) != state["original"]["sha256"]:
            raise ArchiveError("pinned original changed during check")
        if cli_sha256(cli) != state["checker"]["cli_sha256"]:
            raise ArchiveError("checker binary changed during check")
        return record, receipt_bytes


def offer(root: Path, cli: Path, candidate_path: Path, source: str,
          timeout: int = DEFAULT_TIMEOUT) -> dict[str, Any]:
    if len(source) > 80 or not source or any(ord(c) < 32 for c in source):
        raise ArchiveError("source label must be printable and at most 80 characters")
    with lock(root):
        state = read_manifest(root)
        ensure_cli(state, cli)
        if len(state["attempts"]) >= MAX_EVENTS:
            raise ArchiveError("attempt budget exhausted: create a new archive")
        candidate = bounded_file(candidate_path, MAX_PROGRAM_BYTES)
        if not candidate:
            raise ArchiveError("empty candidate file")
        sha = digest(candidate)
        if sha in state["candidates"]:
            return {"status": "duplicate-recorded", "sha256": sha,
                    "note": "not freshly replayed; use verify to recheck stored entries"}
        assessed, receipt_bytes = check_and_replay(root, state, cli, candidate, timeout)
        seq = len(state["attempts"]) + 1
        if receipt_bytes is None:
            result = {"seq": seq, "sha256": sha, "source": source,
                      "result": "rejected", "checker_status": assessed["checker_status"],
                      "exit_code": assessed["exit_code"]}
            state["attempts"].append(result)
            write_manifest(root, state)
            return {"status": "rejected", **result}
        record = assessed
        record["source"] = source
        write_new(root / "candidates" / f"{sha}.zcve", candidate)
        write_new(root / "receipts" / f"{sha}.json", receipt_bytes)
        state["candidates"][sha] = record
        cell = record["cell"]
        prior_sha = state["cells"].get(cell)
        improved = prior_sha is None or rank(record["metrics"], sha) < rank(state["candidates"][prior_sha]["metrics"], prior_sha)
        if improved:
            state["cells"][cell] = sha
        result = {"seq": seq, "sha256": sha, "source": source,
                  "result": "elite" if improved else "verified-nonelite", "cell": cell,
                  "receipt_sha256": record["receipt_sha256"]}
        state["attempts"].append(result)
        write_manifest(root, state)
        return {"status": result["result"], "sha256": sha, "cell": cell,
                "metrics": record["metrics"], "receipt_sha256": record["receipt_sha256"],
                "evidence": "complete-finite-equivalence-to-original-replayed",
                "authority": "none"}


def replay_archive(root: Path, cli: Path, timeout: int = DEFAULT_TIMEOUT) -> dict[str, Any]:
    """Independent CLI replay of every elite; fail closed if any check fails."""
    with lock(root):
        state = read_manifest(root)
        ensure_cli(state, cli)
        statuses = []
        if not state["cells"]:
            raise ArchiveError("empty archive: no elites exist to replay")
        for cell, sha in sorted(state["cells"].items()):
            record = state["candidates"][sha]
            receipt_file = root / "receipts" / f"{sha}.json"
            code, report = run_cli(cli, ["transform", "replay", "--receipt", str(receipt_file),
                                         "--original", str(root / "original.zcve"),
                                         "--candidate", str(root / "candidates" / f"{sha}.zcve"),
                                         "--max-input-tuples", str(state["checker"]["max_input_tuples"])], timeout)
            if code != 0 or report.get("schema") != REPORT_SCHEMA or report.get("status") != "replayed" or report.get("authority") != "none" or report.get("detail", {}).get("receipt_sha256") != record["receipt_sha256"]:
                raise ArchiveError(f"FAILED replay of {cell}: {sha}")
            statuses.append({"cell": cell, "sha256": sha, "status": "replayed"})
        ensure_cli(state, cli)
        return {"status": "all-elites-replayed", "authority": "none",
                "scope": "finite equivalence to pinned original, not intended behavior",
                "checker_sha256": state["checker"]["cli_sha256"],
                "elite_count": len(statuses), "candidates_examined": len(state["attempts"]),
                "elites": statuses}


def view(root: Path) -> dict[str, Any]:
    state = read_manifest(root)
    entries = []
    for cell, sha in sorted(state["cells"].items()):
        r = state["candidates"][sha]
        entries.append({"cell": cell, "sha256": sha, "metrics": r["metrics"],
                        "source": r["source"], "receipt_sha256": r["receipt_sha256"],
                        "human_review": state["reviews"].get(sha, {"decision": "not-reviewed"})})
    return {"status": "stored-evidence-not-freshly-replayed", "authority": "none",
            "original_sha256": state["original"]["sha256"],
            "specification": state["specification"],
            "checker_sha256": state["checker"]["cli_sha256"],
            "attempts": len(state["attempts"]), "accepted_candidates": len(state["candidates"]),
            "archive_coverage": len(entries), "elites": entries}


def sweep(root: Path, cli: Path, strategies: list[str], timeout: int) -> dict[str, Any]:
    """Proposal-only e-graph search; candidates are separately checked by offer()."""
    state = read_manifest(root)
    ensure_cli(state, cli)
    outcomes: list[dict[str, Any]] = []
    for strategy in strategies or ["default"]:
        with tempfile.TemporaryDirectory(prefix=".search-", dir=root) as td:
            candidate = Path(td) / "optimized.zcve"
            args = ["optimize", "--program", str(root / "original.zcve"),
                    "--candidate-out", str(candidate), "--max-input-tuples",
                    str(state["checker"]["max_input_tuples"])]
            if strategy != "default":
                args.extend(["--strategy", str(Path(strategy).resolve())])
            code, report = run_cli(cli, args, timeout)
            if code == 0 and report.get("schema") == "zeno-fcis/optimize-result/1" and report.get("status") == "improved" and report.get("authority") == "none" and candidate.is_file():
                # It is not enough that optimize reported acceptance. A separate
                # `transform check` + `transform replay` is mandatory here.
                outcome = offer(root, cli, candidate, "egraph:" + Path(strategy).name, timeout)
            else:
                outcome = {"status": "no-proposal", "exit_code": code,
                           "optimizer_status": report.get("status", "invalid-response")}
            outcomes.append({"strategy": strategy, **outcome})
    return {"status": "search-completed", "authority": "none", "results": outcomes}


def review_candidate(root: Path, sha: str, decision: str, rationale_path: Path,
                     reviewer: str) -> dict[str, Any]:
    """Record operator-curated preference, never a checker result or release approval."""
    if decision not in ("preferred", "rejected", "needs-work"):
        raise ArchiveError("invalid human review decision")
    if not reviewer or len(reviewer) > 80 or any(ord(c) < 32 for c in reviewer):
        raise ArchiveError("reviewer label must be printable and at most 80 characters")
    try:
        rationale = bounded_file(rationale_path, 4096).decode("utf-8")
    except UnicodeDecodeError as exc:
        raise ArchiveError("rationale must be UTF-8") from exc
    if not rationale.strip():
        raise ArchiveError("human review rationale must not be empty")
    with lock(root):
        state = read_manifest(root)
        if sha not in state["candidates"]:
            raise ArchiveError("candidate is not a checked archive entry")
        state["reviews"][sha] = {"decision": decision, "rationale": rationale,
                                 "rationale_sha256": digest(rationale.encode()),
                                 "reviewer": reviewer,
                                 "attestation": "operator-supplied-unverified-identity"}
        write_manifest(root, state)
        return {"status": "review-recorded", "authority": "none", "sha256": sha,
                "decision": decision, "rationale_sha256": digest(rationale.encode()),
                "reviewer": reviewer, "note": "human preference, not formal evidence or adoption"}


def export_elite(root: Path, cli: Path, sha: str, out: Path,
                 timeout: int = DEFAULT_TIMEOUT) -> dict[str, Any]:
    """A manually preferred elite is replayed before creating an export bundle."""
    with lock(root):
        state = read_manifest(root)
        ensure_cli(state, cli)
        if sha not in state["cells"].values():
            raise ArchiveError("selection must be a current elite")
        human = state["reviews"].get(sha)
        if not human or human["decision"] != "preferred":
            raise ArchiveError("export requires a recorded human 'preferred' review")
        record = state["candidates"][sha]
        cand_path = root / "candidates" / f"{sha}.zcve"
        receipt_path = root / "receipts" / f"{sha}.json"
        code, report = run_cli(cli, ["transform", "replay", "--receipt", str(receipt_path),
                                     "--original", str(root / "original.zcve"),
                                     "--candidate", str(cand_path), "--max-input-tuples",
                                     str(state["checker"]["max_input_tuples"])], timeout)
        if code != 0 or report.get("schema") != REPORT_SCHEMA or report.get("status") != "replayed" or report.get("authority") != "none" or report.get("detail", {}).get("receipt_sha256") != record["receipt_sha256"]:
            raise ArchiveError("export was refused: checker replay failed")
        ensure_cli(state, cli)
        if out.exists():
            raise ArchiveError("export directory already exists; refusing overwrite")
        out.mkdir(parents=True)
        write_new(out / "candidate.zcve", bounded_file(cand_path, MAX_PROGRAM_BYTES))
        write_new(out / "receipt.json", bounded_file(receipt_path, 64 * 1024))
        summary = {"schema": "zeno-fcis/elite-selection/0.1", "authority": "none",
                   "status": "exported-for-manual-consideration", "candidate_sha256": sha,
                   "receipt_sha256": record["receipt_sha256"],
                   "original_sha256": state["original"]["sha256"],
                   "checker_sha256": state["checker"]["cli_sha256"],
                   "human_review": human,
                   "note": "equivalence only; not formal correctness of human intent or publication authority"}
        write_new(out / "selection.json", canonical(summary))
        return summary


def output(value: Any) -> None:
    sys.stdout.buffer.write(json.dumps(value, indent=2, sort_keys=True).encode() + b"\n")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Non-authoritative MAP-Elites for ZenoFCIS finite programs")
    parser.add_argument("--cli", help="exact zeno-fcis executable; or ZENO_FCIS_CLI")
    sub = parser.add_subparsers(dest="action", required=True)
    init = sub.add_parser("init", help="snapshot original, optional reviewed specification and checker")
    init.add_argument("--archive", required=True, type=Path)
    init.add_argument("--original", required=True, type=Path)
    init.add_argument("--spec", type=Path, help="operator-supplied specification snapshot, not authenticated")
    init.add_argument("--max-input-tuples", type=int, default=100_000)
    init.add_argument("--node-bin-width", type=int, default=2)
    init.add_argument("--byte-bin-width", type=int, default=64)
    add = sub.add_parser("offer", help="independently check, replay, and conditionally archive")
    add.add_argument("--archive", required=True, type=Path)
    add.add_argument("--candidate", required=True, type=Path)
    add.add_argument("--source", default="external-proposer")
    add.add_argument("--timeout", type=int, default=DEFAULT_TIMEOUT)
    show = sub.add_parser("show", help="show recorded elites (not a fresh verification)")
    show.add_argument("--archive", required=True, type=Path)
    verify = sub.add_parser("verify", help="replay every elite against pinned original and checker")
    verify.add_argument("--archive", required=True, type=Path)
    verify.add_argument("--timeout", type=int, default=DEFAULT_TIMEOUT)
    run = sub.add_parser("sweep", help="run e-graph strategies; independently check each result")
    run.add_argument("--archive", required=True, type=Path)
    run.add_argument("--strategy", action="append", default=[], help="strategy JSON path or default")
    run.add_argument("--timeout", type=int, default=DEFAULT_TIMEOUT)
    review = sub.add_parser("review", help="attach an operator-stated human quality decision")
    review.add_argument("--archive", required=True, type=Path)
    review.add_argument("--sha", required=True)
    review.add_argument("--decision", required=True, choices=("preferred", "rejected", "needs-work"))
    review.add_argument("--rationale", type=Path, required=True)
    review.add_argument("--reviewer", default="operator")
    export = sub.add_parser("export", help="replay and export a human-preferred elite, not adopt it")
    export.add_argument("--archive", required=True, type=Path)
    export.add_argument("--sha", required=True)
    export.add_argument("--out", type=Path, required=True)
    export.add_argument("--timeout", type=int, default=DEFAULT_TIMEOUT)
    args = parser.parse_args(argv)
    try:
        if args.action == "show":
            value = view(args.archive)
        elif args.action == "review":
            value = review_candidate(args.archive, args.sha, args.decision, args.rationale, args.reviewer)
        else:
            if hasattr(args, "timeout") and not 1 <= args.timeout <= 3600:
                raise ArchiveError("timeout must be 1..3600 seconds")
            cli = cli_binary(args.cli)
            if args.action == "init":
                value = init_archive(args.archive, args.original, cli, args.spec,
                                     args.max_input_tuples, args.node_bin_width, args.byte_bin_width)
            elif args.action == "offer":
                value = offer(args.archive, cli, args.candidate, args.source, args.timeout)
            elif args.action == "verify":
                value = replay_archive(args.archive, cli, args.timeout)
            elif args.action == "export":
                value = export_elite(args.archive, cli, args.sha, args.out, args.timeout)
            else:
                value = sweep(args.archive, cli, args.strategy, args.timeout)
        output(value)
        return 0 if value.get("status") not in ("rejected", "no-proposal") else 2
    except (ArchiveError, OSError, ValueError, KeyError, TypeError) as exc:
        output({"status": "refused", "authority": "none", "message": str(exc)})
        return 1


if __name__ == "__main__":
    sys.exit(main())
