"""Optional second-solver confirmation of an "equivalent" verdict.

The profile checker builds its mismatch query in Z3 and decides. This module reruns that same check while recording
the query, then asks the pinned cvc5 (the version the formal-tools workflow pins, 1.3.3) the same SMT-LIB question.
An "equivalent" verdict is confirmed only when cvc5 also answers unsat. Any other answer, a timeout or a version
mismatch reports a disagreement, which callers treat as inconclusive. The SMT-LIB text is Z3's own rendering of the
query, so both solvers read one formula. This strengthens the solver part of the trusted base; it does not check the
instruction models or the encoding.
"""

from __future__ import annotations

import hashlib
from pathlib import Path
import subprocess
import threading

VERSION = "1.3.3"
# Recording replaces the global z3.Solver class for one checker call; one lock serializes every such call, so an
# overlapping call can neither record another call's query nor restore a stale class.
_HOOK = threading.Lock()


def recorded_check(problem: dict, code: bytes, timeout_ms: int):
    """(the checker's verdict, every Z3 solver it built), recorded under the module lock."""
    import z3
    from .session import _checker
    recorded = []
    with _HOOK:
        original = z3.Solver

        class Recording(original):
            def __init__(self, *args, **kwargs):
                super().__init__(*args, **kwargs)
                recorded.append(self)
        z3.Solver = Recording
        try:
            verdict = _checker(problem).check(problem, code, timeout_ms=timeout_ms)
        finally:
            z3.Solver = original
    return verdict, recorded


def _version(cvc5: Path) -> str:
    banner = subprocess.run([str(cvc5), "--version"], capture_output=True, text=True, timeout=30).stdout
    first = banner.splitlines()[0] if banner else ""
    prefix = "This is cvc5 version "
    return first[len(prefix):].split()[0] if first.startswith(prefix) else ""


def confirm(problem: dict, code: bytes, cvc5: Path, timeout_ms: int = 10_000) -> dict:
    """The checker's verdict, plus cvc5's answer to the same query when the verdict is equivalent."""
    cvc5 = Path(cvc5)
    version = _version(cvc5)
    identity = {"name": "cvc5", "version": version,
                "sha256": hashlib.sha256(cvc5.read_bytes()).hexdigest() if cvc5.is_file() else None}
    if version != VERSION:
        return {"status": "inconclusive", "reason": f"SecondSolverVersion:{version or 'unreadable'}",
                "second_solver": identity}
    verdict, recorded = recorded_check(problem, code, min(timeout_ms, 60_000))
    result = dict(verdict, second_solver=identity)
    if verdict.get("status") != "equivalent":
        return result
    if len(recorded) != 1:
        return dict(result, status="inconclusive", reason=f"SecondSolverQueryCount:{len(recorded)}")
    text = "(set-logic ALL)\n" + recorded[0].to_smt2()
    try:
        answer = subprocess.run([str(cvc5), "--lang=smt2", f"--tlimit={timeout_ms}"], input=text,
                                capture_output=True, text=True, timeout=timeout_ms / 1000 + 30)
        reply = answer.stdout.strip().splitlines()[-1] if answer.stdout.strip() else answer.stderr.strip()[:200]
    except subprocess.TimeoutExpired:
        reply = "timeout"
    identity["answer"] = reply
    identity["query_sha256"] = hashlib.sha256(text.encode()).hexdigest()
    if reply != "unsat":
        return dict(result, status="inconclusive", reason="SecondSolverDisagrees:" + reply)
    return result
