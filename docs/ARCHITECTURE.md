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
   law-engine type and verified law set, interpreter type, deployment profile,
   state domain, catalog, and resource envelope.
5. Authenticated authority: `zeno-fcis-authenticated-authority` binds one
   qualified projector, exact authorized candidate, strict persisted plan,
   per-transition projection relation, and nominal authenticated publication.
6. Imperative adapters: mounted runtimes, code generation, SQLite, and other
   external tools interpret or persist already bounded values. The SQLite
   production port accepts only `CatalogAuthorizedTransition`.

A lower ring never imports a higher ring. The semantic kernel is `no_std +
alloc`, forbids unsafe Rust, and has no ambient I/O.

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
