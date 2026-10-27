# Swap-pool risk questions

This LLM-generated finite example holds reserve 0..4, last fee 0..2 and a
slippage-limit flag. A command supplies amount 0..3. Amount above reserve
commits a failure with no reserve change and zero reward; otherwise reserve
falls by amount and the fee is 1 for amount 0 or 2 for a positive amount.
Genesis is reserve 4, fee 0, flag false. There is no price curve.

From the repository root, count complete declared inputs:

```sh
zeno-fcis contract risk examples/quantitative-risk/swap-pool \
  --where 'post.100.112 == 1' --format json
```

Expect 36 of 120, or 3/10. This is a cardinality share, with no empirical swap
probability claim. Unreachable pre-states are included: reserve 4, fee 2 and
flag true is declared but unreachable.

Then count distinct states through three commands:

```sh
zeno-fcis contract risk examples/quantitative-risk/swap-pool \
  --states --steps 3 --where 'post.100.112 == 1' --format json
```

Finally find accumulated fee extremes using declared reward 610:

```sh
zeno-fcis contract risk examples/quantitative-risk/swap-pool \
  --reward 610 --steps 3 --format json
```

Expect best 0 on the empty run and worst 6, for example amounts 1, 1, 1.
Every returned run lists full input tuples: reserve, fee, flag, amount, context.
The unit context is 0. Inspect `status`, `claim`, limits and witnesses before
using an answer. Only a complete result is exhaustively checked over the
declared domains. See the [judge guide](../../../docs/QUANTITATIVE_RISK.md) for
refusal semantics, caps, exits, tests and the independent Python reference.

## Lineage

Background: Valiant, “The Complexity of Enumeration and Reliability Problems,”
SIAM Journal on Computing (1979); Mohri, “Semiring Frameworks and Algorithms
for Shortest-Distance Problems,” JALC (2002); Maclagan and Sturmfels,
*Introduction to Tropical Geometry*, AMS (2015); Kwiatkowska, Norman and Parker,
“PRISM 4.0: Verification of Probabilistic Real-Time Systems,” CAV (2011);
Dehnert, Junges, Katoen and Volk, “A Storm is Coming: A Modern Probabilistic
Model Checker,” CAV (2017). VERIFY bibliographic details.

What is new here: integration of finite counting and path aggregation;
prior-art search pending. Scope: this finite reserve/fee model and bounded
command horizon. No live pool, market-risk, randomness-source or runtime
authority claim follows from the example.
