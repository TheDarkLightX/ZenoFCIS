"""Lossless scalar reports: safe JSON number or canonical decimal string.

Decode at scalar/tuple use sites; never reinterpret expression text, variant
names, program documents or receipt strings with an untyped recursive decoder.
Encoding a display makes a fresh tree and grants no decision authority.
"""
from behavior import Refusal

ENCODING = 'safe-number-or-decimal-string/1'
SAFE = (1 << 53) - 1
I128 = (-(1 << 127), (1 << 127) - 1)


def integer(value):
    if type(value) is int:
        number = value
    elif type(value) is str and len(value) <= 40:
        try:
            number = int(value)
        except ValueError:
            raise Refusal('report-integer') from None
        if str(number) != value:
            raise Refusal('report-integer')
    else:
        raise Refusal('report-integer')
    if not I128[0] <= number <= I128[1]:
        raise Refusal('report-integer-range')
    return number


def integers(values):
    if type(values) not in (list, tuple):
        raise Refusal('report-integer-tuple')
    return tuple(integer(value) for value in values)


def encode(value):
    """Browser-safe JSON display tree; retain bools, text and small numbers."""
    if type(value) is int:
        return value if -SAFE <= value <= SAFE else str(value)
    if type(value) is dict:
        return {k: encode(v) for k, v in value.items()}
    if type(value) in (list, tuple):
        return [encode(v) for v in value]
    return value
