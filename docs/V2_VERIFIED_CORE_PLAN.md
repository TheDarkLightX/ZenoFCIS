# Smaller V2: verified core and mandatory publication route

Status: implementation complete on one tree; final qualification in progress, 2026-10-04.
The owner chose simplification first, then a smaller V2. This plan supersedes
its earlier open-ended definition of done. It does not announce a release.

A **formally verified functional core** is the central architecture concept.
ZenoFCIS intends to become a **state of the art high assurance software factory**.
That is the destination; the supported guarantees and unfinished work below
define what this V2 can establish. Additional factory features remain in the
[V2.1 roadmap](V2_1_FACTORY_PLAN.md).

## Supported scope and guarantee

The supported applications are durable-counter, account-lockout,
order-fulfillment, inventory-reservation, compliance-gateway, withdrawal-queue,
agent-treasury-guard and prepared-counter, using their retained legal domains.
Their one authoritative route is:

```text
checked schema + reviewed declarative program/policy + immutable invocation
    -> library Authority: admission, execution, decisions, laws and genesis
    -> private Publication
    -> SQLite v2 shell: identity/freshness checks, persistence and replay
```

Applications declare meaning. They cannot supply an authoritative decision,
law verdict, usage report, candidate sealer or substitute evaluator.
Reject and technical refusal grant no publication capability. Reject changes
no authoritative state, effect or outbox. CommittedFailure has only the changes
specified by its reviewed branch. Replays must match the exact bound subject.

The core claim is executable conformance to the reviewed mathematical contract
for every input in the supported admitted domain, including refusal behavior.
It does not prove that the contract captures human intent or external facts.
The SQLite refinement and delivery environment have separate evidence.

## Intake and current qualification

Status on 2026-10-04, branch `agent/v2-simplify-20261003`.

**Frozen intake (2026-10-03).** All eight templates constructed a checked
library Authority; seven used `V2SqliteShell` and prepared-counter used
`HistorySqliteShell`. The intake contained the embedded source bundle, compound
walker, account producer, law bridge and separate genesis artifact wrappers.
Normal authority and native transition callbacks were already retired.
Source: templates under `crates/zeno-fcis-cli/templates/*/src/lib.rs`.

**Implemented since intake, on one source tree (S1-S5).** A generated
evaluator digest replaces the embedded source bundle. History, the History
wrappers and the pair export/import API are removed; prepared-counter runs on
`V2SqliteShell`. One scalar/record decoder and driver replaces the compound
walker and the separate record driver. Account rules are declared on the
generic path. One value model and one tagged outcome shape, retaining the
invoked `Kind`, replace the separate genesis wrappers. Admission is performed
once and carried by the `BoundCore` type invariant; the law bridge is gone and
laws read decision deliveries directly. Shared helpers live in `util.rs`.

**Checkpoints that pass on this tree.** These are the intermediate checkpoints
named under "Checkpoints and final qualification"; none is the final gate.

- Whole-core proof: `verification/verus/authority_v2.rs` with the pinned
  flags reports 835 verified, 0 errors. All 468 Exec functions have zero
  `requires` and at least one `ensures`. The core contains no `admit`,
  `assume`, `external_body` or `#[verifier::external]`. Its two `rlimit`
  attributes (`decision.rs` 30, `authority/spec.rs` 20) are unchanged from
  the baseline; `seal` and `prepare` gained proof assertions after formatting
  instead of a raised limit. The baseline reported 1338 verified and 625 Exec
  functions; unit counts overlap and are not additive, and the decrease comes
  from removed code, not dropped checks.
- Native: the core harness's 134 native tests pass; strict Clippy on
  `zeno-fcis-synthesis` (all targets), the no-std harness build and the
  no-default-features check pass; the workspace is rustfmt-clean.
- Gate positives: the 14 gate profiles are regenerated from fresh pinned
  positive proofs. The positive runs of twelve gates, plus the authority,
  guarded-laws and original-output positives, pass on this source. The
  shared-runtime gate's proof subject (`domain_bounds.rs` with
  `finite_bounds.rs`) is unchanged from the baseline. The account-data
  gate's positive suite runs in the ATDD.
- Templates: the six finite synthesis manifests are regenerated (only the
  `certificate` field changed; programs, vectors and emitted source are
  byte-identical) and the Rust, Python and JavaScript replays pass. The
  kernel-laws oracle template copies are re-synchronized and their policy
  files regenerated. `check_template_contracts_v2.py` reports matching
  declarations and library-encoded policies for all eight templates.
- Compatibility: `test-data/v1-compatibility/baseline.json` pins the V1
  consumer's documented V2 migration, the five API changes listed in the
  [migration guide](V2_PROGRAM_API_MIGRATION.md#v1-consumer-migration). The
  native zUSD mount is retired from packaging and its workflow deleted.
- Mutation-control inventory: seven controls retired, one replaced, one
  added and two moved from the composition gate to the laws gate, each with
  its reason, in `verification/verus/simplification-retired-controls.json`.
  Anchors are re-pointed, and each must match exactly once (token-wise in
  the finite-execution and catalog gates).

**Open before acceptance.** Nothing below is claimed.

- Gates 2, 3 and 4: the full mutation controls of all 16 proof gates
  (`check_verus`, `check_finite_execution`, `check_metered_execution`,
  `check_canonical_bytes`, `check_protected_input`,
  `check_envelope_frame_v2`, `check_schema_binding_v2`, `check_decision_v2`,
  `check_account_graph_v2`, `check_laws_v2`, `check_catalog_v2`,
  `check_original_output_v2`, `check_publication_v2`,
  `check_composition_v2`, `check_guarded_laws_v2`, `check_authority_v2`) on
  the final source. The profiles are regenerated (above), but no full
  mutation round on the final source has completed; rounds run before the
  anchor repair are not evidence either way. Under the owner decision below,
  CI runs them on the pushed commit, and every retained or replacement
  control must fail there for its stated reason.
- Gate 6: the workspace, template and shell regressions must pass in the
  ATDD run before commit and in exact-head CI. These include
  `check_generated_application.py` for all eight templates and the
  account-lockout boundary cases.
- Gate 7: `python3 tools/atdd.py run --all` immediately before the commit;
  independent scope, source and evidence review (the Astra review packet);
  exact final source identities; exact-head CI on the committed revision.
  The umbrella crate's API custody fixtures run with all features in the
  ATDD (`tools/check_api_refusals.py`); custody of the direct crates'
  exports stays with the independent review.
- Shell: the SQLite shell re-decodes each delivery's destination and payload
  under `DecodeLimits::default()` to rebuild the V1 `OutboxEntry` it commits
  to. A value outside those limits is refused at persistence, so the path
  fails closed. The review decides whether those limits must match the
  core's.
- Ledger: the 19 route-required entries in
  [V2_LEDGER_SCOPE.md](V2_LEDGER_SCOPE.md) stay open until the above pass;
  the 13 withdrawn entries keep their recorded reasons.

Implementation acceptance is distinct from merge, deployment and release
authorization. Verus/Z3 results keep their stated trusted base and are not
Lean `KernelChecked` evidence.

## Bounded definition of done

All seven gates apply to the same frozen source tree and reviewed scope.
Missing, stale, filtered or development-only evidence leaves its gate open.

1. **One route and one core representation.** Qualify the approved sequence
   S1, S3, S2, S4, S5: a 32-byte evaluator manifest digest; one scalar/record
   decoder and driver; account rules declared on the generic path; one value
   model; one tagged artifact shape with genesis as the identity candidate.
   Remove History and pair export/import; migrate prepared-counter to SQLite
   v2. Keep defensive rechecks if removing them needs executable preconditions.
   Check: stage reports, native touched-crate tests, whole-authority proof,
   migrated-test ledger and `check_generated_application.py` for all eight.
   Check public custody with the existing compile-fail/custody controls and an
   independent review of root, direct-crate and feature-enabled exports.

2. **Library work and accesses.** The private meter starts at zero and charges
   protected work before each attempt. Exhaustion preserves previous charges
   and exposes no partial authority. Mandatory checked declarations determine
   the logical ingress selectors, complete successor assignment targets,
   branch delivery plans/channels and law observation nodes. Library execution
   constructs attempts only from those declarations; recorded `permitted`
   flags report meter grants. Ingress reads all declared fields, and successor
   writes include unchanged values; the changed patch is a subset. This is a
   reviewed derived bound over verified components, not a separately proved
   universal footprint theorem or parallel-disjointness certificate.
   Check: renew translated order profiles, meter/protected-input/composition
   and law/sealing controls, and source review after the remaining stages.
   See [ledger row 22](V2_LEDGER_SCOPE.md); no additional caller-supplied
   footprint report or duplicate runtime whitelist is required.

3. **Executable proof coverage and composition.** Run the whole source harness
   `verification/verus/authority_v2.rs` with the pinned Verus flags
   `--no-cheating --no-external-by-default --num-threads 2 -V spinoff-all`.
   Inventory every reachable Exec body: zero `requires`, positive `ensures`,
   no unreviewed assumption, `admit`, opaque body or filtered proof.
   Preserve surviving public postconditions and checked bridges. Regenerate
   source/body profiles after edits and rerun all retained and replacement
   mutations, checking the reason for each failure. Check: full authority and
   publication gates, translated inventory, native shared-source comparisons,
   no-std harness, and independent source/specification review.

4. **Policy and evaluator identity.** Generate the fixed evaluator digest from
   the sorted source manifest outside the verified core, and check it against
   the approved sources in the required gates. Cargo does not regenerate it. Exclude
   test-only files and the generated digest; retain Cargo.lock, dependency
   manifests and toolchain pins required by the compiled evaluator. Replace
   FINITE_V2_* pins with that digest. Bind exact policy and evaluator identity
   at Authority construction and compare it on create/open/commit/replay.
   Check: planned `v2_evaluator_identity` workspace test plus manifest checks
   registered in ATDD and CI. Omission, source edit and digest-flip controls
   must fail. Different-policy replay and stored-genesis-identity tampering
   must refuse through the surviving shell API; removing the comparison must
   make a negative control fail. Record every identity change separately.

5. **SQLite and delivery preservation.** The shell accepts only genuine
   publications, enforces the pre-state root/version, stores authorization
   certificates and chain integrity, and reexecutes persisted subjects.
   If storage strips identity bytes, splice/strip must preserve exact replay.
   Keep prepared-counter's 16,384-byte aggregate publication cap and original
   capacity, partition, crash, cancellation, freshness and retry cases.
   Delivery IDs must bind the real policy/history, record and transaction
   certificate. Recurrent state, identical payloads, different transactions,
   lanes and ordinals must remain distinguishable; an identity digest alone
   is insufficient. Preserve commit-order delivery and destination collision
   refusal. Check: migrated SQLite/prepared-counter lifecycle suites, tamper
   and crash controls, recurrent-state delivery negatives, and I/O review.

6. **Application meaning and domain preservation.** Retain all legal inputs,
   decision examples, precedence, required successful cases, laws and effect
   order for the eight templates. Re-run independent decision comparisons,
   six finite template replays (12,064 retained inputs), and all eight native
   generated-app journeys. Account-lockout must retain now/seen values from
   0 through 4,102,444,800 and until values through 4,102,445,700. At the largest
   legal now, now+900 equals 4,102,445,700; the next now value must refuse as
   outside the admitted domain. This is not machine-integer overflow.
   The approved simplification design removes its specialized correspondence
   lemma. Generic graph execution proves the graph's declared semantics;
   fact-class/boundary replay is bounded evidence of agreement with the rules,
   not an all-timestamps correspondence theorem. State this assurance delta
   explicitly. A template-local proof over generated constants may retain the
   stronger theorem. Check: full-domain descriptor comparison, fact-class
   replay, independent rule review, native conformance and boundary cases.
   Malformed-account refusal classes and regenerated Step limits are explicit
   approved design changes; they must not silently shrink legal inputs.

7. **Final acceptance and review.** Apply the [32-row ledger scope](V2_LEDGER_SCOPE.md):
   qualify its 19 route-required entries; preserve the recorded reasons for
   withdrawing 13 broader requirements from this V2 completion gate.
   Update API migration and user-facing descriptions to this exact scope.
   Run `python3 tools/check_template_contracts_v2.py`,
   `python3 tools/check_generated_application.py`, required native/Clippy,
   no-default/no-std and Miri checks, full mutation gates (run in CI; see
   "Checkpoints and final qualification"), and
   `python3 tools/atdd.py run --all`. The ATDD run is immediately before any
   commit. Recheck exact final source identities and required exact-head CI;
   obtain independent scope/source/evidence review. Implementation acceptance
   is distinct from merge, deployment and release authorization.

## Checkpoints and final qualification

Intermediate stages retain frozen sources, whole-core proof and executable
inventory, touched native/template/shell regressions, strict/no-default/no-std
checks and independent review. They are provisional implementation checkpoints.

**Owner decision, 2026-10-04: mutation suites run in CI, not before the local
commit.** The owner deferred local mutation-control runs so that working 2.1 and
2.2 releases come first. The GitHub `verus.yml` workflow runs every gate and
every control on the pushed commit, so the suites still run on the exact final
source; they run in the cloud after push instead of locally before commit.

The local pre-commit gate for V2 is:
- the whole-core proof, with its executable-function inventory;
- every gate profile regenerated from a fresh pinned positive proof;
- native, strict Clippy, no-std and no-default checks;
- the API custody fixtures (`tools/check_api_refusals.py`);
- the full ATDD run immediately before commit;
- independent review of the exact source.

Every retained control, or its reviewed replacement, must still fail for its
intended reason in CI. A control that does not is an open defect, not a pass.
One is already known: in a stopped local run, the guarded-laws control
`change_declared_law_order` did not report as caught. The CI run decides it.

## Trusted base and evidence strength

Name the reviewed specification and translation, Verus/vstd
`0.2026.09.27.3cf1832`, Z3, verifier Rust 1.98.1, native Rust 1.97.1,
dependencies, allocator/platform and hardware. Name the manifest
producer/checker and SHA-256 assumption for evaluator identity.
Compiling without the required digest checks no longer establishes the former
include_bytes source-binding guarantee. Name SQLite,
OS/filesystem, hash provider and observed destination acknowledgements for
shell claims. The logical meter does not measure physical CPU, allocation,
construction-time hashing, database scans or external delivery.

Verus/Z3 evidence has its own stated scope and trusted base. It is not Lean
`KernelChecked`. A bare solver unsat report does not become `Proved` under
[ADR0003](adr/0003-epistemic-status.md). Historical receipts qualify their own
sources; copies, source scans, tests and review do not qualify a changed tree.

## Retained history

Earlier qualified units established shared scalar admission/eager execution
(`9882136`), the owned meter (`f82bd84`), canonical integer readers (`f88f1a3`),
protected flat records (`5f88a67`) and typed multi-record composition
(`1ed6f88`). Bounded decision, law/genesis, schema/catalog and authority receipts
followed in isolated worktrees. Their contracts, negative evidence and limits
remain historical; none is a receipt for the simplified combined route.
The earlier full plan is recoverable at
`refs/simplify/baseline:docs/V2_VERIFIED_CORE_PLAN.md` in the local recovery tree.
That ref is not yet a published recovery artifact. Preserve a reachable recovery
commit or source archive before claiming remote recovery of removed V2.1 code.

## Work outside this V2 core claim

V2.1 gains a **new supported profile** for 12-type compound-value execution and
full-width U128 zUSD, including producer/laws/root patch and successful wide
values such as the retained 2^63 deposit. Narrowing those domains is not
qualification. The baseline contains recoverable code and historical evidence,
not an implemented end-to-end replacement for that profile.

Release engineering separately owns QEMU boot, portable source transport and
mirrors, archive/privacy scans, the private historical oracle and package/version
cutover. This scope decision does not remove the release gates before shipping,
or allow these artifacts to authorize the core. V2 core acceptance can be
reported separately from release readiness.
