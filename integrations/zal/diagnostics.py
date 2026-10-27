"""Presentation of finite-profile parser refusals; never repairs or admits source."""

import re


def describe(error, source, path):
    """Give one reason, a physical source location and an advisory fix.

    The parser owns the location. Source text is used only to show that line;
    it is never re-parsed or used to substitute a different semantic verdict.
    """
    reason = " ".join(str(error).splitlines())
    cases = [
        ("identifier-too-long", "exceeds 32 characters", "Shorten the identifier to at most 32 ASCII characters and update its references."),
        ("missing-terminator", "missing statement terminator", "End the declaration with a semicolon (symbolic) or period (English); put comments on separate lines."),
        ("duplicate-declaration", r"Statement \d+: duplicate", "Keep exactly one declaration of this kind."),
        ("missing-declaration", "Missing explicit declarations", "Add each named declaration explicitly, including context none when no observations are read."),
        ("reserved-name", "reserved word", "Rename the declaration and its references to a non-reserved lowercase identifier."),
        ("undeclared-reference", "undeclared|Unknown (?:state|event|observation|context)", "Declare the referenced state, event, observation or variable, or correct the reference to a declared name."),
        ("boolean-as-integer", "Boolean observation, not an integer", "Use the Boolean observation directly as a predicate; use a bounded input or variable for integer arithmetic."),
        ("formula-scope", "not allowed in (?:guards|invariants)", "Keep guards on pre-state and inputs, invariants on state, and put event, accepted or post-state conditions in a requirement."),
        ("invalid-range", "inclusive range|starts outside", "Use an inclusive range of 1 to 64 values within -32767 to 32767 and put the initial value inside it."),
        ("invalid-outcome", "explicit outcome|updates one variable more than once|updates need an Accept", "Use accept TARGET with at most one update per variable, or reject REASON unchanged with no updates."),
    ]
    code, fix = "controlled-grammar", "Use one complete controlled-grammar declaration per line; see docs/zal/GRAMMAR.md."
    for candidate, pattern, suggestion in cases:
        if re.search(pattern, reason):
            code, fix = candidate, suggestion
            break
    # Keep the legacy refusal text in the CLI's diagnostic field. This structured
    # reason omits that generic prefix when a more specific lexical reason exists.
    if code in {"identifier-too-long", "missing-terminator"}:
        reason = reason.rsplit("; ", 1)[-1]
    location = getattr(error, "location", None)
    line, column = location if location else (1, 1)
    lines = source.splitlines()
    return {"schema": "zal/source-diagnostic/1", "code": code, "reason": reason,
            "location": {"file": str(path), "line": line, "column": column},
            "suggested_fix": fix, "source_line": lines[line - 1] if 0 < line <= len(lines) else "",
            "location_scope": "parser" if location else "operation"}
