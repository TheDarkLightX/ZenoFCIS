# zeno-fcis-loop-doors

Proposer-side doors for the bounded `zeno-fcis loop`. **The doors propose;
`transform check` decides.**

A proposer, a person or a model, can offer more than a whole canonical
program:

- **expressions**: one expression per program output, in the contract's
  policy-expression grammar with `c ? a : b`. A wrong proposal gets its
  failing region as a formula, the positions in its own text that could fix
  it (ranked), and a repair;
- **sketches**: expressions with holes, `?c`, `?c[lo..hi]`, `?e` and
  `?{a | b}`. A single constant hole is solved exactly over signed 64-bit
  constants. With several holes, a hole alone on its route is derived
  exactly by itself and the search branches on the others over their
  effect classes (values that make the same program are tried once); a
  hole's candidates come from its range, choices or comparison when given,
  else from a heuristic candidate list that is not complete. An expression hole is synthesized,
  and "not found" claims nothing. A sketch is called unrealizable only with
  a witness input, or after a complete stated candidate space was exhausted;
  both claims are this crate's, not a checker's;
- **family programs**: expressions over family parameters such as `$C`,
  with holes solved per instance and fitted to `a * $P + b`;
- **transfer**: moving a program from one family instance to another,
  directly, by opening the constants near a changed parameter, or by repair.

This crate does the untrusted work: parsing, lowering to a finite program
with the original's exact input and output domains, inverse semantics, hole
solving and fitting, and region formulas through the
`zeno-fcis-program-search` crate's top-down synthesis. It compares candidate
programs with the original on every declared tuple using the library
evaluator, but that is its own consistency check and carries no authority.
The `zeno-fcis loop candidate --expressions`, `loop sketch`, `loop family`
and `loop transfer` commands judge every reported program with
`transform check` and replay its receipt.

Expressions are evaluated eagerly with signed 64-bit arithmetic, as lowered
programs are: a `+` or `-` that would overflow refuses the evaluation, and
searches skip such candidates. Inverse semantics keeps exact requirements
over the whole signed 64-bit range.

The crate is `no_std` with `alloc`, reads no clock, file, environment or
randomness, and is deterministic. Its budgets bound the domain size, the
enumerated assignments and the synthesis work; over a budget a door reports
"inconclusive" or "not found" and no program.

## Counterexample-guided screening

`zeno-fcis loop fuzz` looks for differences between a canonical reference
program and a candidate with the same input and output domains:

```sh
zeno-fcis loop fuzz --original reference.zcve --candidate candidate.zcve --request fuzz.json
```

A request needs only the schema; the remaining fields have defaults. For a
one-input scalar program, a request with an initial seed can look like this:

```json
{"schema":"zeno-fcis/loop-fuzz/1","seeds":[[[0]]],"limits":{"seed":7,"trials":256,"pairs":4096,"steps":8,"corpus":64,"uniform":false}}
```

Each seed is a trace of action rows. Each row contains the non-state inputs
in ABI order. For a stateful program, add a `state` array such as
`[{"input":0,"output":1,"initial":0}]`: input 0 begins at 0 and receives
output 1 after each successful evaluation. State input positions must be
distinct, and their domains must match the successor output domains. The
caller supplies and justifies the initial state; this wiring does not establish
that it is the application's qualified genesis. Equal traps stop both traces.

The challenger replays supplied seeds, then explores boundaries, constants
and neighborhoods of retained traces. Every fourth generated trial explores
uniformly; `uniform: true` disables boundary and corpus guidance for a baseline.
The mutation sequence is deterministic for fixed programs, request and source.
The modulus-based sampling is exploratory, not a statistical sampling guarantee.
It does not allocate a table of the complete input domain.

Results have `authority: none` and `qualification: not-run`. A found difference
is reduced and replayed from the supplied initial state, and is returned as a
`counterexample`. No finding is `inconclusive`, even for identical programs.
Exit code 0 means that screening completed, not that the candidate is correct.
There is no claim that a reduced witness is globally minimal. Trials and
evaluation-pair limits include reduction and confirming replay; one pair is
two library evaluator calls. Returned `seeds` can be supplied to the next
candidate, where every observation is computed again. Source and input hashes
bind the CLI report to the owned bytes that were actually used.
The corpus is bounded: when full, its last seed can be replaced by the newly
found witness. Preserve a separate master regression corpus if every historical
failure must be retained.

This profile compares eager checked-i64 outputs and failure categories through
`Program::evaluate`. It does not check Authority publications, resource-meter
equivalence, malformed raw records, different state representations or arbitrary
temporal properties. It compares a fixed reference; requirements-only relation
checking is outside this first profile. Candidate admission still requires
`transform check` and receipt replay. Use direct full checking when it is cheaper.

The design uses structured sequences as in
[RESTler](https://www.microsoft.com/en-us/research/wp-content/uploads/2021/03/RESTler.pdf)
and bounded guidance as explored by
[AFLGo](https://mboehme.github.io/paper/CCS17.pdf). This implementation does not
reproduce those systems or their performance claims. No external fuzzer,
network service, GPU or solver is needed.

## What is not claimed

- Nothing here is optimal: a repair or completion is a checked program, not
  the smallest one.
- Family results hold on the checked instances only, never for all
  parameter values.
- A synthesis "not found" says nothing about whether a completion exists.

This crate is LLM-generated. It ports to Rust the methods of an earlier
Python prototype by the same owner, who relicensed that work for this crate
under the workspace licence (MIT OR Apache-2.0).
