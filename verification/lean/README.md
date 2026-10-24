# Checker argument proofs

These proofs use only the standard Lean distribution. Run
`python3 tools/check_lean_proofs.py` from the repository root. The script
requires the already installed Lean 4.28.0 and 4.32.0 toolchains; it does not
download dependencies. It compiles fresh module snapshots, replays every
compiled module through Lean's kernel with `leanchecker`, then asks Lean to
enumerate every constant owned by each proof module and collect its transitive
axioms. Private, protected, instance, definition and auxiliary declarations are
included. Only `propext` and `Quot.sound` are allowed. `Classical.choice` is
refused by both the module auditor and its Python acceptance check.
The gate refuses every symlink in the proof tree before invoking Lean,
including directory, file, non-source and broken links.
Use `--controls` to check deliberately false assertions and the removed
summary-delta premise in temporary copies. Use `--report FILE --transcript FILE`
to retain the complete audit and output.

## Collections

[Collections.lean](Collections.lean) models scalar state, a partial map,
finitely many named keys, and views containing only scalar values, named
records and key equality patterns. `frame_renaming` and `decision_renaming`
establish invariance when these observations and patterns agree. Decisions
receive only the view (P1/P2). `Checked` is the finite checker's result as an
explicit hypothesis, including agreement of writes through aliased roles.
`MergedWrite` states the runtime's named-write and frame obligations (P4).

`step_preserves` and `run_preserves` require `[DecidableEq K]`. The latter
already had this premise. Integers, enumerations and records of these have
decidable equality, so the added step premise excludes only key types the
checker cannot use. The [checked reduction](CollectionsRequiresEM.lean)
shows why it is needed: an unrestricted one-role preservation principle
implies excluded middle for arbitrary propositions. The amended step proof
uses the supplied equality decision rather than classical choice.

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
serialization and runtime ownership remain outside this original module.
The extensions below cover some of those separate obligations. No production
or release authority follows from these proofs.

## Several maps, summaries, genesis and tallies

[CollectionsAggregate.lean](CollectionsAggregate.lean) defines finite unique-key
partial maps, extensional lookup, insertion/removal, distinct write batches and
nonnegative totals. `total_ext` makes totals independent of row order;
`namedTotal_le` gives coverage by the whole total; `summary_step` proves the
exact aggregate update equation.

[CollectionsReduction.lean](CollectionsReduction.lean) has a finite dependent
family of key and record types. `binding` constructs a representative for every
distinct named key. `frame_admissible`, `step_preserves`, `run_preserves` and
`collections_reduction` preserve lawful decisions, uniform laws, alias agreement
and any finite family of whole-map summaries. There is no bound on map sizes,
keys or the number of steps. The entry predicate is scalar-independent and
includes absence. The semantic P2 factorization, P3 identification of the state
laws and P4 exact merged write are explicit hypotheses. The P1 renaming
consequence follows from P2 when the inputs, scalar/summary/record observations
and key-equality patterns agree; this does not prove a syntactic key-use
restriction unnecessary in the production language. A production checked packet is not itself a proof of the
module's `Checked` predicate.

[CollectionsReductionGenesis.lean](CollectionsReductionGenesis.lean) represents
absence with a presence bit and the supplied least field values.
`genesis_check_iff` equates the single canonical state-view check with all laws
in the supplied genesis-law list at every named-key/input selection.
It does not translate the production `LawSet` selector or formula interpreter.

[CollectionsReductionTallies.lean](CollectionsReductionTallies.lean) fixes the
target amount, source grouping, nonnegative weight and Boolean selector.
`affected_covers_all_deltas` covers all keys by target writes and old/new source
groups. `tally_step` and `tally_run` preserve grouped count/sum invariants under
empty genesis, unique source maps and writes, exact extensional publication,
and the finite affected-key delta check. [CollectionsSignedTallies.lean](CollectionsSignedTallies.lean)
proves conservation also for integer weights. This redundancy of nonnegativity
does not apply to the separate named-contribution coverage inequality.

## Equality patterns and executable binding

[CollectionsPatterns.lean](CollectionsPatterns.lean) constructively normalizes
any list with decidable equality by first occurrence. `enumerate_iff` and
`enumerate_nodup` characterize the restricted-growth enumeration;
`equality_pattern_coverage` gives exactly one representative of each concrete
equality pattern for every list length. Apply it separately per key type.

[CollectionsPatternsExecutable.lean](CollectionsPatternsExecutable.lean)
specifies active input/present-stored key positions, canonical absence, alias
agreement, distinct named sums and grouped tally coverage. The executable driver
[CollectionsListing.lean](../../tools/lean/CollectionsListing.lean) is outside
the trusted proof-module inventory and exports the full admitted listing.
`python3 -B tools/check_collections_reduction.py --out EVIDENCE --controls`
compares it with the native production `Shape::admissible` harness under both
Lean versions. The bounded grammar covers up to four key positions, two key
types and two maps; the remaining bounds are printed by the driver and retained in its receipt.
The grammar has at most two entry roles and two input positions, one numeric
and one stored-key field per map, one summary and one grouped tally. Numeric
record intervals are `[0,0]`, `[0,1]` or `[1,1]`; summary upper bounds are 0, 1
or 2. It has no ordinary scalar fields, ordered keys or order reads. This is an exhaustive finite
differential, not a kernel proof of the Rust parser or of all contract shapes.
`law_filter_ext` lifts equal structural admission through an identical Boolean
state-law filter; it does not establish interpreter equivalence.

## Specification validation and runtime boundary

[CollectionsReductionValidation.lean](CollectionsReductionValidation.lean)
discharges the token transition obligations by `decide`, applies C1 to arbitrary
token input streams, checks token and library genesis, and instantiates every
C3 premise on a concrete join/add/borrow/return journey. The library run theorem
also covers arbitrary input streams in its concrete finite fixture. These are
executable specifications derived from the shipped examples, not parsed policy
translations.

[CollectionsValidationWitnesses.lean](CollectionsValidationWitnesses.lean)
contains the premise witnesses and explicit refusals for canonical deletion,
conflicting and read-only aliases, supply deltas, malformed genesis, initial
conditions, unbound stored keys, arithmetic boundaries and inconclusive
transition relations. Their exact native regression correspondences and limits
are the separate obligations described in the
[collections guide](../../docs/COLLECTIONS.md#why-the-check-covers-every-size).

[CollectionsValidationPremises.lean](CollectionsValidationPremises.lean)
strengthens the premise validation with joint countermodels for semantic P2,
P3 and P4 and for T1, T3 and T4. Each establishes the other explicit hypotheses
and a failed invariant. The semantic P3 witness checks the need to identify the
declared law with the checked uniform predicate; syntactic role uniformity has
the separate witness above. The T1 witness changes the grouping interpretation
after a lawful first insertion while later writes remain empty. These results
validate the model premises without proving the parser-to-model translation.

The separate [Verus harness](../verus/collections_runtime.rs) includes production
runtime functions by `#[path]`. `python3 -B tools/check_collections_runtime.py
--out EVIDENCE --controls` proves their conditional framing/write-back contracts,
checks the complete owned proof inventory, exercises native standard-library
samples and rejects the two required runtime defects. A well-formed layout,
standard comparison laws and a stable storage-read adapter are explicit
premises. Capacity admission, SQLite publication and durability remain outside
this pure-function proof. No theorem composes all of these layers into a proof
of every production checked packet.

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
This is the two-level case. The indexed extension below supplies the finite
conjunction and additional inner greatest fixed point needed by the games guide.

## Indexed strata and the games guide

[IndexedFixedPoints.lean](IndexedFixedPoints.lean) models
`greatest z. meet_j least y. join_i greatest x. F[j,i](z,y,x)` on
`Fin n → Bool`. Every `F[j,i]` must be monotone in **all three** arguments.
`meet_monotone` and `join_monotone` prove closure of finite meets and joins;
`nested_extremal` proves the greatest/least/greatest extremal properties of
the resulting operators. Empty meets are top and empty joins are bottom,
including a zero-dimensional state space.

`value_reindex` proves invariance under relabelling either finite index set.
Surjective index maps suffice, so duplicate conjuncts or disjuncts are harmless. Omitting
an index is not covered. This equality holds even for the finite iteration
definitions without monotonicity; their fixed-point interpretation requires it.

`dual_iteration`, `complemented_lfp`, and the finite De Morgan identities
connect each inner greatest chain to the complemented least chain in the
generated tables. `tableMiddle` complements the meet of those inner tables;
`strata` complements the join of the complemented middle results. The inner
tables run exactly `n` rounds, which is sufficient under monotonicity.

`generator_stable` uses the guide's indexing: `K > 0`, outer seed top,
middle seed bottom, and the shared proposed length `L[k]` in round `k`.
For every `k` from 1 through `K` and every guarantee `j`, `L[k] > 0` and
the middle results at `L[k]` and `L[k]-1` must agree. The outer results at
`K` and `K-1` must also agree. Equality of complemented tables is equivalent
to equality of their values (`complement_eq_iff`). These checks identify
the result with the exact indexed nested fixed point. The theorem accepts
the first repeat, or any later repeat; it does not trust minimality.
`bounded_certificate` proves that `K = n+1` and every `L[k] = n+1` satisfy
the premises for every monotone family. `concrete_certificate` instantiates
them on a nonempty one-bit family. `wrong_middle_seed_counterexample`
shows that a top-seeded middle identity chain can repeat with the wrong value.

The guide may cite these results for this finite, total, monotone recurrence
and its checked stopping rule. They do **not** prove that a particular game
has those operators, that the nested value is its winning region, or that
strategies meet temporal assumptions. Full game semantics remains out of scope.
The Python source generator, parsing, table expansion, bounded arithmetic,
definedness and traps, and the executable `stable` check are not refined by
these proofs. The model's empty-family algebra does not assert that a source
generator accepts empty table or index declarations. A false `stable` table
must still be explicitly rejected by the guide's validation procedure.

## Checking and axioms

The module audit uses Lean's environment ownership and `collectAxioms` API;
source-text patterns do not select declarations. In addition, each named
theorem in [AXIOMS.json](AXIOMS.json) must have exactly one `#print axioms`
report matching both the manifest and its imported kernel declaration on each
toolchain. `least_of_stable` and `greatest_of_stable` are axiom-free;
`run_preserves` and `nested_stable` use exactly `propext` and `Quot.sound`.
No owned declaration depends on `Classical.choice`. The finite Boolean
repeat proofs use decidable, bounded pointwise comparisons. Other theorem
reports are printed in full by the script.
Defect controls first compile the unmodified model, then add a named false
assertion in an isolated copy. Only rejection by `decide` proving that
assertion false counts as a killed control; build or transport failures do
not count. Temporary copies are removed, and the proof sources are unchanged.
Compilation alone does not show that the kernel checked a declaration: an
elaborator option can add one the kernel never saw, and the axiom audit reads
the same environment. The kernel replay re-checks every declaration of the
compiled module and refuses such a module; a gate test compiles one and
requires the replay to reject it.
The proof sources also refuse `sorry`, `admit`, `axiom`, `native_decide`,
`implemented_by`, `extern`, `unsafe`, `csimp`, `skipKernelTC`, `set_option`,
`run_cmd`, `run_elab`, `run_meta`, `addDecl` and `#eval` tokens, even in
comments, and any import other than `Std` or another proof module.
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
premise counterexamples. The indexed nesting follows the GR(1) fixed-point form
in Piterman, Pnueli and Sa'ar, “Synthesis of Reactive(1) Designs,” VMCAI 2006;
only the algebraic recurrence is formalized here. Comparative prior-art search
pending; no novelty claim.
