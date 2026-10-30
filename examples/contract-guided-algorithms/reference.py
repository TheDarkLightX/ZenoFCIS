"""Independent finite u8 reference semantics for the research corpus.

This module does not import the candidate interpreter or search procedure.
Only the exact, closed functions below may be called by an experiment contract.
"""

from __future__ import annotations


def result(function: str, x: int) -> int:
    if type(x) is not int or not 0 <= x < 256:
        raise ValueError("reference input outside u8 domain")
    if function == "lowest-set-bit-u8":
        for bit in range(8):
            if x & (1 << bit):
                return 1 << bit
        return 0
    if function == "clear-lowest-set-bit-u8":
        for bit in range(8):
            if x & (1 << bit):
                return x - (1 << bit)
        return 0
    if function == "gray-code-u8":
        # Each output bit is parity of adjacent input bits, with a zero ninth bit.
        return sum(((((x >> bit) & 1) ^ ((x >> (bit + 1)) & 1)) << bit)
                   for bit in range(8))
    if function == "rotate-left-one-u8":
        # Re-index individual bits instead of using the candidate's shift/or tree.
        return sum((((x >> ((bit - 1) % 8)) & 1) << bit) for bit in range(8))
    if function == "divide-by-eight-u8":
        return x // 8
    if function == "modulo-eight-u8":
        return x % 8
    raise ValueError("unknown reference function: " + function)
