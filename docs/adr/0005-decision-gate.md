# 0005: Library-owned decision gate

## Status

Proposed for V2. This record specifies the boundary to implement; it does not
claim that the current authority or templates enforce it. The first additive
implementation may be opt-in in 1.x. Making it mandatory changes the authority
API and belongs in V2 (see [0004](0004-v2-ledger.md)).

## Context

The finite synthesizer can check a program against a contract over every input
in a declared finite domain. The three template adapters still translate
admitted state, command, and context into that domain and turn the result into
a `TransitionDecision`. Order fulfillment synthesizes its branch and complete
successor state, but Rust still assigns typed reasons and stages outbox entries.
An exhaustive test of that adapter is useful evidence; the authority does not
currently require the test or compare its output with the finite contract.

Today `CatalogCommitAuthority::execute` calls a project-supplied
`CatalogTransitionProgram::execute` and then validates the returned artifacts.
Its build hash is self-reported. `ProjectLawEngine` supplies verdicts. This
checks structure and binding, but a structurally valid wrong decision can pass.
Neither a pure-code scan nor a repeated-execution probe establishes semantic
correctness. Calling such a decision formally guaranteed would overstate the
evidence ([0003](0003-epistemic-status.md)).

## Decision

The authority must own a gate that computes the *complete* expected decision
from a canonical contract and the exact admitted invocation. The preferred
path evaluates the contract and constructs the decision through library-owned
builders. An optimized or hand-written executor may run only when the gate
independently computes the expected decision and compares the complete
normalized result before authorization. A caller cannot provide a Boolean
"conforms" verdict, expected bytes, or a substitute contract at invocation
time. A mismatch, missing field, undefined operation, exhausted meter, or
unknown domain value refuses authorization before any commit or outbox delivery.

The contract must be closed and total over its stated admitted domain. It
binds, in versioned canonical bytes:

1. catalog/schema identities, state, command, context, initial-state and input
   domains, and any abstraction of raw values into finite facts;
2. precedence and all three decision classes, with stable reason IDs;
3. the complete successor state and canonical patch for committing decisions;
4. effect and outbox plans, including channel, destination, payload, ordering,
   and idempotency fields;
5. declared reads/writes, resource limits, and the observation of those limits
   that the gate itself meters;
6. the evaluator version and domain-separated contract commitment.

No field may be silently omitted because a template does not use it. Empty
plans and unchanged state are explicit values. Reject is a no-op on state,
effects, and outbox. A committed failure may change them only as specified.
The library derives candidate and authorization identities from its canonical
artifacts; the contract does not invent those hashes.

The gate receives the same `ReviewedTransitionInput` as execution. It admits
and decodes raw values with the authority's catalog, then evaluates the bound
contract using a deterministic, metered interpreter. If an input abstraction
is used, the library computes it from checked expressions over admitted fields;
an arbitrary project callback is not a proved abstraction. A finite proof
applies to actual invocations only after the abstraction's range and semantic
preservation are established for all admitted raw inputs. Without that bridge,
the claim remains scoped to the abstract tuples.

The comparison covers decision class and reason, pre/post state, canonical
patch, complete effects and outbox, receipt/bundle fields, resource report,
and observed footprint. The library normalizes both sides through the same
strict admission and sealing rules and compares their canonical bytes. If any
authority-bearing field lacks a canonical projection, implementation of this
gate is blocked until that projection exists. Comparing only state roots,
branch codes, law verdicts, or hashes supplied by the project is insufficient.

There is one contract identity in the authority policy, persisted
authorization, replay record, and audit certificate. The library computes it
from the actual canonical contract bytes and its evaluator version. A
project-supplied `transition_build_hash` cannot stand in for this identity.
For emitted native code or Wasm, the separate artifact identity and exact
target conformance evidence remain bound to the contract; the gate's
per-invocation comparison is the runtime backstop, not a substitute for
artifact provenance.

For each declared law, the gate evaluates a library-interpreted predicate on
the same pre-state, command, context, and complete decision. The initial state
is checked against the initial-condition predicate at genesis. An external
`ProjectLawEngine` may contribute advisory evidence but cannot upgrade a
library judgment by reporting success. V2 makes decision conformance and
initial-condition law families mandatory; 1.x may expose an opt-in path without
altering the frozen public enum or canonical format.

## Consequences

This removes project-written decision judgment from the authority path for
contracts the interpreter can express. Projects still author and review
meaning: a wrong contract can be executed perfectly. Independently written
decision examples, keyed by schema field IDs and checked without the
contract's resolver, remain necessary to challenge that meaning.

The first implementation should cover one finite template end to end before
generalizing the language. It must encode the template's reason and outbox
mapping in the contract, delete the corresponding trusted Rust choice, bind
the contract at authority construction, and show that a planted change to
each authority-bearing output is refused at runtime. Inventory reservation is
a suitable first candidate because its admitted decision domain is finite.
Order fulfillment follows after its typed reason and outbox mapping is
expressed. Account lockout needs a checked time-fact abstraction or a larger
domain/proof route before claiming exhaustive coverage.

The gate does not prove that the contract reflects user intent, external
authentication is genuine, delivery succeeds, the compiler or hardware is
correct, or the imperative shell is atomic. Those require separate evidence.
The highest composite assurance level is limited by every remaining trusted
step and by the fit of their scopes ([0003](0003-epistemic-status.md)).

## Evidence

This is a design decision, not implementation evidence. Existing finite
contract checks, template conformance tests, determinism probes, and pinned
formal-tool runs do not demonstrate the proposed runtime gate. Acceptance for
the implementation requires all of the following on the exact source head:

- a positive, nontrivial complete decision accepted for every admitted finite
  input, with an independently authored example corpus;
- planted mutations to branch, reason, post-state, patch, effect, outbox,
  footprint, and meter, each refused by the authority gate;
- an out-of-domain/undefined input and an incomplete contract refused closed;
- a tautological or domain-only property reported as such, never promoted to a
  system guarantee;
- independent agreement of exhaustive enumeration and the pinned symbolic
  route where the latter applies, with checked proof objects for any `Proved`
  claim that relies on a solver result;
- replay of persisted authorizations under the exact contract identity and
  refusal after a contract or evaluator change;
- the repository's full acceptance gate and source-bound evidence regenerated
  after the final edit.
