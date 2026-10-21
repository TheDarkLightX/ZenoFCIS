# Random payout tie-break

## Journey

Run `zeno-fcis contract distribution examples/distribution-laws/payout-tie`.
It checks who receives one indivisible payout unit when two recipients tie.

## Formal model

Given a uniform independent one-bit context input r, independent of the fixed
inputs, set Alice's payout to 1-r and Bob's to r. The invocation law requires
their total to be one. Each declared marginal distribution is {0: 1/2, 1: 1/2}.

## Difference

The conservation law constrains each payout decision. The distribution laws
check the two exact marginals across every random value for each pre-state.
Together they describe this bounded tie-break without averaging over pre-states.

## Scope

Exactly checked over the declared finite domain, given uniform independent
random bits. These are integer payout decisions; no external value is moved.
The check does not establish joint laws beyond the separately declared marginals.
