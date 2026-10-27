# Reserve shortfall rewards

## Journey

Run `zeno-fcis contract distribution examples/distribution-laws/shortfall-reward`.
Exit 0 means exactly checked over the declared finite domain, given uniform
independent random bits. Two bits select one of four price-shock scenarios.

## Formal model

Scenario losses of 1, 2, 3 and 5 against reserve 2 produce shortfalls
R = max(0, loss - 2) of 0, 0, 1 and 3. Integer shortfalls satisfy
P(R > 0) = P(R >= 1) = 1/2. With excess budget b = 1,
E[max(0, R - b)] = 1/2. Both declared upper bounds are exactly met.

## Difference

The threshold constrains the frequency of a shortfall; expected excess also
constrains its size beyond the budget. See the [biased version](../biased-reward/README.md)
and the [checker reference](../../../docs/DISTRIBUTION_LAWS.md).

## Scope

The scenario set and reserve are fixed finite declarations. This is a
single-step illustration, with no financial forecast or multi-step guarantee.
