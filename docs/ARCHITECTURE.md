# ZenoFCIS architecture

ZenoFCIS separates protocol meaning from runtime mechanism.

## Dependency rings

1. Foundations: `zeno-fcis-core`, `zeno-fcis-value`, and `zeno-fcis-codec`
   define decisions, deterministic budgets, immutable values, canonical bytes,
   and commitment-provider interfaces.
2. Semantic structures: project profiles, schemas, patches, plans,
   composition, and receipts add closed protocol values without ambient I/O.
3. Construction and checking: catalogs, transitions, relational laws,
   refinement, evidence, authenticated-state planning, bounded synthesis, and
   backend protocols validate and compose those values.
4. Nominal authority: `zeno-fcis-authority` binds an externally admitted
   invocation to one approved provider, reviewed transition type, exact project
   law-engine type and verified law set, outbox-delivery-interpreter type, deployment profile,
   state domain, catalog, and resource envelope.
5. Authenticated authority: `zeno-fcis-authenticated-authority` binds one
   qualified projector, exact authorized candidate, strict persisted plan,
   per-transition projection relation, and nominal authenticated publication.
6. Imperative adapters: mounted runtimes, code generation, SQLite, and other
   external tools deliver or persist already bounded values. The SQLite
   production port accepts only `CatalogAuthorizedTransition`.

`CommitPlan` contains non-executable evidence published atomically with state.
`OutboxPlan` is the only generic durable external-work boundary, and value-moving
effect definitions fail catalog construction.

A lower ring never imports a higher ring. The semantic kernel is `no_std +
alloc`, forbids unsafe Rust, and has no ambient I/O.

## Proposers, judges, and effects

The rings implement one rule: untrusted components propose values, and small
deterministic checkers judge them. A judgment is carried by a type that only
the judging code can construct, so later code relies on the type rather than
on a convention.

Proposers, none of which is trusted to be right:

- project transitions, whether written by hand, generated, or written by an
  LLM;
- the synthesis search, SMT solvers, and Lean;
- mounted runtimes, such as the ZenoDEX zUSD engines;
- rows read back from storage.

Judges:

- ZCVE/1 strict decoding, which accepts only canonical bytes;
- schema, catalog, and invocation admission;
- the authority's own execution of the reviewed program, its requirement that
  every applicable project law hold, and its byte-for-byte re-execution of
  persisted transitions;
- exhaustive checking of finite programs and properties;
- replay of solver models through the interpreter;
- known-answer checks of the hash provider;
- re-authorization of the complete history when a store reopens.

Judgments carried by types with private constructors:

- `AdmittedValue` and `AdmittedEnvelope` for canonical, bounded values;
- `VerifiedProvider` for a hash provider that passed its known answers;
- `CatalogAuthorizedGenesis`, `CatalogAuthorizedTransition`, and
  `CatalogAuthorizedAuthenticatedCommit` for publication authority;
- `VerifiedCompletion` for a checked finite exit plan.

Effects happen only in the shell, only on authorized values:

- atomic publication of state, receipts, and the outbox;
- delivery of outbox entries under a stable delivery identity, which lets a
  destination deduplicate retries.

`CommitPlan` records evidence and is never executed.

What each judgment establishes differs: some recompute a result, and some only
record a component's report. Law verdicts, for example, come from the project's
law engine, and the authority enforces them without recomputing them.
[Design record 0003](adr/0003-epistemic-status.md) classifies each by level,
scope, assumptions, and trusted base.

## Current nonclaims

The workspace implements all layers above as bounded Rust libraries and
reference adapters. It now makes a complete profile-bound relational-law set
and fresh per-invocation evaluation mandatory before production authorization.
Each promoted project still has to supply and independently qualify its exact
conservation/invariant definitions and checker implementation. The library now
implements strict SQLite history reconstruction, manifest-backed exhaustive
refinement, complete-footprint witnesses, strict authenticated transport
decoding, and candidate-bound authenticated publication authority. A concrete
production authenticated datastore, qualified project-specific projectors and
relations, cross-store atomicity, and chain deployment qualification remain
project work. Concrete Lean, Flux, Kani, SMT, private ESSO, and Morph checkers
still require separately mounted adapters and exact evidence. These gaps block
an official production value-moving profile, but not publication of the
reusable core library.
