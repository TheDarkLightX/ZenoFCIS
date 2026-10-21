# Distribution law examples

## Journey

Run `zeno-fcis contract distribution examples/distribution-laws/reporter-choice`.
The result is exactly checked over the declared finite domain, given uniform
independent random bits. [Reporter choice](reporter-choice/README.md) selects
one of four reporters. [Modulo bias](modulo-bias/README.md) is refused with a
counterexample. [Payout tie-break](payout-tie/README.md) splits one payout unit.

## Formal model

For each fixed non-random tuple, fold every random assignment with exact weight
1/N. Compare the output histogram with a declared rational target. See the
[format and checker reference](../../docs/DISTRIBUTION_LAWS.md).

## Difference

These checks compare distributions across inputs, alongside the ordinary laws
that constrain each individual decision. They do not acquire randomness.

## Scope

The results depend on uniform independent random bits, independent of the fixed
inputs. Examples are finite declarative contracts with advisory check packets;
they do not perform payments or implement cross-step retry protocols.
