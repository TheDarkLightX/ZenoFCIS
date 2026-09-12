# ZenoFCIS 1.0.0: review of the 13 historical issues against `cca3f6b`

I only read source, tests and docs in this archive. Nothing was run, so "test exists" means I read it, not that it passes. This review is advisory.

## Bottom line

I'd fix two things before tagging. Everything else is implemented, missing only some test evidence, or outside the V1 contract.

**1. Code defect (#55): the SQLite delivery membership check isn't atomic.**
- `SqliteShell::next_pending` (`crates/zeno-fcis-shell-sqlite/src/lib.rs:754-829`) does three separate autocommit reads:
  - it validates the whole history (761);
  - it selects the pending row (762-781);
  - it re-reads that candidate's rows from the database (812).
- It never compares the returned entry with the reauthorized in-memory bundle, even though `record.bundle` is already in hand at 808-811.
- `deliver_next` (889-902) gives that entry to the interpreter *before* `acknowledge` runs its transaction (837-840).
- A second writer can insert a fake row after 761 and delete it before 812. With self-consistent row-local hashes, it passes 813-827 and is delivered externally.
- This needs concurrent write access to the SQLite file. Multi-process use is a documented nonclaim, but that access is exactly #55's threat model, and `docs/STRICT_ARTIFACT_AND_SQLITE_HISTORY.md:72-73` (law 8) promises membership without conditions.
- The fix is about 5 lines with no schema or API change (see "Smallest recommended changes").

**2. Documentation overclaim (#58, affects #74).**
- `docs/PROJECT_RELATIONAL_LAWS.md:180-182` says permanent tests cover debit, credit, fee, mint, burn, asset, recipient, authority, subject and effect-count mutations "in the executable fixture law engine", plus "aggregate multi-effect imbalance".
- No such fixture or test exists. The only `ProjectLawEngine` implementations are:
  - authority tests (2545, 2582);
  - laws tests (2565);
  - SQLite tests (1911);
  - the non-value durable-counter template.
- Fix the doc, or add the fixture.

**Background facts the per-issue findings rely on**
- **Mint paths.** Production authority is only created by `CatalogCommitAuthority::execute` (authority `lib.rs:863-896`, through private `authorize_artifacts` 1858-1940) and `reauthorize_canonical_transition` (905-960). Genesis authority comes only from `authorize_genesis` (775-813). All witness types have private fields, with no `From`, `Default` or decoder.
- **Raw-bundle entry points.** Only `apply_reference_bundle` (`zeno-fcis-shell/src/lib.rs:207`, whose doc at 202-206 disclaims authority) and the diagnostic `NormalizedDecision::from_bundle` accept a `CommitBundle`.
- **Owner-selected components.** The program `P`, law engine `L`, verifiers and interpreter `I` are chosen by the owner. Their hashes are bindings, not attestation (`docs/CATALOG_AUTHORIZATION_BOUNDARY.md:212-233`). I did not count "someone writes a lying trait implementation" as forging a witness.
- **What the candidate ID binds.** `context_commitment` (authority 2030-2057) hashes `policy_id` (2045), principal, authentication evidence and replay ID into `context_hash`. So `CandidateId` and delivery IDs are indirectly bound to catalog, laws, genesis, interpreter/deployment hashes and replay. They are the same across shell implementations, but not across deployments.

---

## #54: catalog-authorized witness at shell commit

| Acceptance criterion | Status | Evidence |
|---|---|---|
| Raw balance-increasing bundle can't type-check at the production commit API | Implemented | `AuthorizedShellState::commit` 1698-1701; `SqliteShell::commit` 539-544 |
| Compile-fail test shows the witness can't be forged | Implemented | doctest `bundle.into()` 1389-1406 has correct bounds; struct-literal construction blocked by private fields 1413-1417 |
| Catalog A artifacts can't commit through a shell pinned to catalog B | Implemented, no dedicated test | `policy_id` covers the catalog hash (encode 537-540); checked at 1702-1707 and SQLite 559-561. Tests only swap *deployment* (3185, SQLite 2486) |
| Command/context/provider/domain/interpreter/deployment substitution fails | Partial evidence | command/context: `transition/tests/laws.rs:606-633`; deployment: 3185; interpreter token: SQLite 2498; principal/replay change IDs: 3024. No provider or state-domain swap test at authority level (mechanism is encode 541, 544-545) |
| Shell persists authorization identity for replay/restart | Implemented | `AuthorizationRecord` 1589; SQLite table 60-70; reauthorization on reopen 1082-1087 |
| Reference-shell tests kept on a non-authoritative API | Implemented | shell tests 520-586 |
| Fuzz target no longer treats raw bundles as the production path | Implemented (reference path only) | `fuzz/fuzz_targets/candidate_bundle.rs:66-99` uses `apply_reference_bundle`. No fuzz target covers the authority or SQLite path |

No defect.

## #55: SQLite rows cross-validated against the authorized bundle

| Acceptance criterion | Status | Evidence |
|---|---|---|
| Strict bounded decoders, exact re-encoding | Implemented | receipt 387-490; authority 905-1032; receipt tests 1025-1207 |
| Startup rejects extra/missing/changed/duplicate/stale/cross-candidate rows | Implemented for row tampering | 1057-1133, 1196-1218, 1326-1384; mutation matrix `transaction_tests.rs:9-195`. Not detected: flipping the `acknowledged` bit (explicit nonclaim, STRICT_ARTIFACT 100-101) and rollback to a consistent earlier prefix (not named in any nonclaim) |
| `next_pending` proves membership before delivery | **Partial: defect** | 754-829; see bottom line |
| Idempotent replay revalidates completeness | Implemented | 581 and 583-604, inside the IMMEDIATE transaction 571-574 |
| Redundant bundle bytes checked | Implemented | 1282-1310; test 2445 |
| Tamper tests recompute row-local hashes | Implemented | 2318, 2370 |
| Every crash point, then reopen | Partial evidence | pre-commit crash is only snapshotted on the same connection (2200); post-commit crash only retried on the same connection (2230); reopen is tested only after clean commits (2538, `transaction_tests.rs:352, 507`) |

## #56: frame authorization and parallel evidence

| Acceptance criterion | Status | Evidence |
|---|---|---|
| `overlaps` replaced by `covers` | Implemented | compose 2544; `covers` 165-178 |
| Exact/ancestor/descendant/sibling/wildcard tests | Implemented | `tests/frame_authorization.rs:118,133,142,150`; unit test 3148 |
| Effect conflicts over authority/subject/asset dimensions | Replaced by a stricter rule | any effect conflicts with any effect (401-405). Less precise, still sound |
| Explicit outbox footprints | Implemented | field 545, `try_new_with_outbox` 573; `component_conflicts` 2760-2782 |
| Value-moving effects conflict unless a verified law exists | Implemented | 2615-2634 |
| Canonical parity claim, verified | Implemented | 1867-1920, 2176-2184, 2638-2656 |
| Discharge bound to component/assumption/sorted providers/spec | Implemented | 2151-2161, 2515-2526, sort 1815 |
| Mutation tests per binding | Partial | parity mutates only one context field (3393; `composition_identity.rs:463`). Provider test 3181 swaps to an *unknown* guarantee, so artifact binding isn't isolated. No negative test that `OutboxOutbox` or `LeftEffectRightOutbox` without a law blocks (only the positive case, 3268) |

No defect.

## #57: invocation, provider, interpreter and deployment binding

| Acceptance criterion | Status | Evidence |
|---|---|---|
| Swap tests: command/context/caller/nonce/oracle/domain/catalog/profile | Partial evidence | same as #54; "oracle" is just context contents; domain only tested in refine (2225-2237) |
| Production APIs reject a lying hasher with the same algorithm ID | Implemented at commit ports | sealed trait crypto 90-92, 137-140; doctest 120-136; SQLite hardcodes `RustCryptoSha256` (292). Leftover: `authorize_deterministic_parallel<H: CommitmentHasher>` (compose 2666-2670), a planning value, not a commit port |
| Approved provider is nominal and retained | Implemented | encode 544-545; refine 1403 |
| Candidate/receipt identity binds interpreter and deployment | Implemented indirectly | `policy_id` (includes `ExecutionBinding`, 549) enters `context_hash` (2045), then `candidate_bindings` (transition 962-984) |
| Proxy/upgradeable bindings | Outside V1 | `CATALOG_AUTHORIZATION_BOUNDARY.md:246-248`; README 344-347 |
| Restart/replay compares persisted environment | Implemented | 1386-1416, 1082 |
| Expected values never taken from the artifact | Implemented | transition 358-369, 455-466; authority 1869-1880 |

- **Asymmetry, covered by a stated nonclaim.** The program's configuration hash is checked (736, 867). The interpreter instance is only bound to `policy_id` (763-772), not compared to `delivery_interpreter_profile_hash`.
- **Principal visibility.** Principal and authentication hashes are not visible to `ReviewedTransitionInput` (287-295). Authorization logic must use the admitted context (documented assumption, 214-215).

## #58: value conservation and effect/state relations as proof obligations

| Acceptance criterion | Status | Evidence |
|---|---|---|
| Profile declares complete law set | Implemented | `LawManifest::try_new` 434-493; policy/registry binding 1357-1371 |
| Every committing candidate checked before authorization | Implemented | authority 1882-1911 |
| Mutating debit/credit/fee/mint/burn/asset/recipient/authority/subject/effect count fails | **Absent** (framework only) | engine is project-supplied (1117-1131); the only mutation tests are non-value, in `durable-counter/tests/laws.rs:100-181` |
| Rejection keeps no patch/effect/outbox authority | Implemented | `LawDecisionView::Reject` 820-823; `seal_reject` 833-867; framework law 1218-1228; test 2892 |
| Aggregate multi-effect conservation tested | **Absent** as an executable test | — |
| Chain backends reproduce results | Outside V1 | PROJECT_RELATIONAL_LAWS 209-210 |
| Evidence import bound to source/profile/algorithm/coverage | Implemented | 1781-1857; tests 2825, 2847 |

**Defect:** documentation overclaim at `PROJECT_RELATIONAL_LAWS.md:180-182`.

## #61: fabricated exhaustive refinement

| Acceptance criterion | Status | Evidence |
|---|---|---|
| Repeated input hashes fail | Implemented | 1168-1174; test 2354 |
| Arbitrary `domain_hash` fails | Implemented | legacy path always blocked (1447, 1468-1473; test 2325); validated path requires a verified claim over the manifest (1547-1561, manifest bytes 1651) |
| Missing/duplicate/reordered/extra members | Implemented | 771, 1193-1200; tests 2384, 2409 |
| No `ValidatedNormalizedDecision` from raw fields | Implemented | private 330-333; doctest 317-328; only `try_from_untrusted` 337-393 |
| Artifact mutation fails reconstruction | Implemented | 379-381; test 2241 |
| Exhaustive coverage needs verified enumeration | Implemented | 1554-1561; test 2481 |
| Zero-case needs an explicit verified declaration | Implemented | 772-775; test 2531 |

- **Design choice.** Expected bindings are a raw `CandidateBindings`, not an `InvocationWitness`, because of crate layering.
- **Inherent limit.** Two identical, valid but fabricated bundles still compare as exact (nonclaim, VALIDATED_REFINEMENT 122-130).
- No defect.

## #62: projector and sparse-proof context

| Acceptance criterion | Status | Evidence |
|---|---|---|
| Different projector identity gives non-interchangeable evidence | Partial, by design | `projector_hash` 54-95; `verify_against` 799-801. The root (1053-1099) isn't separated by tree or profile, so a relabeled proof verifies only where the expected root is equal, which is still a true statement |
| Replay under another tree/profile/version/root/key fails | Implemented | 795-818; test 1647 |
| Projector that omits a changed value is detected | Implemented through the required relation engine | authenticated-authority 617-634; test 1546 |
| Verified witnesses private and context-bound | Implemented | 912-915; doctest 899-910 |
| Plans can't apply independently of the authorized candidate | Partial | `publish` takes the nominal type (798-818), but neither the authority nor the port is pinned to a semantic `policy_id` (config 940-954; checks 583-599, 802-807). All tests use a private `TestAuthorization` (1387-1430), so the real `authorize`/`publish` path and its `SemanticAuthorizationView` implementation (900-938) are untested |

- **Leftover (not blocking):** a same-typed authorization from another deployment's policy is accepted.
- Cross-store atomicity is a nonclaim (AUTHENTICATED_AUTHORITY 181-182), so this doesn't block. Pinning the policy changes a constructor, so decide before freezing the 1.0 API.

## #67: complete static footprints

| Requirement | Status | Evidence |
|---|---|---|
| Nominal witness bound to component/profile/program/footprint/schema/catalog/algorithm/source/toolchain/verifier/method/coverage | Implemented | 1375-1387, 1528-1533, 1683-1687, 1732-1755 |
| Proof methods 1–4, no bounded-test method | Implemented | 1233-1242, 1260-1330 |
| One witness per component, footprint equals spec | Implemented | 2679-2736 (2711-2719) |
| Changed build/schema/catalog/profile/footprint invalidates | Implemented | `complete_footprint.rs:241` |
| Observed outbox ⊆ declared outbox | Partial (definition missing) | the transition `Footprint` has no outbox set (266-271); `enqueue` (706-714) records nothing; `covers_observed` (314-319) ignores outbox; `observed_outbox` exists only in `COMPLETE_FOOTPRINT_WITNESS.md:143` |
| Adversarial tests (rare branch, hidden outbox, …) | Partial | set-level test only (`complete_footprint.rs:218`); hidden branches are a property of the external verifier |

No runtime consumer uses this (doc 27-28). Not blocking.

## #68: mechanized soundness theorem

**Outside V1.** `V1_PRODUCT_CONTRACT.md:95-98` and `QUICKSTART.md:305` explicitly exclude a mechanized end-to-end theorem.
- The Rust side has partial analogues: nominal witnesses, law-completeness checks, the reject-no-authority rule, and the SQLite-vs-reference trace test (2131).
- There is no model, extraction or theorem. Not a runtime defect. No Lean change recommended.

## #72: law-verified, policy-bound genesis

| Acceptance criterion | Status | Evidence |
|---|---|---|
| Law-violating schema-valid state can't initialize | Implemented | 775-813, 1644-1671, SQLite 343-382; tests 3314, 3269 |
| Genesis root and evaluation bound into policy/record | Implemented | encode 542-543; body 574-581; table 43-53 |
| Separate create and reopen APIs | Implemented | 313-341 |
| Reopen validates without caller-supplied state | Implemented | 947-1042; tests 2538, 2562, 2672, 2717 |
| Changing owner/supply/deployment/law set invalidates | Implemented | root pin 784-786; tests 3269, 3285 |
| Compile-fail: raw envelope can't initialize | Guaranteed by types; **doctest doesn't test it** | doctest 637-646 uses `<(), (), (), ()>` with `let _: T = value`, which fails on unmet bounds and type mismatch even if a `From` impl existed. `GENESIS_AUTHORIZATION.md:88-90` relies on it |
| SQLite tamper/reopen fails before commit/delivery | Implemented | 2562, 2592, 2672, 2696, 2735 |

## #73: shared delivery identity

| Acceptance criterion | Status | Evidence |
|---|---|---|
| One versioned preimage and domain | Implemented (option A: candidate-based) | plan 213-223; OUTBOX_DELIVERY_IDENTITY 12-28 |
| Reference and SQLite byte-identical | Implemented | shell 291-293; SQLite 1465-1467, 1367-1369, 819-825; test 2176 |
| Candidate-vs-authorization mutation test | Implemented | 2184-2190 |
| Parity incl. ordering, acknowledgement, crash/reopen replay | Partial | single-entry parity only (2131); `AuthorizedShellState` has no acknowledge path |
| Migration/version handling | Implemented | 919-945; tests 2752, 2777, 2800 |
| Destination failover between shells | Absent test | — |
| Docs and parameter names match | Implemented | wording note: OUTBOX_DELIVERY_IDENTITY 26-28 is literally true, but the candidate ID already commits to policy and replay |

## #74: economic law families derived from the catalog

| Acceptance criterion | Status | Evidence |
|---|---|---|
| Value-moving operation can't omit its families | Implemented for declared flows | laws 1484-1555; value-moving effects rejected, catalog 1032-1037 |
| Derived set bound into identity | Implemented (recomputed from the bound catalog) | 1339; authority 417-419 |
| Custom value flow needs an evidenced claim | Implemented | 1556-1572; test 2455 |
| Reclassification invalidates law set and policy | Implemented; partial test | catalog test 2102; no stale-law-set test at authority level |
| Per-family mutation tests | Implemented (representative flows) | 2357, 2379, 2401, 2481 |
| Aggregate conservation required | Required, not tested | 1507; see #58 |
| Solidity/Solana catalogs | Outside V1 | — |

`OperationSemantics::non_value` (239-244) accepts any nonzero hash. Honest classification is an explicit assumption (PROJECT_RELATIONAL_LAWS 198-200).

## #76: CommitPlan semantics

The chosen design is "commit effects are non-executable evidence", with the outbox as the only durable external-work channel.

| Acceptance criterion | Status | Evidence |
|---|---|---|
| Each effect definition has one exact semantics | Implemented | evidence-only forced at 451; value-moving effects rejected 1032-1037; test 2102 |
| No success without effects applied or scheduled | Implemented | outbox rows in the same transaction 719-721; test 2268 |
| Interpreter/deployment identities verified and policy-bound | Partial, by design | 1145-1173 binds policy only; hash is binding only |
| Crash tests at every boundary | Implemented for the commit protocol | 2200, 2230; deliver-before-acknowledge crash in `durable-counter/src/lib.rs:213-222` |
| Exact replay, no duplicate effects | Implemented | effects are never executed; delivery IDs are idempotent |
| Conservation checked against real outcomes | Outside V1 | COMMIT_EVIDENCE 101-104 |
| Same surface in reference and SQLite | Implemented | — |
| Docs distinguish patch, evidence and outbox clearly | Partial | normative docs are clear, but old "authoritative effect/plan" rustdoc remains: project 147, 248; catalog 840, 966, 1111; transition 1111; receipt 171; refine 65; laws 4; and bootstrap `templates.rs:285`, which writes it into generated adopter code |

---

## Smallest recommended changes

1. **#55, in `next_pending` right after `record` is obtained (811):**
   ```rust
   if pending.authorization_id != record.authorization_id
       || !record.bundle.outbox_plan().entries().contains(&pending.entry)
   {
       return Err(SqliteShellError::CorruptOutbox);
   }
   ```
   - This follows the same pattern `snapshot` already uses (checking against the in-memory root, 512-514).
   - Behavior is unchanged on consistent databases. Existing tamper tests already fail at 761 first, so their expected error variants stay the same.
2. **#58:** rewrite `PROJECT_RELATIONAL_LAWS.md:180-182`. Say the generic suite tests manifest derivation and observation completeness, and that economic-mutation and aggregate tests are the adopting project's job. Alternatively, add a small value-moving fixture engine.
3. **#72:** rewrite the genesis doctest like the transition one: generic `H, P, L, I` with real bounds and `value.into()`.
4. **Optional, API-affecting, decide before 1.0 (#62):** pass the semantic `policy_id` into `AuthenticatedCommitAuthority::try_new`, include it in `authenticated_configuration_id`, and check it in `authorize_view`.
5. **Documentation only:**
   - Define the outbox path encoding, or state that the verifier defines it (#67).
   - Clean up the "authoritative effect" rustdoc (#76). This changes the generated bootstrap bytes.
   - Add "rollback to a consistent earlier prefix is not detected" to the SQLite nonclaims (#55).
6. **Not blocking, API note:** `AuthorizedShellState::commit(self)` (1698) drops the shell state on any error, and the type isn't `Clone`.

## Summary for root

| # | V1 status | Blocks release? | Remaining gap / action |
|---|---|---|---|
| 54 | Implemented | No | Add catalog and state-domain swap tests |
| 55 | Partial | **Yes (small code fix)** | Membership check against the in-memory bundle in `next_pending`; rollback nonclaim |
| 56 | Implemented (stricter conflict rule) | No | Negative outbox-conflict test; isolated provider-artifact test |
| 57 | Implemented (proxy bindings outside V1) | No | Parallel authorization accepts any hasher (planning only) |
| 58 | Partial | **Yes (doc/evidence)** | Fix `PROJECT_RELATIONAL_LAWS.md:180-182` or add fixture |
| 61 | Implemented | No | — |
| 62 | Implemented with gap | No (API decision) | Semantic policy pin; real `authorize`/`publish` test |
| 67 | Implemented at authorization level | No | Observed-outbox definition |
| 68 | Outside V1 | No | — |
| 72 | Implemented | No | Genesis compile-fail doctest doesn't test the claim |
| 73 | Implemented | No | Multi-entry and acknowledgement parity test |
| 74 | Implemented (classification trusted) | No (shares #58 doc fix) | Solidity/Solana outside V1 |
| 76 | Implemented (non-executable effects) | No | Rustdoc terminology cleanup |

## Regression tests for root to run

Existing suites:
```bash
cargo +1.97.1 test -p zeno-fcis-shell-sqlite --locked
cargo +1.97.1 test -p zeno-fcis-authority -p zeno-fcis-crypto -p zeno-fcis-refine --all-features --locked   # includes doctests; refine provider-crossing test needs libcrux
cargo +1.97.1 test -p zeno-fcis-compose -p zeno-fcis-laws -p zeno-fcis-transition -p zeno-fcis-catalog -p zeno-fcis-receipt --locked
cargo +1.97.1 test -p zeno-fcis-authenticated -p zeno-fcis-authenticated-authority --locked
python3 tools/atdd.py run --all   # durable-counter template law/lifecycle tests
```

New narrow tests to add:
1. **SQLite membership.** Pull the new check into a helper. Assert it rejects the replacement entry from test 2328-2347 (with recomputed hashes) and accepts the committed entry.
2. **SQLite crash then reopen.** For each pre-commit `CrashPoint`, reopen with `from_existing_connection` and assert the snapshot equals the one before.
3. **Authority substitution.** A different catalog or state domain gives `PolicyMismatch` at both `AuthorizedShellState::commit` and `SqliteShell::commit`.
4. **Compose.** `OutboxOutbox` and `LeftEffectRightOutbox` without a law produce `ParallelConflict`. Swapping to a *known* provider guarantee while keeping the old artifact gives only `MissingAssumptionDischarge`.
5. **Authenticated authority.** Run a real `CatalogAuthorizedTransition` through `authorize` and `publish`. Include a second-policy authorization: it currently succeeds, which documents the #62 gap.
6. **Genesis doctest.** The rewritten compile-fail doctest.