#!/usr/bin/env python3
"""Run local Tau validity checks and corroborate their exact paired SMT queries.

No Tau or SMT answer supplies library authority or a kernel-checked proof.
"""
from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import math
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time

SCHEMA = "zeno-fcis/tau-kit/1"
MAX_BUNDLE = 64 << 20
MAX_OUTPUT = 1 << 20
MAX_QUERY = 1 << 20
# `valid ` plus the formula and NUL stay below Linux's 128 KiB argv limit.
MAX_TAU_QUERY = (128 << 10) - 8
MAX_QUERIES = 20000
MAX_SMT_DEPTH = 1024
MAX_SMT_COMMANDS = 32768
ANSI_COLOR = re.compile(r"\x1b\[[0-9;]*m")
TAU_VERDICT = re.compile(r"(?:%[0-9]+\s*:\s*|:\s*)?(T|F)")
SMT_VERDICT = re.compile(r"(sat|unsat|unknown)")
SMT_TOKEN = re.compile(r"[ \t\r\n]+|;[^\r\n]*|[()]|[A-Za-z0-9_:+*/=<>!?~^&%@.$-]+")
SMT_NAME = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")


class Refused(ValueError):
    pass


def unique_object(entries):
    result = {}
    for key, value in entries:
        if key in result:
            raise Refused(f"duplicate JSON key {key}")
        result[key] = value
    return result


def digest(path):
    hasher = hashlib.sha256()
    with Path(path).open("rb") as source:
        if not os.stat(source.fileno()).st_size <= 512 << 20:
            raise Refused("executable exceeds the identity-read cap")
        for chunk in iter(lambda: source.read(1 << 20), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def validate_smt(source):
    """Parse only the bounded top-level command profile emitted by this kit.

    This is a protocol guard, not an SMT evaluator or a semantics checker.
    Comments are lexical comments; quoted strings/symbols and other commands
    are outside the emitted profile, including output-channel changes.
    """
    if not isinstance(source, str) or not 1 <= len(source.encode()) <= MAX_QUERY:
        raise Refused("SMT counterpart exceeds the bounded text profile")
    if (not source.isascii() or any((ord(character) < 32 and character not in "\t\r\n")
                                  or ord(character) == 127 for character in source)):
        raise Refused("SMT text must use the emitted ASCII/control profile")
    commands, stack, position = [], [], 0
    while position < len(source):
        match = SMT_TOKEN.match(source, position)
        if match is None:
            raise Refused("SMT text is outside the emitted token profile")
        token, position = match.group(), match.end()
        if token[0].isspace() or token.startswith(";"):
            continue
        if token == "(":
            if len(stack) >= MAX_SMT_DEPTH:
                raise Refused("SMT nesting exceeds its bounded profile")
            stack.append([])
        elif token == ")":
            if not stack:
                raise Refused("unmatched SMT closing parenthesis")
            expression = stack.pop()
            if stack:
                stack[-1].append(expression)
            else:
                commands.append(expression)
                if len(commands) > MAX_SMT_COMMANDS:
                    raise Refused("SMT command count exceeds its bounded profile")
        elif stack:
            stack[-1].append(token)
        else:
            raise Refused("SMT atoms must be inside a top-level command")
    if stack:
        raise Refused("unterminated SMT command")
    if not commands or commands[0] not in (["set-logic", "ALL"], ["set-logic", "QF_BV"]):
        raise Refused("SMT must start with the kit's explicit logic")
    if commands[-1] != ["check-sat"]:
        raise Refused("SMT must end with one parsed check-sat")

    def named(value):
        return isinstance(value, str) and SMT_NAME.fullmatch(value) is not None

    def scalar_sort(value):
        if value in ("Int", "Bool"):
            return True
        return (isinstance(value, list) and len(value) == 3 and value[:2] == ["_", "BitVec"]
                and isinstance(value[2], str) and re.fullmatch(r"[1-9][0-9]?", value[2]) is not None
                and int(value[2]) <= 64)

    for index, command in enumerate(commands[1:-1], 1):
        if not command or not isinstance(command[0], str):
            raise Refused("invalid SMT top-level command")
        if command == ["set-option", ":produce-models", "true"] and index == 1:
            continue
        if (command[0] == "declare-const" and len(command) == 3
                and named(command[1]) and scalar_sort(command[2])):
            continue
        if (command[0] == "define-fun" and len(command) == 5 and named(command[1])
                and command[2] == [] and scalar_sort(command[3])):
            continue
        if command[0] == "assert" and len(command) == 2:
            continue
        # In particular, an earlier check-sat, echo, exit, state-changing
        # solver commands and output redirection cannot reach a tool.
        raise Refused("SMT command is outside the emitted query profile")


def validated(bundle):
    if not isinstance(bundle, dict) or bundle.get("schema") != SCHEMA or bundle.get("authority") != "none":
        raise Refused("not an advisory Tau kit")
    queries = bundle.get("queries")
    if not isinstance(queries, list) or not 1 <= len(queries) <= MAX_QUERIES:
        raise Refused("query count outside 1..20000")
    seen, controls = set(), 0
    for query in queries:
        if not isinstance(query, dict) or set(query) != {"id", "kind", "expected", "tau", "smt"}:
            raise Refused("a query must contain only id, kind, expected, tau and smt")
        identifier, formula, smt = query["id"], query["tau"], query["smt"]
        if not isinstance(identifier, str) or not re.fullmatch(r"[a-z][a-z0-9-]{0,95}", identifier) or identifier in seen:
            raise Refused("invalid or duplicate query identifier")
        seen.add(identifier)
        if query["expected"] not in ("T", "F") or not isinstance(query["kind"], str):
            raise Refused("queries require an explicit T/F expectation")
        if not isinstance(formula, str) or not 1 <= len(formula.encode()) <= MAX_TAU_QUERY:
            raise Refused(f"Tau query exceeds {MAX_TAU_QUERY} bytes; valid-command argv transport is bounded below 128 KiB")
        if formula.rstrip().endswith("."):
            raise Refused(f"trailing-period query refused: {identifier}")
        if "." in formula:
            raise Refused(f"period in query refused: {identifier}")
        if any(character in formula for character in "\r\n;#") or formula.lstrip().startswith("valid "):
            raise Refused("Tau formulas must be one command-free line")
        validate_smt(smt)
        controls += query["expected"] == "F" and query["kind"] in ("planted-control", "import-control")
    if controls < 1:
        raise Refused("the kit lacks a planted false control")
    return queries


def classify(expected, tau, smt):
    if tau not in ("T", "F") or smt not in ("sat", "unsat"):
        return "incomplete"
    if (tau == "T") != (smt == "unsat"):
        return "disagreement"
    if tau != expected:
        return "control-failed" if expected == "F" else "refuted"
    return "corroborated" if expected == "T" else "control-rejected"


def answer(kind, output, returncode, failure):
    if failure or returncode != 0 or not isinstance(output, str) or len(output.encode()) > MAX_OUTPUT:
        return None
    text = ANSI_COLOR.sub("", output)
    if (not text.isascii() or any((ord(character) < 32 and character not in "\t\r\n")
                                or ord(character) == 127 for character in text)):
        return None
    lines = [line.strip() for line in text.splitlines() if line.strip()]
    if len(lines) != 1:
        return None
    pattern = {"tau": TAU_VERDICT, "smt": SMT_VERDICT}.get(kind)
    match = pattern.fullmatch(lines[0]) if pattern is not None else None
    return match.group(1) if match is not None else None


CHILD = """import os, resource, sys
memory, cpu, output = map(int, sys.argv[1:4])
for key, target in ((resource.RLIMIT_AS, memory), (resource.RLIMIT_CPU, cpu),
                    (resource.RLIMIT_FSIZE, output), (resource.RLIMIT_CORE, 0)):
    inherited = resource.getrlimit(key)
    expected = tuple(target if value == resource.RLIM_INFINITY else min(value, target)
                     for value in inherited)
    resource.setrlimit(key, expected)
    if resource.getrlimit(key) != expected:
        raise RuntimeError("rlimit-readback")
os.execv(sys.argv[4], sys.argv[4:])
"""


def execute(command, stdin, cwd, output_file, timeout, memory_mb):
    start = time.monotonic()
    failure, returncode = None, None
    with output_file.open("xb") as output:
        try:
            # The short resource wrapper execs the tool in the same process.
            # Avoid preexec_fn in the multithreaded coordinator.
            wrapped = [sys.executable, "-I", "-B", "-c", CHILD, str(memory_mb << 20),
                       str(math.ceil(timeout) + 1), str(MAX_OUTPUT), *command]
            process = subprocess.Popen(wrapped, stdin=subprocess.PIPE, stdout=output, stderr=subprocess.STDOUT,
                                       cwd=cwd, start_new_session=True)
            try:
                process.communicate(stdin.encode(), timeout=timeout)
            except subprocess.TimeoutExpired:
                failure = "timeout"
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
            returncode = process.returncode
        except OSError as error:
            failure = f"execution-error:{error.errno}"
    raw = output_file.read_bytes()
    if len(raw) >= MAX_OUTPUT:
        failure = "output-cap"
    text = raw.decode("utf-8", errors="replace")
    return {"returncode": returncode, "failure": failure, "seconds": round(time.monotonic() - start, 6),
            "output_sha256": hashlib.sha256(raw).hexdigest(), "output": text}


def run_one(query, tau, solver, kind, directory, timeout, memory_mb):
    query_dir = directory / query["id"]
    query_dir.mkdir()
    tau_result = execute([str(tau), "--charvar", "false", "--severity", "error", "--status", "false", "--color", "false",
                          "--benchmarks", "false",
                          "--evaluate", "valid " + query["tau"]], "", query_dir, query_dir / "tau.txt", timeout, memory_mb)
    tau_answer = answer("tau", tau_result["output"], tau_result["returncode"], tau_result["failure"])
    command = [str(solver), "-in", "-smt2"] if kind == "z3" else [str(solver), "--lang=smt2"]
    # Run the independent counterpart for every query, including F controls.
    # Thus a false T, unexpected F, or unknown answer is never hidden.
    smt_result = execute(command, query["smt"], query_dir, query_dir / "smt.txt", timeout, memory_mb)
    smt_answer = answer("smt", smt_result["output"], smt_result["returncode"], smt_result["failure"])
    for result in (tau_result, smt_result):
        del result["output"]
    return {"id": query["id"], "kind": query["kind"], "expected": query["expected"],
            "status": classify(query["expected"], tau_answer, smt_answer),
            "tau": {**tau_result, "answer": tau_answer}, "smt": {**smt_result, "answer": smt_answer},
            "tau_input_sha256": hashlib.sha256(query["tau"].encode()).hexdigest(),
            "smt_input_sha256": hashlib.sha256(query["smt"].encode()).hexdigest(),
            "pair_sha256": hashlib.sha256((query["tau"] + "\0" + query["smt"]).encode()).hexdigest()}


def safely_run_one(query, tau, solver, kind, directory, timeout, memory_mb):
    try:
        return run_one(query, tau, solver, kind, directory, timeout, memory_mb)
    except OSError as error:
        return {"id": query["id"], "kind": query["kind"], "expected": query["expected"], "status": "incomplete",
                "failure": f"retention-error:{error.errno}",
                "tau_input_sha256": hashlib.sha256(query["tau"].encode()).hexdigest(),
                "smt_input_sha256": hashlib.sha256(query["smt"].encode()).hexdigest()}


def false_t_diagnostic():
    # A nonvalid alias/order proposition. A concrete counterexample is
    # a=0,b=5,c=10,d=0,e=5,f=11. This diagnostic must remain visible whether
    # the installed Tau returns the expected F or disagrees with SMT.
    tau = "all a:bv[8] all b:bv[8] all c:bv[8] all d:bv[8] all e:bv[8] all f:bv[8] ((a <= c && b <= c && d = a && e = b && f = c + {1}:bv[8] && c < {200}:bv[8]) -> (d <= f && e <= f && d >= e))"
    variables = "\n".join(f"(declare-const {name} (_ BitVec 8))" for name in "abcdef")
    premise = "(and (bvule a c) (bvule b c) (= d a) (= e b) (= f (bvadd c (_ bv1 8))) (bvult c (_ bv200 8)))"
    conclusion = "(and (bvule d f) (bvule e f) (bvuge d e))"
    return {"id": "alias-order-diagnostic", "kind": "planted-control", "expected": "F", "tau": tau,
            "smt": f"(set-logic QF_BV)\n{variables}\n(assert (not (=> {premise} {conclusion})))\n(check-sat)\n"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kit", type=Path)
    parser.add_argument("--tau", type=Path, required=True)
    parser.add_argument("--tau-sha256", required=True)
    parser.add_argument("--solver", type=Path, required=True)
    parser.add_argument("--solver-sha256", required=True)
    parser.add_argument("--solver-kind", choices=("z3", "cvc5"), required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--jobs", type=int, choices=range(1, 5), default=1)
    parser.add_argument("--timeout", type=float, default=60)
    parser.add_argument("--memory-mb", type=int, default=768)
    args = parser.parse_args()
    if not math.isfinite(args.timeout) or not 0 < args.timeout <= 3600 or not 128 <= args.memory_mb <= 1024:
        raise Refused("timeout must be in (0,3600]; memory must be in 128..1024 MiB per child")
    tau, solver = args.tau.resolve(strict=True), args.solver.resolve(strict=True)
    for binary, expected in ((tau, args.tau_sha256), (solver, args.solver_sha256)):
        if not re.fullmatch(r"[0-9a-f]{64}", expected) or not binary.is_file() or digest(binary) != expected:
            raise Refused("configured executable identity does not match its required SHA-256")
    with args.kit.open("rb") as source:
        raw = source.read(MAX_BUNDLE + 1)
    if len(raw) > MAX_BUNDLE:
        raise Refused("Tau kit exceeds 64 MiB")
    bundle = json.loads(raw, object_pairs_hook=unique_object)
    validated(bundle)
    bundle["queries"].append(false_t_diagnostic())
    queries = validated(bundle)
    # Validation and identity checks finish before any output directory or
    # external process is created.
    args.out.mkdir(parents=False, exist_ok=False)
    start = time.monotonic()
    versions = {}
    for label, binary in (("tau", tau), ("smt", solver)):
        result = execute([str(binary), "--version" if label == "tau" or args.solver_kind == "cvc5" else "-version"], "",
                         args.out, args.out / f"{label}-version.txt", min(args.timeout, 10), args.memory_mb)
        versions[label] = result
    directory = args.out / "queries"
    directory.mkdir()
    if any(result["failure"] or result["returncode"] != 0 for result in versions.values()):
        rows = []
        status = "incomplete"
    else:
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
            # At most jobs processes run: each task waits for Tau to exit before
            # launching its SMT counterpart. Raw output uses bounded files.
            rows = list(pool.map(lambda query: safely_run_one(query, tau, solver, args.solver_kind, directory, args.timeout, args.memory_mb), queries))
        refused = sum(count for key, count in bundle.get("review", {}).get("tally", {}).items() if key.startswith("refused:"))
        status = "corroborated" if not refused and all(row["status"] in ("corroborated", "control-rejected") for row in rows) else "incomplete"
    identity_unchanged = digest(tau) == args.tau_sha256 and digest(solver) == args.solver_sha256
    if not identity_unchanged:
        status = "incomplete"
    report = {"schema": "zeno-fcis/tau-run/1", "status": status, "authority": "none", "evidence": "attested-corroboration-only",
              "kit_sha256": hashlib.sha256(raw).hexdigest(), "runner_sha256": digest(Path(__file__)),
              "tool_identities_unchanged": identity_unchanged,
              "tools": {"tau_sha256": args.tau_sha256, "solver_sha256": args.solver_sha256, "solver_kind": args.solver_kind, "versions": versions},
              "limits": {"jobs": args.jobs, "per_process_timeout_seconds": args.timeout, "per_process_memory_mib": args.memory_mb, "output_bytes": MAX_OUTPUT},
              "seconds": round(time.monotonic() - start, 6), "queries": rows,
              "native_review": bundle.get("review"), "import_scope": bundle.get("import", {}).get("scope"),
              "disagreements": [row["id"] for row in rows if row["status"] == "disagreement"],
              "not_claimed": ["KernelChecked", "authentication", "publication authority", "query-generator correctness beyond supplied source identities"]}
    (args.out / "report.json").write_text(json.dumps(report, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": status, "queries": len(rows), "disagreements": report["disagreements"], "authority": "none"}))
    return 0 if status == "corroborated" else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (Refused, OSError, ValueError, TypeError, KeyError) as error:
        print(f"Tau kit refused: {error}", file=__import__("sys").stderr)
        raise SystemExit(1)
