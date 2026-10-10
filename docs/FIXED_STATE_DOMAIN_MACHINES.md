# Fixed-size domain machines

## Purpose

This package adds an executable reference model for composable domain machines.
It connects the proof-carrying `CompositionSpec` boundary to deterministic,
sequential execution over a compile-time fixed state matrix.

The model is intentionally narrow:

```text
exact CompositionSpec
+ fixed machine interfaces
+ schema-admitted state matrix
+ schema-admitted command/context matrix
+ per-machine deterministic budgets
+ pure domain-machine implementations
    -> Accept | Reject | CommittedFailure
    -> fixed successor-state matrix
    -> fixed output matrix
    -> exact per-machine resource reports
```

## Inputs

`ExecutableComposition<MACHINES, STATE_SLOTS, PORTS>` binds:

- one exact `CompositionSpec`;
- exactly `MACHINES` interfaces in canonical component order;
- exactly `STATE_SLOTS` owned state cells per machine;
- exactly `PORTS` input positions and output positions per machine;
- schema type and schema commitment for every command, context, state cell,
  input port, and output port;
- concrete access paths for every state cell and active port;
- an internal route matrix derived from the specification's exact wirings;
- the specification's exact deterministic merge order.

One execution additionally receives:

- a state matrix admitted by the executable composition;
- one admitted command and context per machine;
- one pure `DomainMachine` implementation per component;
- one immutable `BudgetLimits` value per component.

Clocks, randomness, databases, filesystems, networks, threads, async runtimes,
and mutable global state are not inputs.

## Outputs

Execution returns one `SystemExecution`:

- `Accept` contains the complete successor matrix and emitted output matrix;
- `Reject` contains the rejecting component and stable project reason, with no
  candidate matrix or output authority;
- `CommittedFailure` contains the successor and output matrices through the
  failing component plus that component's stable project reason;
- a fixed report row records the decision kind, limits, and exact logical usage
  for each component that ran.

The output matrix is evidence about machine emissions. It is not a
`CommitPlan`, `OutboxPlan`, or executable shell instruction.

## Authority boundary

This crate is an executable semantic reference. It does not grant production
commit, effect, delivery, proof-promotion, or deployment authority.

Construction authority is deliberately split:

1. `CompositionSpec` owns reviewed component identities, footprints, frames,
   wiring, merge order, and proof claims.
2. `ExecutableComposition` owns the fixed matrix shape, typed interfaces, and
   exact route derivation.
3. `SchemaAdmittedTypeEnvelope` owns structural and schema admission for every
   runtime value.
4. `DomainMachine` proposes one local decision using only its row, command,
   context, inbox, and deterministic limits.
5. The global executor validates every local candidate against the exact
   interface before it can enter a successor matrix.

`zeno-fcis-composed-program` now owns the reviewed root projections, exact
machine implementations and build commitments, closed reason domains, output
treatment, and deterministic budgets. It turns this executor into one
`CatalogTransitionProgram`. `zeno-fcis-authority` then binds the exact
invocation, catalog, project laws, provider, interpreter, deployment, replay
policy, and transition build before constructing production commit authority.
Production parallel promotion still requires complete static footprint
evidence and equality with the canonical sequential result.

## Trusted dependencies

The crate uses only existing workspace semantic crates:

- `zeno-fcis-core` for the three-way decision algebra and deterministic budget
  reports;
- `zeno-fcis-codec` for canonical encoding and commitments;
- `zeno-fcis-compose` for component contracts, paths, wiring, and merge order;
- `zeno-fcis-project` for stable project reason identifiers;
- `zeno-fcis-schema` for schema-admitted typed envelopes.

No new external dependency is introduced. As with the existing semantic core,
correctness depends on the selected `CommitmentHasher`, schema definitions,
component implementations, compiler, and target behaving according to their
reviewed contracts.

## Deterministic resource bounds

Compile-time dimensions provide the first bound. Construction also enforces
hard library maxima:

```text
1 <= MACHINES <= 256
1 <= STATE_SLOTS <= 256
0 <= PORTS <= 256
MACHINES * STATE_SLOTS <= 16,384
MACHINES * PORTS <= 16,384
```

The underlying `CompositionSpec`, access paths, schemas, envelopes, and values
retain their own existing hard limits. Each component receives an immutable
`BudgetLimits` value and returns the exact `BudgetUsed` report produced by its
execution-local meter. No wall-clock timeout is interpreted as semantic fuel.

Execution invokes at most `MACHINES` steps and examines at most:

```text
MACHINES * STATE_SLOTS state cells
+ MACHINES * PORTS input positions
+ MACHINES * PORTS output positions
+ CompositionSpec.wirings().len() routes
```

## Laws

The implementation and tests enforce:

1. **Fixed shape.** Matrix dimensions are compile-time constants and must also
   satisfy the hard runtime maxima.
2. **Canonical row identity.** Matrix row `i` always belongs to canonical
   component `i`; merge order changes execution order, not row ownership.
3. **Exact composition binding.** Admitted state and invocation matrices retain
   the exact canonical executable-composition bytes and cannot be replayed
   under another same-shaped composition.
4. **Narrow state ownership.** Every state read and write declared by a
   component is covered by one of its concrete state-slot roots. State-slot
   roots do not overlap within or across components. Every exposed cell is in
   the complete read footprint; a write must cover a complete cell, and cells
   outside the write footprint must remain unchanged.
5. **Narrow typed ports.** Every active port binds one concrete path, nonzero
   type identifier, and nonzero schema commitment.
6. **Exact global wiring.** Routes are derived only from
   `CompositionSpec::wirings`. Every wiring maps one exact output port to one
   exact input port with equal type and schema bindings.
7. **No hidden fanout or fan-in.** One source port has at most one destination
   and one destination port has exactly one source.
8. **Forward-only execution.** Internal wiring must point from an earlier
   component to a later component in the exact merge order.
9. **Schema preservation.** Commands, contexts, pre-state, successor state,
   routed values, and emissions must match the exact interface binding.
10. **Reject atomicity.** A local `Reject` discards all provisional predecessor
   changes and returns no successor or output candidate.
11. **Committed-failure preservation.** The first local
    `CommittedFailure` terminates global execution and returns the authoritative
    successor through that component. Later components do not run.
12. **Deterministic replay.** Equal composition, state, invocation, machines,
    and limits produce equal complete executions.
13. **Canonical identity.** The composition, state, invocation, candidate, and
    execution artifacts have explicit versioned canonical encodings.

## Negative cases

Construction or execution fails closed for:

- zero or excessive matrix dimensions;
- a component count, order, identity, or profile mismatch;
- zero schema or type identities;
- wildcard interface paths;
- duplicate or overlapping state ownership;
- a read or write footprint outside the owned state row;
- an exposed state cell absent from the read footprint, a partial-cell write,
  or mutation of a read-only cell;
- a context envelope broader or narrower than the declared context footprint;
- an output outside the declared effect footprint;
- missing source or destination ports for a wiring;
- type or schema disagreement across a wiring;
- hidden fanout, hidden fan-in, backward wiring, or an unbound active input;
- command, context, pre-state, successor-state, or output envelope mismatch;
- state or invocation replay under another same-shaped executable composition;
- a supplied machine whose identity differs from its fixed row.

Infrastructure validation failures return `ExecutionError` and create no
semantic candidate.

## Assumptions

- Every `DomainMachine` implementation obeys the documented pure-transition
  contract. Rust's trait system cannot prevent an implementation in another
  crate from consulting ambient state.
- `CompositionSpec` proof evidence is checked separately through
  `verify_assume_guarantee` or `verify_deterministic_parallel`.
- Schema commitments identify the intended reviewed schema definitions.
- Component footprints are complete or are accompanied by separately verified
  completeness evidence when used for promotion.
- The exact merge order is acceptable project policy.

## Explicit nonclaims

This package does not:

- prove a `DomainMachine` implementation pure or correct;
- prove handwritten footprint completeness;
- execute components concurrently;
- support cyclic wiring, fanout, fan-in, or dynamic component creation;
- infer business invariants, conservation, authority, or validate project
  reason disposition and rejection precedence;
- convert output ports into authoritative effects or outbox obligations inside
  this crate; that mapping belongs to `zeno-fcis-composed-program`;
- provide canonical decoders for fixed-domain artifacts in version 1;
- verify a proof artifact or make ESSO, Lean, SMT, Z3, CVC5, Kani, Flux, or any
  other backend mandatory;
- establish compiler, database, chain, operating-system, or hardware
  refinement;
- grant production authority by itself.

Private tools such as ESSO may consume or verify the canonical artifacts through
the existing tool-neutral backend protocol. Universal users may plug in Lean,
SMT, Z3, CVC5, Kani, Flux, or another independent checker without changing this
semantic interface.
