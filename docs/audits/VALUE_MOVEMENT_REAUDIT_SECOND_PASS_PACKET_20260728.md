# ZenoFCIS Value-Movement Re-audit — Second-Pass Agent Packet

## Exact target

Audit exact commit:

```text
aea9d9db3ff596ba09afed1f2f1b9b13c914f651
```

This is the integrated head of PR #70. Do not silently switch to `main`, another PR, or a later branch. Record any later commit separately as a new target.

Read first:

- `docs/audits/VALUE_MOVEMENT_SECURITY_REAUDIT_20260728.md`
- `docs/CATALOG_AUTHORIZATION_BOUNDARY.md`
- issues #55, #57, #58, #61, #62, #67, #72, #73, #74

Your goal is not to restate the first pass. Your goal is to disprove its conclusions, find omitted authority paths, or produce concrete confirming tests.

## Output format for every finding

```text
ID:
Severity:
Exact file/symbol:
Attacker or defect capability:
Precondition:
Minimum reproduction:
Expected behavior:
Actual behavior:
Authority/value impact:
Blast radius:
Smallest safe repair:
Required regression test:
Which guarantee is invalidated:
```

Do not report a concern without an exact reachable path or a clearly stated unverified assumption.

## Pass A — API reachability and construction graph

Start at every externally callable function returning or accepting:

- `CatalogAuthorizedTransition`;
- `CatalogAuthorizationDecision`;
- `InvocationWitness`;
- `VerifiedProjectLaws`;
- `BoundInterpreter`;
- `AuthorizedShellState`;
- `SqliteShell`;
- `CommitBundle`;
- `TransitionDecision`;
- composition reports/evidence;
- domain-machine execution results.

For each, draw the constructor graph and answer:

1. Can external safe Rust create it directly, via `Default`, deserialization, public fields, conversion, clone of a stale value, or another generic instantiation?
2. Can a reference/testing value enter a production port?
3. Can the exact program/checker/interpreter implementation be changed while preserving all type parameters and claimed hashes?
4. Can an authorization be replayed against another shell instance, database, deployment, or interpreter?
5. Does any public helper weaken a private-construction invariant?

Required compile-fail mutations:

- construct `CatalogAuthorizedTransition` from a raw bundle;
- initialize production shell from raw state after genesis fix;
- bind an interpreter from another policy;
- use an unapproved commitment provider;
- swap law-engine type;
- swap same-type interpreter configuration.

## Pass B — genesis and restart

Inspect:

- `AuthorizedShellState::new`;
- `SqliteShell::open`;
- `SqliteShell::open_in_memory`;
- `from_connection`;
- `initialize_or_validate`;
- schema migration/version handling.

Attempt:

1. Schema-valid state with inflated balance/supply.
2. Schema-valid wrong owner/admin/treasury.
3. Existing database reopened with another supplied initial state.
4. Existing database reopened with same policy but changed same-type interpreter.
5. Database copied to another deployment/shell instance.
6. Policy/state-domain/genesis row tampering.
7. Empty or partially initialized schema at every transaction boundary.

Prove whether state invariants and supply/conservation laws apply before the first transition. If they do not, reproduce #72.

## Pass C — ingress authenticity

Inspect `CatalogCommitAuthority::admit_invocation` and all caller-facing adapters.

Attempt substitutions one at a time:

- principal;
- authentication evidence;
- command;
- context;
- caller/signer;
- nonce/replay ID;
- oracle source/value/time/confidence/finality;
- chain/domain/deployment identity.

Determine whether the library verifies authenticity or merely commits the supplied assertion.

Build a test authenticator that always claims an administrator. Determine whether any private witness prevents it from reaching authorization.

Check whether `LawCheckInput` exposes enough exact fields to prove:

```text
principal in authenticated evidence
= principal used by transition
= authority/recipient/subject used by effects
```

## Pass D — program, checker, verifier, and interpreter attestation

For each boundary, create two values of the same Rust type with different behavior/configuration:

- transition program;
- law engine;
- law evidence verifier;
- composition evidence verifier;
- effect/outbox interpreter;
- replay policy.

Try to associate the malicious/configured value with the reviewed claimed build/profile hash.

Specific mutations:

- law engine returns `Satisfied` for every expected ID;
- evidence verifier always attests;
- interpreter sends to another endpoint/vault/asset;
- transition program chooses a hidden branch;
- replay policy returns caller-selected IDs.

A type marker is not proof of a concrete value. Require a verified identity relation or report the boundary as trusted.

## Pass E — law-set completeness

Construct catalogs containing each semantic operation:

- no-value bookkeeping;
- transfer;
- mint;
- burn;
- escrow lock/release;
- fee-bearing transfer;
- multi-asset settlement;
- external value delivery;
- committed-failure value effect.

For each catalog, attempt to mark every nonmandatory economic family `NotApplicable`.

Verify whether construction mechanically rejects missing:

- conservation;
- mint/burn authorization;
- debit/credit/effect equality;
- fee/rounding;
- authority/subject/recipient;
- aggregate multi-effect relation.

Mutate exactly one field of a valid law subject and require failure:

- debit;
- credit;
- transfer amount;
- asset;
- recipient;
- authority;
- subject;
- fee;
- rounding remainder;
- supply delta;
- effect count/order;
- committed-failure reason/effect relation.

## Pass F — SQLite exactness

Use a real file-backed database. After a valid authorized commit, stop the shell and mutate SQLite directly.

Tamper cases:

1. Insert an extra outbox row under a real authorization with recomputed local hashes.
2. Delete one required outbox row.
3. Change candidate/authorization mapping consistently in one table.
4. Change redundant authorization/bundle/receipt bytes in one table.
5. Delete replay row but retain authorization/bundle.
6. Delete authorization but retain outbox where foreign keys permit sequencing/offline edits.
7. Change acknowledgement state.
8. Recompute all row-local hashes after tampering.
9. Crash at every injected point, reopen, then call `snapshot`, `commit` replay, `next_pending`, and `deliver_next`.

The store is correct only if every operation first proves:

```text
one decoded authorization
<-> one exact bundle
<-> one exact receipt
<-> exact replay bindings
<-> exact complete outbox set
```

Check both directions: no missing rows and no extra rows.

## Pass G — delivery identity parity

For one authorized transition containing at least one outbox entry:

1. Commit through `AuthorizedShellState` reference semantics.
2. Commit through SQLite.
3. Extract every delivery ID.
4. Require byte equality.
5. Simulate destination state created under one shell and fail over to the other.
6. Require no duplicate delivery.

Trace the complete preimage and domain. Detect candidate-ID versus authorization-ID substitution. Confirm or refute #73.

## Pass H — composition and footprint completeness

For every component, hide one operation behind:

- rare command variant;
- maximum/minimum numeric boundary;
- map-key branch;
- ordinary rejection path;
- committed-failure path;
- effect-only path;
- outbox-only path.

Show whether observed footprint is always covered by the declared static footprint.

Then determine whether production authorization requires:

- complete-footprint witness;
- verified composition report;
- exact parallel context;
- parity evidence;
- exact fixed-domain execution result.

If ordinary `CatalogCommitAuthority` can authorize a composed component without these values, composition remains optional evidence rather than construction authority.

## Pass I — competing law protocols

Compare PR #65 `zeno-fcis-laws` with PR #66 `zeno-fcis-cbc`.

Check:

- exact invocation binding;
- profile/catalog/policy binding;
- command/context source;
- decision reconstruction;
- checker/verifier identity;
- coverage semantics;
- production authority integration;
- duplicate claim registries/domain tags/versioning.

Do not recommend merging both independently. Identify the strongest pieces to retain in one normative protocol.

Specific check: in PR #66, determine whether `LawSubject::from_transition` validates against externally expected command/context or copies them from the artifact.

## Pass J — resources and termination

Instrument a transition that performs expensive logical work but supplies `BudgetUsed::zero()`.

Determine whether:

- generated APIs force every operation through a meter;
- the authority detects omitted charges;
- recursion/iteration can exceed intended bounds without reflected usage;
- runtime wall-clock or memory exhaustion is incorrectly interpreted as semantic evidence.

Separate safety from availability, but report any value-moving path that can be forced into inconsistent partial behavior.

## Pass K — chain backends

For Solidity and Solana generated outputs, check whether the integrated generic authorization/law/composition identities are reproduced exactly.

Solidity mutations:

- fee-on-transfer;
- rebase;
- proxy implementation change;
- callback/reentrancy;
- token address/codehash/config change;
- successful call with wrong balance delta.

Solana mutations:

- Token-2022 transfer fee;
- transfer hook;
- permanent delegate;
- wrong mint/program/vault;
- upgrade authority/program-data change;
- same accounts with changed extension configuration.

Require state/effect conservation against actual asset deltas, not merely successful external calls.

## Required final classification

Return one table:

| Claim | Proven by construction | Enforced under trusted component | Tested only | Open |
|---|---:|---:|---:|---:|

At minimum classify:

- raw bundle non-authority;
- exact command/context;
- authentic principal;
- replay derivation;
- genesis invariant;
- approved hash provider;
- exact transition implementation;
- exact law engine/verifier;
- complete economic law set;
- conservation;
- SQLite exactness;
- delivery idempotency;
- composition footprint completeness;
- sequential/parallel parity;
- exhaustive runtime refinement;
- authenticated projector/proof;
- Solidity deployment;
- Solana deployment.

A green build or nonzero commitment is not sufficient to place a claim in “proven by construction.”
