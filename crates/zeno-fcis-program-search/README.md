# zeno-fcis-program-search

Untrusted search for smaller finite decision programs that behave like a given
one. **The search proposes; the transform checker decides.**

Given a canonical `zeno-fcis/finite-i64/1` program (`program.zcve`), the
library looks for a program with fewer instructions (then fewer canonical
bytes) that gives identical outputs, or the identical failure kind, on every
declared input tuple. Five proposers run in a deterministic portfolio:

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
