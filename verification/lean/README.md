# Checker argument proofs

These synthesized proofs use only the standard Lean distribution. Run
`python3 tools/check_lean_proofs.py` from the repository root. The script
requires the already installed Lean 4.28.0 and 4.32.0 toolchains; it does not
download dependencies. It checks every theorem's printed axioms and rejects
placeholders or axioms outside `propext`, `Quot.sound`, `Classical.choice`.
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

## Background and contribution

The collections argument follows Pierre Wolper, “Expressing Interesting
Properties of Programs in Propositional Temporal Logic,” POPL 1986, for data
independence. These files formalize the scoped checker argument and a premise
counterexample. Comparative prior-art search pending; no novelty claim.
