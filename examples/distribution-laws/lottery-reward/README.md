# Lottery payout rewards

## Journey

Run `zeno-fcis contract distribution examples/distribution-laws/lottery-reward`.
Exit 0 means exactly checked over the declared finite domain, given uniform
independent random bits. Four tickets pay 0, 1, 2 and 3 units.

## Formal model

At each fixed input tuple, two uniform independent bits select a ticket.
E[R] = 3/2 and Var[R] = 5/4. The declaration bounds both measures above and
below at these exact rational values. Each bound is exactly met.

## Difference

Reward measures constrain numeric summaries of the output histogram. They do
not require an output distribution target or run a second decision evaluator.
See the [reward syntax](../../../docs/DISTRIBUTION_LAWS.md).

## Scope

This finite, single-step example checks every ticket for every declared state.
The packet is advisory and does not authorize payments or certify an entropy source.
