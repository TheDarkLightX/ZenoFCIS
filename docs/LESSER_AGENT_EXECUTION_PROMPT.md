# ZenoFCIS downstream-agent execution prompt

> **Historical document.** This prompt describes the original PR #1 through
> PR #5 work-package stack and must not be used as a current branch or package
> instruction. Current agents should start with [LLM usage](LLM_USAGE.md), the
> [quickstart](QUICKSTART.md), and the exact source and open PR stack. Stable
> identifiers, authority, proof, promotion, and release decisions remain owner
> controlled.

Copy the prompt below into a fresh coding agent. Replace only the selected work-package name. Do not combine packages.

---

You are implementing one bounded work package in `TheDarkLightX/ZenoFCIS`.

## Repository and stack

The authoritative draft stack is:

1. PR #1 — semantic kernel: immutable values, ZCVE/1, deterministic budgets, `Accept | Reject | CommittedFailure`.
2. PR #2 — preconditioned canonical patches, closed plans, candidate sealing, receipts, `CommitBundle`, atomic reference shell.
3. PR #3 — compositional contracts, footprints/frame rules, exact refinement, initial ZenoDEX profile.
4. PR #4 — pinned RustCrypto and libcrux/HACL* SHA-256 providers with parity evidence.
5. PR #5 — closed schemas, value admission, deterministic Rust/Python generation, and this handoff boundary.

Read the current code and these documents before editing:

- `docs/AGENT_HANDOFF_BOUNDARY.md`
- `docs/SCHEMA_CODEGEN_BOUNDARY.md`
- `docs/SHA256_PROVIDER_POLICY.md`
- `docs/COMPOSITION_REFINEMENT_ZENODEX.md`
- `docs/CANDIDATE_COMMIT_BOUNDARY.md`

Resolve the exact current PR heads from GitHub. Do not assume the SHAs in an old prompt remain current. Start a new `agent/<bounded-description>` branch from the exact head of `agent/schema-codegen-v1`, unless your assigned package explicitly names a later reviewed base.

## Selected work package

`<REPLACE WITH EXACTLY ONE PACKAGE FROM THE LIST BELOW>`

Do not implement adjacent packages. A small complete draft PR is preferred over a large mixed one.

## Invariants you may not reinterpret

- Semantic crates use `#![forbid(unsafe_code)]`.
- The core has no ambient clock, randomness, filesystem, network, database, thread, async runtime, global mutable state, or interior mutability.
- All protocol values are transitively owned and immutable.
- Decisions are exactly `Accept | Reject | CommittedFailure`; ordinary rejection has no candidate, commit evidence, or outbox obligation.
- Stable reason precedence is explicit, total, versioned, and content-addressed.
- ZCVE/1, schema identifiers, domain-separated preimages, roots, candidate IDs, and receipts are protocol meaning.
- State updates are preconditioned canonical patches with expected pre-root and expected old-value commitments.
- Effects and outbox obligations are closed data, never closures.
- Patch, plans, receipt, replay identity, and `CommitBundle` must refer to the same candidate.
- Shell publication is expected-root atomic compare-and-swap; external delivery is idempotent and receipt-bound.
- Parallelism requires complete read/write/context/effect footprints and equality with the canonical sequential result.
- Full runtime refinement compares the complete normalized decision artifact, not only roots or success/failure.
- Bounded testing is not an unbounded proof. Evidence claims must name the exact artifact, tool version, source hash, assumptions, and coverage mode.
- Do not hand-roll cryptography. Do not make Serde, Postcard, a database, JMT, `rpds`, `imbl`, or any collection's internal shape define consensus bytes.
- An LLM may propose bounded values or examples; it may not choose schemas, synthesis grammars, wiring, proof claims, or release status.

## Required implementation process

1. Inspect the exact base diff and all public APIs you will use.
2. Write a short design note in the PR branch stating:
   - inputs and outputs;
   - authority boundary;
   - trusted dependencies;
   - deterministic resource bounds;
   - laws and negative cases;
   - explicit nonclaims.
3. Add the smallest new crate or adapter that fits the dependency rings. Do not create circular dependencies.
4. Keep semantic crates `no_std + alloc` unless the package is explicitly shell-, codegen-, CLI-, benchmark-, or tool-only.
5. Pin every new external dependency exactly and explain why it is needed. Prefer an adapter over exposing the dependency's types in public protocol APIs.
6. Add positive, boundary, negative, and metamorphic tests. Tests must include stale roots, duplicate identifiers, noncanonical ordering, wrong candidate bindings, wrong reason precedence, or mismatched evidence wherever applicable.
7. Run the pinned Rust 1.97.1 gates:
   - `cargo +1.97.1 fmt --all -- --check`
   - `cargo +1.97.1 clippy --workspace --all-targets --all-features --locked -- -D warnings`
   - `cargo +1.97.1 test --workspace --all-features --locked`
   - `cargo +1.97.1 test --workspace --doc --all-features --locked`
   - relevant `--no-default-features --target wasm32-unknown-unknown` checks;
   - `RUSTDOCFLAGS='-D warnings' cargo +1.97.1 doc --workspace --all-features --locked --no-deps`.
8. Add a permanent read-only workflow for any new assurance surface. Temporary write-enabled assembly workflows, payload fragments, caches, and diagnostics must be removed before review.
9. Open a draft stacked PR. Its body must include exact-head validation, authority boundary, assumptions, nonclaims, and the next blocked step.
10. Do not merge, mark ready, change protocol versions, or weaken an existing gate without explicit owner instruction.

## Work packages

### Package A — generated typed adapters and negative vectors

Goal: extend `zeno-fcis-codegen` so a closed schema generates inspectable Rust and Python domain adapters, not only identifier constants.

Acceptance criteria:

- generate typed record, tuple, enum, sum, bounded vector, and bounded map declarations;
- generate `to_value` and strict `try_from_value` conversions;
- generate typed path constructors for patchable fields;
- generate positive, boundary, malformed, noncanonical, unknown-field, unknown-variant, and trailing-byte vectors;
- compile the generated Rust fixture as a separate crate;
- import and replay the generated Python fixture;
- prove repeated generation is byte-identical;
- bind generator version, formatter identity, schema hash, file hashes, and vector hashes in the manifest;
- never infer stable IDs from field order or source hashes.

### Package B — mounted ZenoDEX full-decision adapter

Goal: make the current ZenoDEX zUSD Rust and Python transitions emit and compare complete ZenoFCIS decisions.

Acceptance criteria:

- define callable and canonical JSON-line adapters;
- bind exact state, command, context, policy, algorithm, schema, and precedence identities;
- normalize `Accept`, `Reject`, and reserved `CommittedFailure` without collapsing them;
- compare reason, pre/post roots, patch, commit plan, receipt, outbox plan, bundle, and decision commitment;
- persist counterexamples as canonical replay fixtures;
- fail closed on extra output, missing fields, unknown codes, noncanonical bytes, timeout, crash, or tool disagreement;
- do not grant production authority; emit refinement evidence only.

### Package C — authenticated-state/JMT adapter

Goal: plan and verify versioned authenticated-state updates behind `CanonicalPatch`.

Acceptance criteria:

- keep semantic state and canonical patch authoritative;
- define `TreeReader`, `TreeWriter`, `PlannedAuthenticatedCommit`, node batch, stale-node candidate, membership, and absence proof values;
- bind tree/profile/version/pre-root/post-root/patch hash;
- prove incremental root equals full rebuild for every fixture;
- require atomic commit of semantic state, tree nodes, root/version, receipt, replay data, and outbox records;
- implement crash-point tests and pruning-authority checks;
- use a vetted JMT implementation only behind the adapter; its internal node layout must not silently redefine existing ZenoDEX roots;
- introduce any root migration through an explicit dual-root profile.

### Package D — formal-evidence importers

Goal: make proof artifacts first-class, independently checkable promotion inputs.

Acceptance criteria:

- define canonical evidence envelopes for Kani, Lean, Z3, CVC5, Aeneas/translation validation, and codec-vector runs;
- bind tool binary/version, source commit, profile/schema/algorithm hashes, theorem/query identity, assumptions, result, and retained artifact digest;
- reject missing, stale, inconclusive, timed-out, or solver-disagreed evidence;
- add independent checker adapters rather than trusting self-reported JSON;
- distinguish exhaustive finite, bounded, and proof-assisted coverage;
- add no production claim without mounted runtime refinement.

### Package E — ESSO synthesis integration

Goal: connect ZenoFCIS schemas/contracts to deterministic ESSO synthesis without granting an LLM authority.

Acceptance criteria:

- derive synthesis holes, dependency sets, grammars, budgets, and candidate order from reviewed schemas/contracts;
- make candidate search canonical and complete within declared bounds;
- persist normalized counterexamples as blockers;
- compile accepted candidates into ZenoFCIS values and generated adapters;
- require SMT/reference refinement and composition checks before code generation;
- issue a content-addressed certificate stating grammar, bounds, trace, selected candidate, and nonclaims;
- reject truncated search as incomplete.

### Package F — concrete shell refinement

Goal: prove a real datastore and outbox implementation refines `zeno-fcis-shell`.

Acceptance criteria:

- choose one concrete database transaction API and one idempotent destination stub;
- model every crash point before, during, and after commit/delivery acknowledgement;
- atomically publish state/root, full bundle, receipt, replay binding, and outbox entries;
- reject stale expected roots and replay-key collisions;
- recover committed but undelivered outbox entries deterministically;
- bind acknowledgements to exact outbox-entry hashes;
- compare implementation traces against the pure shell model;
- keep database-specific types out of semantic crates.

### Package G — persistent-collection adapters and benchmarks

Goal: evaluate structural sharing without changing protocol meaning.

Acceptance criteria:

- implement adapters for standard `BTreeMap` builder, `rpds`, and `imbl` candidates;
- keep equality, ordering, canonical encoding, and roots defined over logical entries;
- benchmark small/dense and large/sparse workloads, retained snapshots, allocations, lookup, update, freeze, and root generation;
- differential-test every backend against the reference owned map;
- test deletion, zero-removal, alias resistance, insertion-history independence, and snapshot retention;
- make no backend default until benchmark and audit promotion criteria are met.

### Package H — security and reproducible-release assurance

Goal: turn the draft workspace into a reviewable release candidate without changing semantics.

Acceptance criteria:

- add dependency-license/advisory/source-pin policy;
- add Miri where supported, fuzz targets for decoders/patches/plans/receipts, and mutation/property tests;
- add reproducible source/generator/vector manifests;
- verify no semantic crate contains unsafe, FFI, interior mutability, ambient I/O, time, randomness, threads, or async;
- define semver versus protocol-version policy;
- produce an audit checklist and retained evidence bundle;
- do not publish crates or claim production readiness.

## Required final response from the agent

Return:

- branch and draft PR URL;
- exact base and head commits;
- files and public APIs changed;
- laws, negative tests, and resource bounds added;
- exact CI run and tool versions;
- authority boundary and explicit nonclaims;
- unresolved blockers and the single next package that should follow.

---
