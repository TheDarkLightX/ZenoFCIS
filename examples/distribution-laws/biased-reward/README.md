# Biased shortfall rewards

## Journey

Run `zeno-fcis contract distribution examples/distribution-laws/biased-reward`.
Exit 1 reports a refutation, given uniform independent random bits. Compare
the [passing shortfall example](../shortfall-reward/README.md).

## Formal model

The planted scenario bias produces rewards 0, 1, 3 and 3. The exact threshold
probability P(R >= 1) = 3/4 exceeds the declared bound 1/2. At the first fixed
state (shortfall = 0), the outcome R = 3 has mass 1/2, from random assignments
2 and 3. Its signed contribution to actual minus bound is 1/4, the largest.
R = 1 contributes 1/8 and R = 0 contributes -1/8; these sum to the exact gap 1/4.
The packet retains all outcomes and their preimages in that ranked order.

## Difference

A few passing tickets cannot establish this bound. The complete fiber exposes
the biased scenario mapping with an exact rational actual value and bound.

## Scope

The witness is a deterministic explanation of this finite counterexample.
It claims no minimum witness size and grants no runtime authority.
