# ZenoFCIS architecture

ZenoFCIS separates protocol meaning from runtime mechanism.

It intends to become a **state of the art high assurance software factory**.
A formally verified functional core is the architectural foundation; the
[2.1 roadmap](V2_1_FACTORY_PLAN.md) adds tooling for reviewing meaning, reusing
verified components and maintaining applications through change.

## Formally verified functional core

The central assurance goal for V2 is a **formally verified functional core**:
a pure, total decision boundary whose executable implementation has a
machine-checked proof of conformance to its versioned decision contract over
the full admitted input domain.

Purity is reviewed at the complete authoritative invocation boundary. Immutable
input bytes and bound policy determine the result; checked results expose no
mutable aliases to admitted state. Private, exclusively owned invocation
scratch may change while computing that result. The compound parser's meter
and read log account for actual parsing work and refusals within this boundary;
they do not make it an imperative-shell component. The shell acquires inputs,
persists authorized results and delivers effects.

Stateful preparation handles and utilities that append to caller buffers are
not individually pure decision functions. Their contracts must identify that
state explicitly. Preparation does not confer decision, resource-usage or
publication authority; the authoritative invocation evaluates and accounts for
its own work. Ownership review alone does not establish semantic correctness
or complete the machine-checked proof obligations.

The contract covers complete decisions, successor state, ordered effect plans,
laws, genesis, logical resources and refusal behavior. Verification must also
establish the connections from original-byte admission through decision
construction to authorization and replay. An open connection remains a
completion obligation.

V2's supported authority route must require library-owned verified execution,
with private checked results between stages. Applications declare rules;
synthesizers and LLMs may propose programs. Unsupported contracts are refused,
and application decision or law callbacks cannot replace this route.

Each claim binds an implementation, specification, domain and named trusted
base. Specification adequacy, external fact authentication, persistence and
delivery retain their own review and qualification obligations.

V2 remains under construction. The
[implementation and proof plan](V2_VERIFIED_CORE_PLAN.md) tracks the bounded
verified units, open bridges and release gates. The dependency rings below
describe the existing library; their presence alone does not establish the
completed V2 guarantee.

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

Proposers, whose output is checked before anything relies on it:

- project transitions, whether written by hand, generated, or written by an
  LLM;
- the synthesis search, when it selects a program;
- solver models, which are replayed through the interpreter;
- proof search, meaning the tactics that build a Lean proof, which the Lean
  kernel then checks;
- mounted runtimes, such as the ZenoDEX zUSD engines;
- rows read back from storage.

Judges:

- ZCVE/1 strict decoding, which accepts only canonical bytes;
- schema, catalog, and invocation admission;
- the authority's own execution of the reviewed program, its requirement that
  every applicable project law hold, and its byte-for-byte re-execution of
  persisted transitions;
- exhaustive checking of finite programs and properties;
- `zeno-fcis synth verify`, which replays emitted source on every admitted
  input in its target runtime;
- replay of solver models through the interpreter;
- known-answer checks of the hash provider;
- re-authorization of the complete history when a store reopens.

Trusted components, which a result names in its trusted base instead of
rechecking:

- the pinned Lean kernel and runtime, the allowed axioms, and the translation
  from `.zeno` to Lean, for `KernelChecked` results;
- a solver's `unsat` answer, which is recorded as Attested because nothing
  rechecks it;
- the synthesis search's enumeration of candidates, for a "no solution"
  result;
- a project's law engine, whose verdicts the authority enforces;
- the `finite-i64/1` interpreter, the Rust compiler, and the target runtimes.

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
record a trusted component's report.
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
