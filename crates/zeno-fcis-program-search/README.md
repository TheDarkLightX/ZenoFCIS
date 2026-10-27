# zeno-fcis-program-search

Untrusted search for smaller finite decision programs that behave like a given
one. **The search proposes; the transform checker decides.**

Given a canonical `zeno-fcis/finite-i128/1` or main-route arithmetic
`zeno-fcis/finite-i128/3` program (`program.zcve`), the
library looks for a program with fewer instructions (then fewer canonical
bytes) that gives identical outputs, or the identical failure kind, on every
declared input tuple. The proposers run in a deterministic portfolio:

- don't-care resubstitution (a node may be replaced wherever its value cannot
  reach an output);
- top-down synthesis with case splits, arithmetic and equality inversion and
  two-valued selection, counterexample-guided on a seeded sample for large
  domains;
- decision-list folding;
- reduced decision-diagram resynthesis over input variable orders;
- programs supplied as seeds, for example the output of `zeno-fcis optimize`.

Values, traps, output-domain refusals and canonical bytes come from the
`zeno-fcis-synthesis` library evaluator and encoder. The library is `no_std`,
reads no clock, file, environment or randomness, and the same input and
settings give the same bytes. Explicit budgets bound the domain size, memory,
work and rounds; over a budget it returns a typed stop and no program.

The graph grammar retains main-route multiplication, rounded division,
full-product `mul_div_floor`/`mul_div_ceil` and exact two- or three-factor
product comparisons. Source arithmetic subgraphs can serve as top-down atoms;
their values come from the library evaluator. This does not enumerate every
possible arithmetic expression. Refusals, including zero denominators and
negative full-product operands, remain part of the complete comparison.

Multiply-heavy work should be split into small declared finite instances.
Three inputs in `0..=40` produce 68,921 tuples; node signatures require one
i128 cell per node per live tuple. The default 67,108,864-cell cap applies
separately to each large table. Work budgets and final comparison apply even
when a proposal is refused. A size cap does not guarantee completion within
an external stage deadline.

## Indexed branches (2.6 draft)

`Settings::default().proposers.indexed_branches` is false. Enable it to try
indexed depth-two branch proposals after the ordinary portfolio. The CLI
exposes `zeno-fcis optimize --indexed-branches`; the `search_program` example
accepts the same flag. **It proposes; the exact check accepts.**

Components have expanded tree cost at most 6: bounded components of the source
and pairwise input min/max selectors in the existing pool. Predicates compare
integer components. For each output, exact bitsets index ordered leaf pairs
that cover an example and predicates true on that example. A depth-one branch
must cover every demanded example and its guard must choose the correct arm
where only one arm is correct. A depth-two root requests two such branches.
Expanded tree cost is capped at 96; ties retain the first deterministic offer.
This bounds a search fragment and makes no optimality claim for emitted DAGs.

Above `cegis_tuples`, this route uses the existing stratified sample and adds
counterexamples from the library comparison. Index words and concurrent
scratch are charged, rounded up to i128 cells, to the plan memory budget
alongside arena values. Word intersections and examined memberships consume
work units. A budget stop returns no candidate. An enabled run can take longer
or stop where the ordinary portfolio completes.

All mutation is exclusive invocation-local scratch. Inputs and settings are
borrowed immutably; returned program bytes are owned. No index or mutable
handle escapes. Every proposal still crosses the portfolio's full-domain
library comparison, including eager traps and output-domain refusals. CLI
results additionally require the existing transform checker and receipt.

## Using a result

A returned program has been compared with the original on every declared
tuple by the library evaluator. That is the search's own consistency check, not
a judgement. Check it, keep the receipt, and replay it:

```sh
zeno-fcis transform check --original original.zcve --candidate candidate.zcve --receipt receipt.json
zeno-fcis transform replay --original original.zcve --candidate candidate.zcve --receipt receipt.json
```

A checked program may also be supplied to `zeno-fcis optimize
--with-candidate`. Adopting it in an application remains `zeno-fcis contract
adopt`, a human decision.

The `search_program` example is a file-in, file-out driver, and
`tools/measure_program_search.py` measures the search on the core-component
instances.

## What this does not claim

- No optimality: a result is the smallest program found within the budgets,
  not the smallest that exists.
- No authority: every result carries `authority: none`.
- No proof of the search itself: proposals can be wrong and are refused when
  they disagree on any tuple.
- No equivalence beyond the declared input domains. Step usage is not part
  of equivalence: the checker reports it, and it changes with the program.

## Provenance

This crate is LLM-generated. It ports to Rust the methods of an earlier
Python prototype by the same owner, who relicensed that work for this crate
under the workspace licence (MIT OR Apache-2.0).
