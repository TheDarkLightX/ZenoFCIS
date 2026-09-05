# Generated application and translation milestone

Base: `4025ccdcaaa455e3d7e5626153c92ecf6c997026`.
Worktree: `/tmp/zenofcis-generated-app-translation-20260905`.
Branch: `agent/generated-app-translation-20260905`.

This is the first implementation milestone from the architecture review. It
adds a bounded adopter journey and strengthens existing translation evidence.
It is a development candidate, not a new RC3 release or deployment approval.
Lean stays pinned at 4.30.0. No Lean installation, update, or additional local
runtime copy is authorized by this milestone.

## Scope and contracts

1. Add checked schema lowering from the existing typed authoring AST. Record
   fields and sum variants come from authored declarations. Leaf shapes and
   bounds, root selection, and schema version remain explicit reviewed inputs.
   Reject missing, extra, ambiguous, and incompatible bindings before codegen.
2. Provide one generated durable counter application. Its bounded state,
   commands, rejection precedence, committed-failure changes, and non-value
   notification channel are explicit example semantics. Generated typed
   transitions feed the existing catalog authority, genesis authorization,
   SQLite publication, restart reauthorization, and idempotent outbox APIs.
3. Exercise the actual generated application through an isolated downstream
   package: acceptance, rejection without publication, committed failure,
   duplicate replay, restart, and repeated delivery.
4. Broaden formula-export tests, including nested temporal scope, arithmetic,
   relational operators, quantification, and negative claims. Fix temporal
   variable capture in the Lean exporter with deterministic fresh binders.

The generator does not select business rules, certify machine implementations,
construct evidence from success flags, or bypass nominal authorization. The
example uses runtime-only project laws with an actual deterministic checker;
its external proof verifier rejects every proof submission. This is a local,
non-value-moving teaching application, not a qualified production profile.

## Preservation obligations

- Existing canonical value, AST, patch, law, authorization, and storage formats
  retain their identifiers and meanings. Corrected Lean source obtains new
  content-addressed obligation identities through the existing source binding.
- Existing generated projects and CLI commands retain their behavior.
- Lowering returns owned schema values and typed errors. It performs no I/O,
  authorizes no transitions, and preserves explicit stable IDs subject to the
  target schema's integer widths and limits.
- Generated application state and envelopes are immutable. Rejection does not
  publish state, replay, or outbox rows. Committed failure changes only the
  declared failure counter. SQLite retains the existing atomic commit protocol.
- No persistence optimization, migration, dependency upgrade, release action,
  or generic law-engine replacement is part of this milestone.

## Evidence plan

Retain failing tests before each implementation change. Schema tests cover
valid records/sums, missing and unused leaf bindings, role/root mismatches,
identifier overflow, and source-order invariance. The generated application
checks its full lifecycle against explicit expected state and delivery counts.
Translation tests distinguish outer and inner temporal times and keep finite
evaluation separate from unbounded theorem claims.

Run focused tests, then repository assurance, documentation, ATDD, formatting,
Clippy, workspace tests and doctests, package checks, and no-std checks as the
available environment permits. Use small build profiles with incremental
compilation disabled to limit disk use. Pinned external-prover, QEMU, Miri,
release reproduction, and full ATDD results must remain explicitly unclaimed
unless actually run. No commit is permitted without the immediately preceding
full ATDD run required by AGENTS.md.

## Implementation status

The generated journey is implemented. `zeno-fcis new <dir> --template
durable-counter` emits the application and its tests. Run the isolated
consumer gate from this checkout:

```sh
python3 tools/check_generated_application.py
```

The gate verifies that the CLI package contains every template resource,
generates into a fresh directory, binds dependencies to this checkout without
changing external locked versions, and runs formatting, Clippy, tests, and the
durable demonstration. See the
[template README](../crates/zeno-fcis-cli/templates/durable-counter/README.md)
for the exact example contract and deployment limits.

The integration exposed and repaired three gaps:

- Generated transitions previously bound only the context value. The new
  `begin_bound_transition` preserves the complete invocation commitment
  supplied by the authority, including authentication and replay bindings.
  It validates the admitted command and constructs a candidate builder; the
  existing nominal authority still owns acceptance and publication.
- Catalogs without effects emitted a Rust enum with an invalid empty numeric
  representation. Empty identifier enums now remain uninhabited and compile.
- Nested temporal quantifiers reused time-variable names. The Lean exporter
  now allocates deterministic fresh names so inner bounds refer to outer time.
  Existing source hashes and obligation identities bind the corrected bytes.

Focused evidence includes eight schema-lowering tests, all 64 bounded
application inputs, explicit incorrect-state and incorrect-plan law checks,
both capacity boundaries, exact replay, database reopen, delivery-before-ack
retry, and the resulting state `(count=1, failures=1)` with two committed
bundles and two deliveries. The destination ledger remains in memory across
database reopen in one process; process-independent destination durability is
an external contract.

The translation corpus checks 45 relational/arithmetic outcomes against
independent expected results and ten temporal formulas against explicit Lean
reference propositions, with ten definedness checks. It passed locally using
the already installed Lean 4.33.0 executable as supplemental translation
evidence. The qualified 4.30.0 pin and checksums are unchanged. The same corpus
is wired into the existing 4.30.0 CI job, reusing that job's runtime. A local
4.30.0 qualification run, runtime receipt, and production proof remain unclaimed.

Repository-wide validation is recorded in the completion receipt; this document
does not itself grant release or deployment authority.

## Next architecture work

1. Bind law and observation adapters to a reusable typed mapping, then check
   interpreter/exporter agreement over a larger bounded corpus. The current
   adapter is deliberately specific to this counter.
2. Specify an authenticated checkpoint and incremental SQLite validation path.
   Preserve restart reauthorization, rejection of corrupt history, and atomic
   state/replay/outbox publication before optimizing full-history scans.
3. Measure persistent values and incremental commitments against the current
   canonical representation. Sharing memory must preserve immutable pre-state
   behavior and exact roots; performance claims need representative workloads.
4. Consolidate repeated CI setup around the existing toolchain pins. Lean
   upgrades are excluded by the user's disk-space constraint.
