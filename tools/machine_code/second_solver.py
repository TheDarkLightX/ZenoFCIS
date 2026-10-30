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

VERSION = "1.3.3"


def _version(cvc5: Path) -> str:
    banner = subprocess.run([str(cvc5), "--version"], capture_output=True, text=True, timeout=30).stdout
    first = banner.splitlines()[0] if banner else ""
    prefix = "This is cvc5 version "
    return first[len(prefix):].split()[0] if first.startswith(prefix) else ""


def confirm(problem: dict, code: bytes, cvc5: Path, timeout_ms: int = 10_000) -> dict:
    """The checker's verdict, plus cvc5's answer to the same query when the verdict is equivalent."""
    import z3
    from .session import _checker
    cvc5 = Path(cvc5)
    version = _version(cvc5)
    identity = {"name": "cvc5", "version": version,
                "sha256": hashlib.sha256(cvc5.read_bytes()).hexdigest() if cvc5.is_file() else None}
    if version != VERSION:
        return {"status": "inconclusive", "reason": f"SecondSolverVersion:{version or 'unreadable'}",
                "second_solver": identity}
    recorded = []
    original = z3.Solver

    class Recording(original):
        def __init__(self, *args, **kwargs):
            super().__init__(*args, **kwargs)
            recorded.append(self)
    z3.Solver = Recording
    try:
        verdict = _checker(problem).check(problem, code, timeout_ms=min(timeout_ms, 60_000))
    finally:
        z3.Solver = original
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
