# Checker argument proofs

These synthesized proofs use only the standard Lean distribution. Run
`python3 tools/check_lean_proofs.py` from the repository root. The script
requires the already installed Lean 4.28.0 and 4.32.0 toolchains; it does not
download dependencies. It compiles fresh module snapshots, then asks Lean to
enumerate every constant owned by each proof module and collect its transitive
axioms. Private, protected, instance, definition and auxiliary declarations are
included. Only `propext`, `Quot.sound`, and `Classical.choice` are allowed.
Use `--controls` to check deliberately false assertions in temporary copies.

## Collections

[Collections.lean](Collections.lean) models scalar state, a partial map,
finitely many named keys, and views containing only scalar values, named
records and key equality patterns. `frame_renaming` and `decision_renaming`
establish invariance when these observations and patterns agree. Decisions
receive only the view (P1/P2). `Checked` is the finite checker's result as an
explicit hypothesis, including agreement of writes through aliased roles.
`MergedWrite` states the runtime's named-write and frame obligations (P4).

`run_preserves` proves that every supplied run from an empty map with lawful
scalar genesis preserves the laws and makes lawful decisions at every step,
for every key selection. The arity, decision and lawfulness relation may vary
at each step, indexing any sequence of commands and their bound inputs.
Its P3 closure is explicit: the same predicate is
required at **every** named post-entry and at every stored entry. This
predicate is independent of scalar state. A scalar-dependent entry predicate
would additionally need preservation when scalars change and an entry is
unnamed. No such extension is asserted here. The predicate includes absence,
and the empty-entry law is a premise. Function-valued maps need no support
bound; starting empty and writing finitely many keys gives the intended runs.

`role_only_counterexample` checks the missing-uniformity failure: a command
preserves a zero-value law on role `a` but writes true through distinct role
`b`; selecting that key as `a` next violates the law. `role_local_preserved`
and `role_write_frame` prove local preservation and framing for this update;
`role_named_outputs` shows that the named outputs use only observations and
the equality pattern, including aliases (the `a` write takes precedence).

This is a model-level theorem about the reduction argument, not a proof that
the Rust checker, parser, authority or runtime implements it. Enumeration of
canonical equality patterns, record/key-field encoding, finite value-domain
coverage, summaries and their aggregate identities, capacity refusal,
serialization and runtime ownership remain outside this model. No production
or release authority follows from these proofs.

## Solver-proposed strata

[FixedPoints.lean](FixedPoints.lean) uses pointwise Boolean implication as
the order on `Fin n → Bool`. `least_of_stable` proves that any adjacent
repeat in iteration from all-false is the least fixed point; the dual
`greatest_of_stable` uses all-true. Thus the first repeat suffices, without
trusting a solver's claim that it is the first. Both require monotonicity.

`ascending_repeats` and `descending_repeats` produce an index `k ≤ n` with
iterate `k` equal to iterate `k + 1`. There are at most `n` strict changes;
detecting the repeat can require `n + 1` evaluations, including when `n = 0`.
`lfp_least` and `gfp_greatest` show that taking `n` iterations already yields
the appropriate fixed point. The proof counts true coordinates and shows
that comparable, distinct bit vectors have different counts.

`nested_stable` handles `greatest z. least y. H(z,y)`, with `H` monotone in
both arguments. It accepts arbitrary proposed inner iteration lengths and
an arbitrary outer length. Each inner chain starts at all-false; every
evaluated inner chain, including the final outer step, must have an adjacent
repeat. The outer chain starts at all-true, follows those computed inner
results, and must also repeat. The theorem identifies its result with the
nested fixed point and proves it greatest for the exact inner operator.
The comparison is `z[K] = z[K+1]`, so it checks `K+1` outer evaluations.

The wrong-seed counterexample uses the identity on one bit: starting at
all-true is stable but is not least. The controls also reject a claimed
repeat for one-bit negation, which oscillates without monotonicity.
This proves an abstract greatest/least nesting. It does not prove the game
semantics, the full GR(1) encoding with conjunctions and an additional inner
greatest fixed point, Boolean complement translation, table expansion, the
Python proposal generator, or the implementation of the `stable` law.

## Checking and axioms

The module audit uses Lean's environment ownership and `collectAxioms` API;
source-text patterns do not select declarations. In addition, each named
theorem in [AXIOMS.json](AXIOMS.json) must have exactly one `#print axioms`
report matching both the manifest and its imported kernel declaration on each
toolchain. `least_of_stable` and `greatest_of_stable` are axiom-free;
`run_preserves` and `nested_stable` use exactly `propext`, `Quot.sound`, and
`Classical.choice`. Other theorem reports are printed in full by the script.
Defect controls first compile the unmodified model, then add a named false
assertion in an isolated copy. Only rejection by `decide` proving that
assertion false counts as a killed control; build or transport failures do
not count. Temporary copies are removed, and the proof sources are unchanged.
The proof sources also refuse `sorry`, `admit`, `axiom`, `native_decide`,
`implemented_by`, `extern`, `unsafe`, and `csimp` tokens, even in comments.
This conservative textual guard is defence in depth, not declaration coverage.
The gate tests compile five declaration forms containing a native decision
proof, bypass the textual guard, and require rejection by the module audit.
Their ordinary proof variants pass; an allowed private theorem must be counted
and pass as well. The parser and module-coverage tests run with
`python3 -B -m unittest discover -s tools -p test_check_lean_proofs.py`.

## Background and contribution

The collections argument follows Pierre Wolper, “Expressing Interesting
Properties of Programs in Propositional Temporal Logic,” POPL 1986, for data
independence. Finite monotone fixed-point iteration builds on Alfred Tarski,
“A Lattice-Theoretical Fixpoint Theorem and Its Applications,” Pacific Journal
of Mathematics, 1955. These files formalize the scoped checker arguments and
premise counterexamples. Comparative prior-art search pending; no novelty claim.
