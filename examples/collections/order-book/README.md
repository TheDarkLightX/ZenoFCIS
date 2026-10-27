# A tiny order book

Start with bids at prices 10 and 20. A seller can fill the bid at 20. Filling
at 10 would give a worse price while a better bid remains, so that request
must be rejected. A level holds one unit; placing an existing price leaves
it at one unit, cancelling removes it, and filling consumes the best level.

The [project](project.zeno) declares an ordered `Price` bounded by
`0..=1000000`, a map of levels, and three commands: Place, Cancel and Fill.
It names the command's price and observes the best bid before the decision:

```text
order 180 best_bid of 110 max into context.102.132 found context.102.133;
entry 160 level of 110 at command.101.131;
entry 170 best of 110 at context.102.132;
```

The result key and found flag are immutable context observations of the
same pre-state map. The [policy](v2/policy.json) rejects a fill when the map
is empty, the named level is absent, or its price differs from the best.
Law 500 requires every committed fill to use a present level whose price
is at least the best bid. The maximum read makes any strictly better
present level impossible. Law 501 keeps absent levels at zero quantity.

Check the synthesized policy:

```text
zeno-fcis contract collections examples/collections/order-book
zeno-fcis check examples/collections/order-book/project.zeno --expand
```

The first command checks all admissible weak-order views, including equal
prices, an absent candidate, an empty map and either ordering of distinct
prices. Removing the best-price refusal is refuted: place at 20, place at
10, then fill at 10. The bare expanded scalar project printed by the second
command does not supply map order facts by itself.

The example exercises the checker and the pure ordered-key query helper.
The existing storage `Layout` refuses ordered projects: framing these
observations, wire encoding, generated applications, matching quantities,
multiple orders at a price, fees and persistence are outside this example.
There is no whole-map iteration in the decision API.

## Lineage

**Background.** The view follows data independence: P. Wolper, *Expressing
Interesting Properties of Programs in Propositional Temporal Logic*, POPL,
1986. Ordered-data candidates are R. Lazić and D. Nowak, CONCUR 2000 (title
and theorem scope await verification), and R. Lazić, *A Semantic Study of
Data Independence with Applications to Model Checking*, doctoral thesis,
1999 (bibliographic details await verification). The
[ordered reduction](../../../docs/COLLECTIONS.md#why-the-ordered-check-covers-every-size)
states the additional premises and explicit absence facts.

**What is new here.** Prior-art search pending; no novelty claim is made.

**Scope.** A tiny synthesized policy, exhaustively checked over its declared
view domains. This example does not qualify an order-book service or an
ordered storage adapter.
