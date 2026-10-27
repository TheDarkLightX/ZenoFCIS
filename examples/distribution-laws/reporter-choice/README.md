# Reporter choice

## Journey

Run `zeno-fcis contract distribution examples/distribution-laws/reporter-choice`.
It checks a choice of one of four reporters, for every declared pre-state.

## Formal model

The two-bit context input r ranges from 0 through 3. The successor reporter is
r. Each reporter therefore has exact mass 1/4, given uniform independent random
bits, independent of the fixed inputs. The law declares `uniform over {0, 1, 2, 3}`.

## Difference

Four reporters allow a direct choice. An arbitrary reporter count need not
divide the size of a finite bit domain; the neighboring modulo example shows
the resulting bias. The singleton boundary is also tested with zero random bits.

## Scope

Exactly checked over the declared finite domain, given uniform independent
random bits. There is no entropy-source or cross-step rejection/retry guarantee.
