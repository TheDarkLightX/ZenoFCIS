# Count outcomes and bound accumulated costs

Use `contract risk` to ask an exact finite question before changing a contract.
It reads `project.zeno` and `v2/policy.json`, uses the bound library Authority,
and emits an advisory JSON packet. It grants no runtime or publication authority.

## Start with a small declared model

The [swap pool](../examples/quantitative-risk/swap-pool/README.md) has reserve
0..4, last fee 0..2, a slippage flag, swap amount 0..3 and a unit context.
A swap exceeding reserve commits a failure, preserves reserve, and sets the
flag. An accepted swap reduces reserve and charges 1 for amount 0 or 2 for a
positive amount. Reward 610 declares the post-decision fee, with failure cost 0.
This LLM-generated example uses integer token units and a flat fee schedule;
it is a finite reserve/slippage model, with no price curve or live-market claim.

## Count declared inputs

```sh
zeno-fcis contract risk examples/quantitative-risk/swap-pool \
  --where 'post.100.112 == 1' --format json
```

The answer is count `36`, total `120`, share `3/10`. All pre-state, command and
context positions participate in the Cartesian product, including unreachable
pre-states. The share is a cardinality ratio. Interpreting it as a probability
would require a uniform distribution over that entire product; no such input
frequency or entropy claim is made here.

`result.satisfying_witness` and `result.other_witness` contain a complete input,
decision class and successor. A missing kind has a null witness. Counts, scalar
coordinates and rational components are decimal strings. Boolean input
coordinates use 0 and 1, and predicates use the existing formula syntax.

## Count distinct reachable states

```sh
zeno-fcis contract risk examples/quantitative-risk/swap-pool \
  --states --steps 3 --where 'post.100.112 == 1' --format json
```

This counts distinct states reached from an Authority-admitted genesis by zero
through three command/context choices. It includes genesis, counts a state
once even when many runs reach it, and retains a shortest witness run for each
reported kind. A state property reads state fields in a genesis frame, using
`post.100.FIELD`. A state property that reads command or context is malformed.

## Find both accumulated reward extremes

```sh
zeno-fcis contract risk examples/quantitative-risk/swap-pool \
  --reward 610 --steps 3 --format json
```

The best value is `0` on the empty run. The worst value is `6`, attained by
three swaps of amount 1. Each extreme contains an exact reduced rational,
length, and full input tuples in execution order. The horizon is inclusive;
K=0 still admits and checks genesis. Signed rewards are supported: negative
cycles can reduce the minimum within the bounded horizon. Equal-cost ties
follow deterministic enumeration; a shortest extreme witness is not promised.

The judge reuses the declared reward projection and its explicit failure
reward. It sums the integer per-decision reward using the reward-measure
rational arithmetic (signed i64 numerator, u64 denominator). It does not apply
the reward's expectation, variance, threshold or excess bounds. Random-bit
inputs become nondeterministic choices for extrema. This is no expected-value,
tail-risk or distributional claim. Accumulated fuel is supported only when the
model declares it as a reward; work counters are not fuel costs.

A rejected command has no declared reward and makes an extrema query UNKNOWN.
For state counting a reject preserves the state and consumes a command. A
committed failure advances to its actual successor, and its extrema cost must
be explicitly declared by the reward. Refused Authority invocations, missing
projections, undefined properties and arithmetic bounds are UNKNOWN, with no
partial exact answer. No refusal is silently dropped or conditioned away.

## Read completion before using the answer

Without `--out`, the packet goes to stdout and a summary to stderr. With
`--out risk.json`, the packet is saved there and `--format json` selects a JSON
summary. Schema: `zeno-fcis/contract-risk/1`. The packet binds project and policy
hashes, checker identity, question, limits and measured work. `authority` is
always `none`.

Exit 0 means the finite answer completed. Exit 2 means UNKNOWN; `claim` is null.
Exit 65 means malformed declarations or property, exit 64 means invalid option
combinations, and exit 3 means an input/output file failure. There is no refuted
exit: this command computes quantities rather than comparing risk limits.

The defaults and hard caps are 1,048,576 declared tuples, 16,777,216 work units
and K=64. `--max-tuples` and `--max-work` can lower the first two. A larger
requested cap is clamped; K above 64 is UNKNOWN. The declared Cartesian domain
must fit even for a reachability query. There is no sampling or boundary fallback.
Work charges calls, folds, discovery, relaxation and scalar run copies. It
excludes compilation and library-internal meter work; inspect the contract's
own execution limits separately.

## Method, checks and scope

At each exact depth and state, min-plus retains the least prefix cost and
max-plus the greatest. Every outgoing transition adds the same declared reward
to either retained prefix. Future choices depend only on the current state and
explicit command/context, so discarding other prefixes preserves both extrema.
The final result compares all layers 0..K. Distinct-state counting unions those
layers. Declared-input counting independently visits the complete product.

`tools/risk_reference.py` lists every command string on ten small swap models
and five horizons, using Python integers and `Fraction`, then independently
replays both kinds of count witnesses and every extreme witness. The three
controls in `tools/check_risk_controls.py` replace input counting with reachable
states, max with min, or K with K-1. Each mutant must build and fail its named
assertion. These checks cover the specified finite examples and implementations;
they are not a proof about an arbitrary contract or a release qualification.

## Lineage

Background (bibliographic details marked VERIFY await source verification):

- Leslie G. Valiant, “The Complexity of Enumeration and Reliability Problems,”
  SIAM Journal on Computing, 1979. Model counting lineage. VERIFY.
- Mehryar Mohri, “Semiring Frameworks and Algorithms for Shortest-Distance
  Problems,” Journal of Automata, Languages and Combinatorics, 2002.
  Min-plus/max-plus path aggregation. VERIFY.
- Diane Maclagan and Bernd Sturmfels, *Introduction to Tropical Geometry*,
  American Mathematical Society, 2015. Tropical semiring background. VERIFY.
- Marta Kwiatkowska, Gethin Norman and David Parker, “PRISM 4.0: Verification
  of Probabilistic Real-Time Systems,” CAV, 2011. Quantitative verification
  background; this judge does not implement PRISM's model classes. VERIFY.
- Christian Dehnert, Sebastian Junges, Joost-Pieter Katoen and Matthias Volk,
  “A Storm is Coming: A Modern Probabilistic Model Checker,” CAV, 2017.
  Quantitative verification background; no Storm backend is invoked. VERIFY.

What is new here: this is an integration of established finite enumeration and
path aggregation methods; prior-art search pending. Scope: exhaustively checked
over the declared finite domains when the packet completes, with an explicit
bounded horizon. There is no unbounded, continuous, production or financial-risk
certification claim.
