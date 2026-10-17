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

## What is not claimed

- Nothing here is optimal: a repair or completion is a checked program, not
  the smallest one.
- Family results hold on the checked instances only, never for all
  parameter values.
- A synthesis "not found" says nothing about whether a completion exists.

This crate is LLM-generated. It ports to Rust the methods of an earlier
Python prototype by the same owner, who relicensed that work for this crate
under the workspace licence (MIT OR Apache-2.0).
