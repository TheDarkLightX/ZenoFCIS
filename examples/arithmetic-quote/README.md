# Ceiling quote contract

A generic exact-output quote computes the least whole input sufficient for a
requested output at a declared integer conversion divisor: `ceil(output / d)`.
For output 5 and divisor 2 the required input is 3. Laws independently check
rounding and sufficiency using exact product comparison. This is a reduced
conversion example, with no liquidity-pool pricing or deployed safety claim.

Generation, finite Authority enumeration, generated-app replay and final-source
proof qualification are pending on this development source. Use the same
commands as the [fee example](../arithmetic-fee/README.md), with this directory.
