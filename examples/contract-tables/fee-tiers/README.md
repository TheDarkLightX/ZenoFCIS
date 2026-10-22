# Fee tiers

The ask: "quote the fee for the requested tier from one fixed schedule."

## Formal model

The constant table `fee_bps` contains `[30, 25, 20, 10]` basis points. A command
has an integer tier in 0..=3. The successor stores the schedule entry at that
tier; law 500 checks `post.100.130 == fee_bps[command.101.120]`. The decision
rule reads the same lookup. Law 501 keeps the stored quote in the schedule,
including at genesis, where it is 30. Session 140 is the sole context.

The index's declared range fits all four positions. Lookup lowers to an eager
Select chain with four entries and three Selects. Both arms retain their native
traps and work; there is no branch discount. A tier type widened to 0..=4 is
refused when the source is parsed, even if another condition would filter it.

## Difference

The project and policy name one schedule instead of repeating its cases. A
computed position such as `fee_bps[3 - command.101.120]` uses the same checked
meaning. The constants add no state fields or input-domain dimensions. The
expanded source and explicit scalar policy remain independently inspectable.

```sh
zeno-fcis check examples/contract-tables/fee-tiers/project.zeno --expand
zeno-fcis contract review examples/contract-tables/fee-tiers --no-kb
zeno-fcis contract lemma examples/contract-tables/fee-tiers --no-kb --lemma 'exists i in table 110 { post.100.130 == fee_bps[i] }'
zeno-fcis contract probe decisions examples/contract-tables/fee-tiers --no-kb
```

## Scope

There are 31 schema states, four commands and one context: 124 declared input
tuples. Only the four schedule values satisfy the state law, and each is
reachable from genesis by a quote command. These are bounded contract checks.
This example stores a quoted basis-point rate; it transfers no value and makes
no claim about a trade's price, market policy, deployment or release readiness.
The expected examples are LLM-generated with the specification, so they are
review material rather than an independent oracle. Range analysis is
conservative; it does not infer correlations, rule guards or policy aliases.

## Background

Alfred V. Aho, Ravi Sethi and Jeffrey D. Ullman, *Compilers: Principles,
Techniques, and Tools*, Addison-Wesley, 1986, describes compiler lowering to
intermediate representations. Patrick Cousot and Radhia Cousot, *Abstract
Interpretation: A Unified Lattice Model for Static Analysis of Programs by
Construction or Approximation of Fixpoints*, Proceedings of the 4th ACM
SIGACT-SIGPLAN Symposium on Principles of Programming Languages, 1977,
supplies the abstract-interpretation framework; this implementation uses
conservative integer intervals to establish index containment.

## What is new here

prior-art search pending. No priority claim is made for this combination of
bounded table syntax, eager lowering and logical work accounting.
