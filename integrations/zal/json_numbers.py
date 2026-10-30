"""Exact numeric meaning for ZAL JSON readers: keep the source number's value or refuse with a named reason.

The supported meanings are:
- An integer lexeme means that integer. `-0` is refused, because readers disagree about a signed integer zero.
- A fraction or exponent lexeme means the binary64 value it rounds to. It is accepted only when that value's
  shortest decimal is exactly the source's decimal value. So `0.1`, `1e-07` and `1.50` are kept, while
  `0.30000000000000001`, `1e-400` and `1e400` are refused.
- `NaN` and `Infinity` spellings are refused.

For every accepted number, the source, the in-memory model and the canonical JSON output denote the same decimal
value. Spelling can differ: `1.50` is written back as `1.5`. A reader that needs the original spelling keeps raw
tokens instead; `zal_notation.normalizer` does. Each refusal names one of the reason codes below.
"""
from __future__ import annotations

from decimal import Decimal
import math

NONFINITE = "json-number-nonfinite"
NEGATIVE_ZERO = "json-number-negative-zero-integer"
TOO_LONG = "json-number-too-long"
INEXACT = "json-number-inexact-decimal"
REASONS = (NONFINITE, NEGATIVE_ZERO, TOO_LONG, INEXACT)
MAX_DIGITS = 4300  # Python's default integer-string conversion limit, now a named refusal.


def integer_hook(refuse, max_digits: int = MAX_DIGITS):
    """A json.loads parse_int hook; refuse(reason, lexeme) must raise."""
    def parse_int(lexeme: str) -> int:
        if lexeme == "-0":
            refuse(NEGATIVE_ZERO, lexeme)
        if len(lexeme) > max_digits:
            refuse(TOO_LONG, lexeme)
        return int(lexeme)
    return parse_int


def hooks(refuse, max_digits: int = MAX_DIGITS) -> dict:
    """json.loads keyword arguments that keep or refuse every number; refuse(reason, lexeme) must raise."""
    def parse_float(lexeme: str) -> float:
        if len(lexeme) > max_digits:
            refuse(TOO_LONG, lexeme)
        value = float(lexeme)
        if not math.isfinite(value):
            refuse(NONFINITE, lexeme)
        if Decimal(repr(value)) != Decimal(lexeme):
            refuse(INEXACT, lexeme)
        return value

    def parse_constant(lexeme: str):
        refuse(NONFINITE, lexeme)

    return {"parse_int": integer_hook(refuse, max_digits), "parse_float": parse_float,
            "parse_constant": parse_constant}


def raising(error_type, prefix: str = ""):
    """A refuse(reason, lexeme) callback that raises error_type with the reason code and a bounded lexeme."""
    def refuse(reason: str, lexeme: str):
        shown = lexeme if len(lexeme) <= 40 else lexeme[:40] + "..."
        raise error_type(f"{prefix}{reason}: {shown}")
    return refuse
