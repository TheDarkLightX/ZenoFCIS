"""Value packets, checked proposal sessions, bounded search and receipt replay.

An LLM or any other proposer supplies data. Only verify.check decides semantic
equivalence. These development receipts grant no runtime or release authority.
"""

from __future__ import annotations

from contextlib import contextmanager
import hashlib
from itertools import islice, product
import json
import os
from pathlib import Path
import platform
import re
import sys
import time

from .model import MASK, canonical, digest, evaluate, integer, read_json, validate_problem
from .targets import assembly_source, elf_object, encode_instruction, encode_program

MAX_EVENTS = 256
MAX_SESSION_BYTES = 8 << 20
MAX_LEDGER_BYTES = 2 << 20
ASSURANCE = "SMT check under the registered ISA model, not a kernel proof or deployment grant"


def _cost(cost):
    if not isinstance(cost, dict) or set(cost) != {"bytes", "instructions"}:
        raise ValueError("invalid receipt or artifact cost")
    integer(cost["bytes"], 1, 256, "receipt bytes")
    integer(cost["instructions"], 0, 16, "receipt instructions")


def _receipt(receipt):
    fields = {"schema", "authority", "problem_sha256", "code_sha256", "identity", "verification", "assurance"}
    if (not isinstance(receipt, dict) or set(receipt) != fields
            or receipt["schema"] != "zeno-fcis/machine-receipt/1" or receipt["authority"] != "none"
            or receipt["assurance"] != ASSURANCE or not isinstance(receipt["verification"], dict)
            or receipt["verification"].get("status") not in ("equivalent", "counterexample", "inconclusive", "refused")):
        raise ValueError("unregistered receipt schema, authority or assurance")
    cost = receipt["verification"].get("cost")
    if cost is not None:
        _cost(cost)


def file_hash(path: Path, limit=None) -> str:
    with path.open("rb") as stream:
        if limit is not None:
            data = stream.read(limit + 1)
            if len(data) > limit:
                raise ValueError("artifact grew beyond its read limit")
            return digest(data)
        return hashlib.file_digest(stream, "sha256").hexdigest()


def identity() -> dict:
    """Pin checker/proposer sources and installed solver inputs, not just banners."""
    import z3
    here = Path(__file__).resolve().parent
    sources = {p.name: file_hash(p) for p in sorted(here.glob("*.py"))}
    package = Path(z3.__file__).resolve().parent
    solver = {p.name: file_hash(p) for p in sorted(package.glob("*.py"))}
    libraries = [p for p in sorted((package / "lib").iterdir())
                 if p.is_file() and any(s in p.name for s in (".so", ".dylib", ".dll"))]
    if not libraries:
        raise ValueError("cannot bind installed Z3 native library inputs")
    solver.update({"lib/" + p.name: file_hash(p) for p in libraries})
    return {"schema": "zeno-fcis/machine-checker-identity/1", "sources": sources,
            "python": platform.python_version(), "python_sha256": file_hash(Path(sys.executable)),
            "z3_version": z3.get_full_version(), "z3_inputs": solver,
            "loader_premise": "trusted Python/package loader and host; hashes are not authenticated origin"}


def _write(path: Path, data: bytes):
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags, 0o600)
    with os.fdopen(descriptor, "wb") as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())


def _json(path: Path, value: object):
    _write(path, canonical(value) + b"\n")


def _code(path: Path) -> bytes:
    if path.is_symlink() or not path.is_file() or not 1 <= path.stat().st_size <= 256:
        raise ValueError("code must be a regular non-symlink file of1..256 bytes")
    with path.open("rb") as stream:
        result = stream.read(257)
    if not 1 <= len(result) <= 256:
        raise ValueError("code grew beyond its byte limit")
    return result


def _grammar(problem: dict):
    encoded = [encode_instruction(problem["target"], item) for item in problem["grammar"]]
    if len(set(encoded)) != len(encoded):
        raise ValueError("grammar has duplicate instruction encodings")
    return encoded


def open_session(problem_path: Path, directory: Path) -> dict:
    problem = validate_problem(read_json(problem_path))
    _grammar(problem)
    binding = identity()
    directory.mkdir(mode=0o700)
    _json(directory / "problem.json", problem)
    _json(directory / "session.json", {"schema": "zeno-fcis/machine-session/1",
          "problem_sha256": digest(canonical(problem)), "identity": binding,
          "authority": "none", "storage_premise": "trusted local session storage; no anti-rollback claim"})
    _write(directory / "events.jsonl", b"")
    return {"status": "opened", "target": problem["target"], "problem_sha256": digest(canonical(problem))}


@contextmanager
def _locked(directory: Path):
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError("session must be an existing non-symlink directory")
    lock = directory / "lock"
    _write(lock, str(os.getpid()).encode("ascii"))
    try:
        yield
    finally:
        lock.unlink()


def _load(directory: Path):
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError("session must be an existing non-symlink directory")
    files = list(islice(directory.iterdir(), 3 * MAX_EVENTS + 5))
    if len(files) > 3 * MAX_EVENTS + 4:
        raise ValueError("session exceeds its file limit")
    if any(p.is_symlink() or not p.is_file() for p in files):
        raise ValueError("session contains a non-regular entry")
    if sum(p.stat().st_size for p in files) > MAX_SESSION_BYTES:
        raise ValueError("session exceeds its byte limit")
    problem = validate_problem(read_json(directory / "problem.json"))
    _grammar(problem)
    manifest = read_json(directory / "session.json")
    if (manifest.get("schema") != "zeno-fcis/machine-session/1"
            or manifest.get("problem_sha256") != digest(canonical(problem))
            or manifest.get("identity") != identity() or manifest.get("authority") != "none"):
        raise ValueError("session problem/checker/solver binding drifted")
    ledger = directory / "events.jsonl"
    if ledger.stat().st_size > MAX_LEDGER_BYTES:
        raise ValueError("session ledger exceeds its byte limit")
    with ledger.open("rb") as stream:
        raw = stream.read(MAX_LEDGER_BYTES + 1)
    if len(raw) > MAX_LEDGER_BYTES:
        raise ValueError("session ledger grew beyond its byte limit")
    if raw and not raw.endswith(b"\n"):
        raise ValueError("incomplete session event")
    events = []; previous = "0" * 64
    expected_files = {"problem.json", "session.json", "events.jsonl", "lock"}
    for sequence, line in enumerate(raw.splitlines(), 1):
        if len(line) > 65_536 or sequence > MAX_EVENTS:
            raise ValueError("session event count/size exceeds its limit")
        event = json.loads(line)
        if (set(event) != {"schema", "sequence", "previous", "proposal_sha256", "code_sha256", "receipt_sha256", "status", "hash"}
                or event["schema"] != "zeno-fcis/machine-event/1" or type(event["sequence"]) is not int
                or canonical(event) != line):
            raise ValueError("session event is not canonical JSON")
        hashed = {key: value for key, value in event.items() if key != "hash"}
        if (event.get("sequence") != sequence or event.get("previous") != previous
                or event.get("hash") != digest(canonical(hashed))):
            raise ValueError("session event chain changed")
        prefix = f"{sequence:04d}"
        paths = {"proposal": directory / (prefix + ".proposal.json"),
                 "code": directory / (prefix + ".bin"),
                 "receipt": directory / (prefix + ".receipt.json")}
        for kind, path in paths.items():
            if event.get(kind + "_sha256") != file_hash(path, 256 if kind == "code" else 65_536):
                raise ValueError("session artifact identity changed")
            expected_files.add(path.name)
        proposal = read_json(paths["proposal"])
        if bytes.fromhex(proposal["code_hex"]) != _code(paths["code"]):
            raise ValueError("proposal and machine bytes disagree")
        receipt = read_json(paths["receipt"])
        _receipt(receipt)
        if (receipt.get("problem_sha256") != manifest["problem_sha256"]
                or receipt.get("code_sha256") != event["code_sha256"]
                or receipt.get("identity") != manifest["identity"]
                or receipt.get("verification", {}).get("status") != event.get("status")):
            raise ValueError("receipt does not bind the event inputs")
        events.append(event); previous = event["hash"]
    if any(p.name not in expected_files for p in files):
        raise ValueError("session contains an uncommitted artifact; inspect the interrupted operation")
    return problem, manifest, events


def _rank(problem, verification, code):
    first, second = ("bytes", "instructions") if problem["objective"] == "bytes" else ("instructions", "bytes")
    return verification["cost"][first], verification["cost"][second], code.hex()


def _best(directory, problem, events):
    accepted = []
    for event in events:
        if event["status"] == "equivalent":
            prefix = f"{event['sequence']:04d}"
            receipt = read_json(directory / (prefix + ".receipt.json"))
            code = _code(directory / (prefix + ".bin"))
            accepted.append((_rank(problem, receipt["verification"], code), event, receipt, code))
    return min(accepted, key=lambda row: row[0]) if accepted else None


def _proposal(value):
    if not isinstance(value, dict) or set(value) != {"schema", "code_hex", "provenance"}:
        raise ValueError("proposal fields differ from the closed version1 schema")
    if value["schema"] != "zeno-fcis/machine-proposal/1":
        raise ValueError("unsupported proposal schema")
    code_hex = value["code_hex"]
    if not isinstance(code_hex, str) or not re.fullmatch(r"(?:[0-9a-f]{2}){1,256}", code_hex):
        raise ValueError("code_hex must contain1..256 lowercase canonical byte pairs")
    origin = value["provenance"]
    if (not isinstance(origin, dict) or set(origin) != {"kind", "name", "note"}
            or origin["kind"] not in ("llm", "human", "local-search")
            or not isinstance(origin["name"], str) or not 1 <= len(origin["name"]) <= 128
            or not isinstance(origin["note"], str) or len(origin["note"]) > 1024):
        raise ValueError("proposal provenance must be bounded descriptive data")
    return bytes.fromhex(code_hex)


def _record(directory, problem, manifest, events, proposal, timeout_ms):
    from .verify import check
    if len(events) >= MAX_EVENTS:
        raise ValueError("session proposal budget exhausted")
    code = _proposal(proposal)
    verification = check(problem, code, timeout_ms=timeout_ms)
    receipt = {"schema": "zeno-fcis/machine-receipt/1", "authority": "none",
               "problem_sha256": manifest["problem_sha256"], "code_sha256": digest(code),
               "identity": manifest["identity"], "verification": verification,
               "assurance": ASSURANCE}
    sequence = len(events) + 1; prefix = f"{sequence:04d}"
    proposal_bytes = canonical(proposal) + b"\n"; receipt_bytes = canonical(receipt) + b"\n"
    _write(directory / (prefix + ".proposal.json"), proposal_bytes)
    _write(directory / (prefix + ".bin"), code)
    _write(directory / (prefix + ".receipt.json"), receipt_bytes)
    event = {"schema": "zeno-fcis/machine-event/1", "sequence": sequence,
             "previous": events[-1]["hash"] if events else "0" * 64,
             "proposal_sha256": digest(proposal_bytes), "code_sha256": digest(code),
             "receipt_sha256": digest(receipt_bytes), "status": verification["status"]}
    event["hash"] = digest(canonical(event))
    line = canonical(event) + b"\n"
    if (directory / "events.jsonl").stat().st_size + len(line) > MAX_LEDGER_BYTES:
        raise ValueError("session ledger budget exhausted")
    with (directory / "events.jsonl").open("ab") as stream:
        stream.write(line); stream.flush(); os.fsync(stream.fileno())
    events.append(event)
    return {"status": verification["status"], "sequence": sequence, "verification": verification,
            "receipt": prefix + ".receipt.json", "event_sha256": event["hash"]}


def submit(directory: Path, proposal_path: Path, timeout_ms=1000) -> dict:
    integer(timeout_ms, 1, 5000, "solver timeout")
    proposal = read_json(proposal_path)
    with _locked(directory):
        problem, manifest, events = _load(directory)
        return _record(directory, problem, manifest, events, proposal, timeout_ms)


def resume(directory: Path, timeout_ms=1000) -> dict:
    from .verify import check
    integer(timeout_ms, 1, 5000, "solver timeout")
    problem, _, events = _load(directory)
    best = _best(directory, problem, events)
    if best is None:
        return {"status": "open", "events": len(events), "incumbent": None}
    _, event, receipt, code = best
    fresh = check(problem, code, timeout_ms=timeout_ms)
    if fresh["status"] != "equivalent":
        return {"status": "inconclusive", "reason": "incumbent did not recheck", "verification": fresh}
    if fresh != receipt["verification"]:
        raise ValueError("incumbent receipt disagrees with fresh byte interpretation")
    return {"status": "replayed", "events": len(events), "incumbent": event["sequence"],
            "code_hex": code.hex(), "verification": fresh, "global_optimality": False}


def packet(directory: Path) -> dict:
    problem, manifest, events = _load(directory)
    feedback = []
    for event in events[-8:]:
        receipt = read_json(directory / f"{event['sequence']:04d}.receipt.json")
        feedback.append({"sequence": event["sequence"], "verification": receipt["verification"]})
    return {"schema": "zeno-fcis/machine-proposer-packet/1", "authority": "none",
            "problem": problem, "problem_sha256": manifest["problem_sha256"],
            "identity_sha256": digest(canonical(manifest["identity"])), "feedback": feedback,
            "proposal_schema": {"schema": "zeno-fcis/machine-proposal/1", "code_hex": "actual lowercase machine bytes",
                                "provenance": {"kind": "llm", "name": "proposer name", "note": "short public description"}},
            "instruction": "Propose data only. Use declared grammar and returned counterexamples. The checker decides."}


def search(directory: Path, max_candidates=10_000, max_checks=64, deadline_ms=5000, timeout_ms=500):
    from .verify import check, run_concrete
    integer(max_candidates, 1, 100_000, "candidate budget")
    integer(max_checks, 1, 128, "checker budget")
    integer(deadline_ms, 1, 60_000, "deadline")
    integer(timeout_ms, 1, 5000, "solver timeout")
    began = time.monotonic(); proposed = 0; checks = 0; filtered = 0; pruned = 0; stopped = "grammar-exhausted"
    witnesses = [(0, 0, {}), (1, 0, {}), (0, 1, {}), (MASK, 1, {}), (MASK, MASK, {}),
                 (1 << 63, (1 << 63) - 1, {})]
    verdicts = []; current_checks = {}
    with _locked(directory):
        problem, manifest, events = _load(directory)
        incumbent = _best(directory, problem, events)
        bound = None
        if incumbent:
            _, event, receipt, code = incumbent
            remaining = deadline_ms / 1000 - (time.monotonic() - began)
            if remaining < .001:
                return {"status": "inconclusive", "reason": "budget prevents mandatory stored-incumbent recheck",
                        "stop_reason": "budget-exhausted", "proposed": 0, "solver_checks": 0, "authority": "none"}
            fresh = check(problem, code, timeout_ms=max(1, min(timeout_ms, int(remaining * 1000))))
            checks += 1
            if fresh["status"] != "equivalent":
                return {"status": "inconclusive", "reason": "stored incumbent did not recheck",
                        "verification": fresh, "proposed": 0, "solver_checks": checks, "authority": "none"}
            if fresh != receipt["verification"]:
                raise ValueError("incumbent receipt disagrees with fresh byte interpretation")
            current_checks[code] = fresh
            bound = _rank(problem, fresh, code)
        grammar = _grammar(problem)
        terminal = encode_program(problem["target"], [])
        for length in range(problem["max_instructions"] + 1):
            for choices in product(range(len(grammar)), repeat=length):
                remaining = deadline_ms / 1000 - (time.monotonic() - began)
                if remaining <= 0 or proposed >= max_candidates or checks >= max_checks or len(events) >= MAX_EVENTS:
                    stopped = "budget-exhausted"; break
                proposed += 1
                code = b"".join(grammar[index] for index in choices) + terminal
                estimated = {"cost": {"bytes": len(code), "instructions": length}}
                if bound is not None and _rank(problem, estimated, code) >= bound:
                    pruned += 1
                    continue
                if any(run_concrete(problem["target"], code, {"x": x, "y": y}, initial) != evaluate(problem["expression"], x, y)
                       for x, y, initial in witnesses):
                    filtered += 1; continue
                proposal = {"schema": "zeno-fcis/machine-proposal/1", "code_hex": code.hex(),
                            "provenance": {"kind": "local-search", "name": "bounded-enumerator/1",
                                           "note": "declared instruction-product enumeration with concrete counterexample filtering"}}
                result = _record(directory, problem, manifest, events, proposal,
                                 max(1, min(timeout_ms, int(remaining * 1000))))
                checks += 1
                if result["status"] == "equivalent":
                    current_checks[code] = result["verification"]
                    rank = _rank(problem, result["verification"], code)
                    bound = rank if bound is None else min(bound, rank)
                verdicts.append({"sequence": result["sequence"], "status": result["status"]})
                verification = result["verification"]
                if result["status"] == "counterexample":
                    inputs = verification["inputs"]
                    witnesses.append((int(inputs["x"]), int(inputs["y"]),
                                      {key: int(value) for key, value in verification["initial_registers"].items()}))
                    witnesses = witnesses[:6] + witnesses[-58:] if len(witnesses) > 64 else witnesses
            if stopped != "grammar-exhausted":
                break
        best = _best(directory, problem, events)
        if best:
            _, event, receipt, code = best
            fresh = current_checks.get(code)
            if fresh is None:
                remaining = deadline_ms / 1000 - (time.monotonic() - began)
                if checks >= max_checks or remaining < .001:
                    return {"status": "inconclusive", "reason": "budget prevents mandatory stored-incumbent recheck",
                            "stop_reason": stopped, "proposed": proposed, "solver_checks": checks, "authority": "none"}
                fresh = check(problem, code, timeout_ms=max(1, min(timeout_ms, int(remaining * 1000))))
                checks += 1
            if fresh["status"] != "equivalent":
                return {"status": "inconclusive", "reason": "stored incumbent did not recheck",
                        "verification": fresh, "proposed": proposed, "solver_checks": checks, "authority": "none"}
            if fresh != receipt["verification"]:
                raise ValueError("incumbent receipt disagrees with fresh byte interpretation")
        return {"status": "checked-candidate" if best else "no-checked-candidate", "stop_reason": stopped,
                "proposed": proposed, "concrete_refutations": filtered, "objective_pruned": pruned, "solver_checks": checks,
                "checker_verdicts": verdicts, "retained_counterexamples": len(witnesses) - 6,
                "incumbent": best[1]["sequence"] if best else None,
                "cost": best[2]["verification"]["cost"] if best else None,
                "elapsed_seconds": time.monotonic() - began, "global_optimality": False,
                "runtime_speedup": "unmeasured", "authority": "none"}


def emit(directory: Path, output: Path, timeout_ms=1000):
    with _locked(directory):
        problem, manifest, events = _load(directory)
        replayed = resume(directory, timeout_ms)
        if replayed["status"] != "replayed":
            return {"status": "inconclusive", "reason": "no freshly rechecked incumbent", "detail": replayed}
        _, event, receipt, code = _best(directory, problem, events)
        obj = elf_object(problem["target"], code)
        output.mkdir(mode=0o700)
        _write(output / "kernel.bin", code)
        _write(output / "kernel.o", obj)
        _write(output / "kernel.S", assembly_source(code))
        _json(output / "problem.json", problem)
        _json(output / "receipt.json", receipt)
        files = {p.name: {"sha256": file_hash(p, 65_536), "bytes": p.stat().st_size} for p in sorted(output.iterdir())}
        result = {"schema": "zeno-fcis/machine-artifact/1", "status": "emitted", "target": problem["target"],
                  "symbol": "zeno_kernel", "files": files, "event_sha256": event["hash"],
                  "problem_sha256": manifest["problem_sha256"], "cost": replayed["verification"]["cost"],
                  "assurance": ASSURANCE, "native_execution": "not established by emission",
                  "global_optimality": False, "runtime_speedup": "unmeasured", "authority": "none"}
        _json(output / "manifest.json", result)
        return result


def replay_artifact(directory: Path, timeout_ms=1000):
    from .verify import check
    integer(timeout_ms, 1, 5000, "solver timeout")
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError("artifact must be a non-symlink directory")
    manifest = read_json(directory / "manifest.json")
    fields = {"schema", "status", "target", "symbol", "files", "event_sha256", "problem_sha256", "cost",
              "assurance", "native_execution", "global_optimality", "runtime_speedup", "authority"}
    if (set(manifest) != fields or manifest.get("schema") != "zeno-fcis/machine-artifact/1"
            or manifest.get("authority") != "none" or manifest.get("status") != "emitted"
            or manifest.get("symbol") != "zeno_kernel" or manifest.get("assurance") != ASSURANCE
            or manifest.get("native_execution") != "not established by emission"
            or manifest.get("global_optimality") is not False or manifest.get("runtime_speedup") != "unmeasured"):
        raise ValueError("unsupported artifact manifest")
    _cost(manifest["cost"])
    if not isinstance(manifest["event_sha256"], str) or not re.fullmatch(r"[0-9a-f]{64}", manifest["event_sha256"]):
        raise ValueError("invalid artifact event reference")
    expected = {"kernel.bin", "kernel.o", "kernel.S", "problem.json", "receipt.json"}
    entries = list(islice(directory.iterdir(), 7))
    if set(manifest.get("files", {})) != expected or {p.name for p in entries} != expected | {"manifest.json"}:
        raise ValueError("artifact file inventory changed")
    for name in expected:
        path = directory / name
        if path.is_symlink() or not path.is_file() or path.stat().st_size > 65_536:
            raise ValueError("invalid artifact file")
        row = manifest["files"][name]
        if not isinstance(row, dict) or set(row) != {"sha256", "bytes"}:
            raise ValueError("invalid artifact file descriptor")
        integer(row["bytes"], 1, 65_536, "artifact file bytes")
        if row != {"sha256": file_hash(path, 65_536), "bytes": path.stat().st_size}:
            raise ValueError("artifact bytes changed")
    problem = validate_problem(read_json(directory / "problem.json"))
    code = _code(directory / "kernel.bin")
    receipt = read_json(directory / "receipt.json")
    _receipt(receipt)
    if (receipt.get("schema") != "zeno-fcis/machine-receipt/1" or receipt.get("authority") != "none"
            or receipt.get("problem_sha256") != digest(canonical(problem))
            or receipt.get("code_sha256") != digest(code) or receipt.get("identity") != identity()
            or receipt.get("verification", {}).get("status") != "equivalent"):
        raise ValueError("artifact receipt binding changed")
    with (directory / "kernel.o").open("rb") as stream:
        object_bytes = stream.read(65_537)
    if (manifest.get("target") != problem["target"] or manifest.get("problem_sha256") != receipt["problem_sha256"]
            or object_bytes != elf_object(problem["target"], code)):
        raise ValueError("artifact target/object correspondence changed")
    with (directory / "kernel.S").open("rb") as stream:
        assembly_bytes = stream.read(65_537)
    if assembly_bytes != assembly_source(code):
        raise ValueError("artifact assembly source correspondence changed")
    fresh = check(problem, code, timeout_ms=timeout_ms)
    if fresh["status"] != "equivalent":
        return {"status": "inconclusive", "reason": "artifact did not recheck", "verification": fresh}
    if fresh != receipt["verification"] or fresh["cost"] != manifest["cost"]:
        raise ValueError("artifact cost binding changed")
    return {"status": "replayed", "target": problem["target"], "code_sha256": digest(code),
            "verification": fresh, "authority": "none"}
