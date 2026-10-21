# Modulo bias

## Journey

Run `zeno-fcis contract distribution examples/distribution-laws/modulo-bias`.
Exit 1 reports a mismatch for the declared uniform distribution on three reporters.

## Formal model

The two-bit input r ranges over 0, 1, 2, 3. The decision computes r mod 3 as
`r - 3 * div_floor(r, 3)`. Its distribution is {0: 1/2, 1: 1/4, 2: 1/4},
given uniform independent random bits, independent of the fixed inputs.
The target is {0: 1/3, 1: 1/3, 2: 1/3}.

## Difference

The minimal excess witness is r in {0, 3}: both select reporter 0, giving mass
2/4 greater than 1/3. One assignment has mass 1/4 and cannot establish excess.
All four random values are counted before the mismatch is reported.

## Scope

This finite refutation depends on uniform independent random bits. It checks
this decision and target; it neither verifies entropy acquisition nor supplies
an unbounded rejection/retry algorithm.
