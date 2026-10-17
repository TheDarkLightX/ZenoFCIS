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

Main-route arithmetic is available as `*`, `div_floor(a, b)`,
`div_ceil(a, b)`, `mul_div_floor(a, b, denominator)`,
`mul_div_ceil(a, b, denominator)`, and `products_ge`.
`products_ge(a, b, c, d)` compares `a*b >= c*d`;
`products_ge(a, b, c, d, e, f)` compares `a*b*c >= d*e*f`.
The fused operations and product comparisons require nonnegative operands.
They use the library's full-product instructions, so an overflowing
intermediate product is not a reason to narrow or wrap it. A zero denominator,
negative full-product operand, or final quotient outside i64 refuses.
The same function names and argument order round-trip on both ZAL surfaces.
`choose(c, a, b)` is accepted as the main-route spelling of `c ? a : b`.

Constant and choice holes inside these calls are searched over their declared
finite candidate spaces and evaluated by the library. The additive inverse
solver supplies no inverse theorem for multiplication or division. An
expression hole on such a path may return `not found`; this does not establish
unrealizability. Law writers preserve the fused operations explicitly.
Scenario arithmetic uses `loop evaluate` to read a library-evaluated value at
one declared input before the existing scenario judge checks the expectation.
That value grants no verdict, receipt, resource or publication authority.

The checked `loop learn` vocabulary accepts `*`, `div_floor`, `div_ceil`,
`mul_div_floor` and `mul_div_ceil` in `vocabulary.arithmetic`.
`"products_ge"` in `vocabulary.compare` enables both four- and six-argument
forms. Each operation costs one expression node plus its arguments. These
names select fixed library operations; custom primitives remain refused.
Learn's counts and uniqueness describe only its bounded vocabulary. Lowered
examples are checked separately; qualification against a complete native
reference also requires the existing exact checker and receipt replay.

Requirements-only version-space sketches and laws accept the same calls.
Owner answers narrow the finite set of complete meanings; a singleton still
passes the existing library law evaluation, transform check and replay.
Capture expression candidates use the shared parser and lowering, followed
by the existing whole-domain comparison and receipt replay. Exposed examples
alone never establish `captured-exact`.

## Multiply-heavy size envelope

Domains remain declared finite products. The expression/sketch doors and
question/teach-back core tables have a hard default cap of 131,072 input
tuples. Law admissibility also caps work at 4,000,000 law evaluations.
For multiply-heavy questions and teach-back, use 8,192 tuples as a conservative
starting envelope, consistent with the existing judge-control envelope.
This is a planning limit, not a measured arithmetic throughput guarantee;
fixed stage deadlines can still yield `inconclusive` below it.

Reserves and amounts each in `0..=40` give 68,921 tuples for one fixed fee.
Use separate finite instances for fee basis points 0, 30 and 100. Combining
all three fee choices into a fourth input gives 206,763 tuples, above the
doors' cap. Count any additional state or context fields as input dimensions
too. A full scaled instance is within the tuple cap, but above the conservative
judge envelope; completion must be measured on the actual checker and budget.

The newer doors have smaller, separate limits. Learn defaults to a maximum
of 4,096 tuples, 2,000,000 work units, 1,000,000 cumulative cells and expression
size 12; its CLI flags only reduce those caps. Four- and six-argument grammar
enumeration can exhaust work or cells even on a small domain. Start with one
operation family and short expressions rather than the full vocabulary.
Version-space defaults to 4,096 tuples and permits an explicit maximum of
65,536, with work and cell budgets also required. Capture caps reference
acquisition at 4,096 tuples and its exact lookup encoding at 256 nodes; an
irregular table can hit the encoding limit first.

Version-space therefore uses disjoint fixed-reserve question declarations
of 1,681 tuples for a scaled cube. An ordinary capture session still needs
one table within its existing limits; separate captures establish only their
respective slices.

The owner-side `loop capture whole --spec ABI.json --reference reference.py
--expressions candidate.json --node-visits 10000000` checks a complete cover
of one larger declared nonnegative-integer input domain. Acquisition pages
retain the 4,096-point cap. Lookup pieces contain at most 16 points and split
further if their encoding exceeds 256 nodes. Every piece needs the existing
exact comparison and receipt replay; a contiguous coverage cursor must reach
the full declared size before `captured-exact`. A mismatch reports its actual
input and expected/actual observation. A partial cover is inconclusive.
The original session and Boolean-input interfaces remain available.

This whole-domain action retains the 10,000,000 cumulative node-visit cap,
62 control frames, 22-second invocation deadline, two-second check stages
and five-second acquisition stages. Two acquisition pages share a search
stage; each page has its own check stage. The frame bound permits at most
20 pages (81,920 tuples at the largest page size). A fixed-fee 41^3 domain
uses 17 pages and 52 frames. Use a separate complete domain for each fee
0, 30 and 100. Table complexity, extra outputs and deadlines can still make
the result inconclusive; these ceilings are not throughput guarantees.
The result is a checked observation of the frozen table, conditional on the
reference being a pure deterministic function, with no application authority.

`contract lemma APP --transition --lemma RELATION` separately checks a
before/after rule on every admitted committed decision through the library
Authority and its transition-law evaluator. For example,
`products_ge(post.100.110, post.100.111, pre.100.110, pre.100.111)` compares
actual post/pre reserve products. The result is labelled `checked` on its
finite domain, with a counterexample when false. Undefined or vacuous scopes
cannot pass. Rejects preserve state but have no post-law view; rejects and
refusals are counted outside the committing scope. The default lemma mode
continues to check state invariants by induction. State domains must admit
the actual successors, including reserve x up to 80 in the scaled swap.

Program search has separate domain, work and cell budgets. Its default domain
cap is 4,194,304 tuples and each large value table is capped at 67,108,864 i64
cells. Those are hard ceilings, not practical memory or deadline guarantees.
Arithmetic search atoms are drawn from the source graph rather than all
six-argument combinations. A budget stop is not a report that no smaller
equivalent program exists.

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
