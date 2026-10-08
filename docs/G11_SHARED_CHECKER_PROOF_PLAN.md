# Shared finite checker: implementation and proof plan

Status: the actual package-library route passed pinned whole-unit proof, independent
inventory review and focused native checks. Archive isolation, mutation execution
and the final integration/release gates remain open.
Implementation baseline: e45fc7fd55d662bc9366862b113c5e9263823f9c.
Integration base: a1ce03e2bcd917ecb9a61a29be429f5ca09a85b8.
Normative intake: full G11 design (168 lines), updated V2.1/V2.2 plan
(1118 lines), source/evidence handoff (114 lines), repository AGENTS and
Probity, functional-core-safety and proof-engineering instructions.

## Definition of done

One source for validation, direct metered observation, ordered enumeration,
exact comparison and tally serves both CLI F3 and SQLite successor checking.
The actual code verifies under pinned Verus 0.2026.09.27.3cf1832 with no new
axioms, external bodies or trusted callbacks. The qualification gate binds the
real evaluator dependency closure, shared source, adapters and receipts, checks
complete translated contracts/bodies, compiles native Rust 1.97.1, and runs
negative controls in CI. F3 known answers, planted defects, shell upgrade tests,
and installed source-archive journey must pass. Root owns final integration,
full ATDD, independent review, commit and publication.

## Proof sketch

- Define valid domains, exact mathematical cardinality, mixed-radix rank and
  the tuple at each ordinal. Prove product overflow and first-empty precedence.
- Prove minima and the odometer's exact successor/reset contract, then rank
  bijection including the one empty tuple at zero dimensions.
- Relate direct `execute_v2` results and Step usage to the already-verified
  public `execution` specification, with all failures and outputs intact.
- Induct over the scan prefix: exact ordinal, earlier equality and exact Step
  aggregates. Derive first mismatch and complete equality in both directions.
- Prove post-scan Step-limit partition and deterministic identity shortcut;
  keep ABI/domain/cap refusal before shortcut and mismatch before Step refusal.
- Qualify actual caller routing, adapter mapping and frozen receipt bytes;
  state separately the byte-import and compiler assumptions.

## Ownership and preflight

The checker is a pure invocation boundary over borrowed immutable program and
domain slices. Only fresh tuple/tally scratch mutates. No input mutation,
interior mutability, ambient state, callbacks or retained scratch is introduced.
Owned counterexample outputs escape; completed summaries have private fields.
CLI admission, canonical receipt serialization and shell publication remain in
their adapters. No migration, authority, evaluator or canonical source changes.
The lock-bound evaluator digest is regenerated for the declared dependency edge;
the evaluator algorithm and canonical policy bytes are unchanged.
The CLI depends on the existing shell-sqlite crate at exactly `=1.1.0`, which
publishes before the CLI. Production CLI source no longer includes sibling shell
files. Its public comparison-data API forwards to the same private checker and
projects its private completed summary into count/usage data. The actual API is
included in the proof harness; its five total forwarding contracts must verify
and appear in the reviewed executable inventory. Completed equality and store
authority constructors remain private. The read-only claims-binding digest
getter grants no authority. A test-only upgrade-source shim remains explicitly
listed in the packaged-test-input manifest.

Preserved semantics: exact ABI kinds and order; late empty domain wins over
prior u128 overflow; last-input-fastest order; eager traps; full result payloads;
true attempted Step counts; first mismatch beats budget refusal; terminal
coverage refusal and checker version. The new evaluator identity changes its
receipt field and derived receipt hash; all other known-answer fields are preserved.
The shell may
skip identical graphs only after validation; the CLI never skips usage scans.

## Evidence and open gates

All build/prover commands require Root's bounded resource slot. Initial work is
source edits, formatting, source inspection and cheap Python tests only.
Required final evidence: positive whole-unit Verus and reviewed VIR inventory;
native direct-source and both actual consumer tests; F3 receipt replay; shell
successor/upgrade tests; source-archive build; CI mutation controls; exact-tree
independent review. A source digest is binding evidence, not a proof of wrappers.

## Checkpoint 2026-10-07

Shared runtime extraction, real-evaluator ghost contracts, rank/scan/tally proof
source, direct-source harness and six focused native cases are authored. Ten
cheap qualification-driver tests pass; they test routing, source inventory,
mutation anchors and strict failure classification, not theorem truth.

Pinned Verus attempts use MemoryMax=4G, MemorySwapMax=0, CPUQuota=100%, one
verifier thread, a 600-second timeout, the shared auxiliary lock, and retained
source/tool hashes. Attempts 1–6 exposed frontend/specification-visibility and
quantifier-trigger issues. Attempt 7 reached a whole-unit result of 873 verified
groups and 11 errors. Attempt 8 reached 891 verified groups and 4 errors, with
no VIR error and stable source/tool hashes. Its remaining failures concern scan
exit facts, output-comparison context and rank-injectivity arithmetic. The
corresponding repairs passed attempt 9: **896 verified groups, zero errors**,
whole-unit verification, no frontend/VIR errors, stable source and pinned tool
hashes. Its complete translated inventory has 1,157 functions and 492 executable
functions; none has a narrowed runtime precondition or missing postcondition.
That inventory still requires independent review. No failed result qualifies G11.
Evidence is retained in the owner's local `proof-work/attempt-*` directories;
these development runs are not release artifacts. The 109-file actual evaluator
dependency closure remains byte-identical to the e45 parent.

An unreviewed candidate is bound to attempt 9's exact VIR and report. Native
run 1 passed with stable source hashes: direct-source harness 134 tests; CLI
transform unit tests 38; CLI process tests 9; SQLite equivalence tests 12; SQLite
upgrade integration tests 26. Every command exited zero under the shared lock
and resource caps, using Rust 1.97.1 and an owned disk-backed target.

Independent review found and repaired a gate-only issue: resource exhaustion or
an unrelated proof failure could previously count as a semantic mutation kill.
The driver now requires pinned whole-unit failure reports, exit 1, every error
block attributed to the declared function and exact source path, consistent
diagnostic counts, and absence of resource/frontend/crash errors. Coverage
controls require a clean positive proof and only the intended translated field
or inventory delta. The order mutant now advances matching tuple and domain
coordinates from the first input. These driver changes do not alter the proved
checker; their new source snapshots and cheap tests are separate evidence.

At that checkpoint no reviewed checker profile existed. The qualification driver
refuses absent or unreviewed profiles. The later package-route checkpoint below
records the independently accepted replacement inventory. Source-archive,
mutation execution and final integration requirements remain open.

The ATDD scenario runs only the eleven cheap gate-admission tests. The separate
Verus CI matrix entry runs the complete qualification driver, including 24
prepared proof/coverage mutations. Neither registration is evidence that CI or
the mutations have passed. Root must regenerate the hotspot baseline after
integrating the bounded G11 changes with concurrent stage-one work.

## Package-library route follow-up

The earlier 896-group proof and native counts describe the frozen pre-library-route
snapshot, not the new public API. Its executable wrappers must establish the
original exact comparison/admission/cardinality/successor relations without new
runtime preconditions. The comparison postcondition is an existential projection
of the original private completed-result theorem; it does not replace the
real-evaluator specification. Added negative controls plant incorrect projected
counts, cap forwarding and Step limits, and a weakened projection contract.
They remain unexecuted until the qualified mutation lane runs.

Eleven cheap Python tests pass after source routing changes. They reject an API
outside the proof harness, a wrong dependency version, sibling production reads,
and reversed publication order. They are not Rust, theorem or archive evidence.
The lockfile has exactly one new dependency edge and therefore changes the
source-bound evaluator identity; Root owns final identity/profile/fixture refresh.
See [the package route qualification plan](G11_PACKAGE_ROUTE_CHECKS.md) for the
required archive-isolation and compiler dep-info checks.

## Final package-library proof and native checkpoint

The final forwarding API uses the original public `FULL_BUDGET` constant directly.
Pinned proof04 verifies **901 groups, zero errors**, including all five public
wrappers. Independent review accepted its full 1,168-function inventory: 497
executable, 552 specification and 119 proof functions. Every executable function
has zero runtime preconditions, a postcondition and a covered body. The committed
profile is the reviewed candidate with only its `reviewed` flag promoted.

Actual native checks passed: direct-source harness 135; CLI transform unit tests
26; CLI process tests 9; shell equivalence tests 13; shell upgrade tests 27; CLI
contract migration tests 4; shell data migration tests 4; live behaviour tests 5.
The shell cases previously copied into CLI tests now run in the actual shell crate.
The test-only upgrade shim's display fallback accommodates the library's
non-exhaustive failure enum; production matching remains exhaustive.

All seven frozen CLI vectors were compared before refreshing three expected
reports and their three canonical receipts for the new evaluator identity.
Their other fields, counts and usage were identical, and the four refusal or
counterexample vectors were unchanged. The custody gate now includes the actual
CLI fixture directory; a fixture-edit regression brings its cheap suite to 13
tests. These checks preserve the distinction between proved checker semantics,
adapter tests and the still-required archive and mutation qualification.

## Integration qualification checkpoint

The positive-only gate was replayed against the integrated code: 901 groups,
zero errors, all five total public wrappers covered, and the native counts above
passed again. Fourteen cheap gate tests cover admission, source binding and strict
mutation attribution. Complete mutation execution remains a CI requirement.

The isolated production build and complete test dep-info scan were executed on
36 development archives. The derived 102-row test-input manifest was independently
reviewed. The first archive run's raw-index custody check failed despite stable
source and HEAD hashes; the final archive-member and staged-content follow-up is
still required. The acceptance registry now includes the shared-checker scenario
and recognizes 59 scenarios. Registry validation is not a full acceptance run.
