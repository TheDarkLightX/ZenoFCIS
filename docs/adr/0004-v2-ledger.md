# 0004: V2 ledger of deferred breaking changes

## Status

Open. Entries are added as 1.x work finds them. An entry is closed when V2 is
released with it or it is withdrawn with a reason.

## Context

The 1.x line promises Cargo API stability. `tools/check_v1_compatibility.py`
pins the source of the frozen crates, and the finite synthesis sources are
hashed into checker identities. Most public enums are not
`#[non_exhaustive]`. So every improvement that would change a signature, an
enum, a canonical byte, or an identity is recorded here instead of being made in
1.x. Additive replacements ship in 1.x where they can, and they are opt-in.

## Decision

These changes are deferred to V2.

### Transitions and programs

- `Transition` and `DomainMachine` receive a library meter and a state view.
- One program family (native metered, IR, and Wasm) replaces the four
  transition abstractions.
- Remove caller-supplied `BudgetUsed` and raw `&Value` state access.
- Make `CandidateBuilder::seal`, `Budget::new`, and `Budget::finish`
  crate-private.
- Remove `GeneratedTransition`, `begin_*transition`, and `pre_state()`.
- Add `Resource::Step`.

### Values, traits, and enums

- Make `Value` variants private and the enum `#[non_exhaustive]`.
- Seal `CommitmentHasher` and `CanonicalEncode`.
- Make protocol enums `#[non_exhaustive]`.

### Laws, evidence, and status names

- Add `LawKind::DecisionConformance`, an initial-condition law kind, sealed law
  engine classes, and reason guard terms in the catalog.
- Retire the legacy relation evaluator and the shallow Lean exporter.
- Rename statuses whose names overstate their level (see
  [0003](0003-epistemic-status.md)), and remove deprecated aliases.
- `EvidenceVerifier` and `FootprintEvidenceVerifier` receive artifact bytes,
  not only an artifact hash.
- Rename `ToolKind` to `EvidenceKind` and `Proven` to a name that states its
  level. Drop `source_commit`.

### Language

- `.zeno` elaboration rejects law and claim paths that name no declared type
  or field. In 1.x they still elaborate; `zeno-fcis check` warns about them,
  and `--require-resolved-paths` refuses them.

### Codec

- Encoding validates against the supplied limits, and payload metrics agree
  with decoding.

### Domains and identities

- `DomainPrefix::try_new` and `StateDomainBinding::try_new` reject the reserved
  `zeno-fcis` namespace. In 1.x only the `try_new_project` constructors do.
- `Domain::new` rejects the reserved prefix outside a library registry, and the
  frozen crates move their domain literals into that registry.
- `finite::Contract` exposes its relation program, so
  `zeno_fcis_synthesis::system::Property` is no longer needed. This edits the
  frozen finite sources and changes checker identities once.
- Synthesis adopts a shared IR evaluator, a one-time identity change.
- Drop the `LawEvaluation` self-hash placeholder.

### Authority policy

- Declared footprints and meter configuration are mandatory.
- A library-computed program identity replaces the self-reported
  `transition_build_hash`.
- The delivery interpreter's identity is checked when it is bound.

### Storage and shell

- The invocation binds the pre-state root instead of pre-state bytes, and the
  SQLite schema stores authorization certificates.
- Pending deliveries follow commit order, not hash order.
- The shell stops revalidating the whole history on every call by default.
  An opt-in revalidation policy ships in 1.x first.
- Make `SqliteShellError` `#[non_exhaustive]`.
- Schema v7 adds a hash chain and checkpoints.

### Removals and cleanups

- Remove `ZenoDexProfileV1`, documentation stubs, and the root-type segment in
  projection paths.
- Compose parity becomes structural in the executor rather than
  caller-supplied.
- Retire the `check_assurance.py` regex rules in favor of a purity checker that
  resolves `use` paths.

## Consequences

Each 1.x package that adds an opt-in replacement names the ledger entry that
would make it mandatory. The V2 migration guide is written from this ledger.

## Evidence

- Frozen sources: `tools/check_v1_compatibility.py`, and the finite synthesis
  identity hashes checked by `tools/check_synthesis.py`.
- Opt-in replacements already shipped on this branch:
  - `try_new_project` (reserved namespace);
  - `system::Property` (a relation that the frozen `Contract` does not expose);
  - `--require-substantive` (vacuity as a failure);
  - `--require-resolved-paths` (unresolved law paths as a failure).
