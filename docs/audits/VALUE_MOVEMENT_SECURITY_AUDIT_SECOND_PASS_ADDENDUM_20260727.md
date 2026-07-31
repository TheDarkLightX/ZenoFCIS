# Value-movement audit second-pass addendum

**Base audit:** `VALUE_MOVEMENT_SECURITY_AUDIT_20260727.md`  
**Revision:** `47c3b659dda8dbd37f3294d090554cb3b2493bbb`

The second pass changed perspective from transition construction to **promotion authority**: can evidence values cause a runtime or implementation to be treated as equivalent to the model without establishing the complete claim?

## VM-017 — P1: exhaustive refinement coverage is cardinality-only

### Location

`crates/zeno-fcis-refine/src/lib.rs`:

- `CoverageMode::Exhaustive`;
- `PromotionEvidence::try_new`;
- `evaluate_promotion`.

### Finding

Exhaustive promotion checks that the declared cardinality equals the number of retained cases. It does not verify that:

- input hashes are unique;
- the retained inputs are exactly the members of the committed domain;
- `domain_hash` is derived from the retained input set or an independently checked domain definition;
- the enumeration algorithm and ordering are pinned;
- every domain member occurs exactly once.

`PromotionEvidence::try_new` rejects duplicate **case IDs**, not duplicate **input hashes**.

### Exploit

Repeat one valid input under several distinct case IDs, set the claimed cardinality to the number of repetitions, choose an arbitrary domain hash, and submit exact model/runtime outputs for that one input. The exhaustive cardinality check can pass although most of the claimed domain was never tested.

### Blast radius

False “complete domain” promotion for runtime equivalence, including value-moving code paths omitted from the retained cases.

### Required repair

Introduce a canonical, independently verified `ExhaustiveDomainManifest` containing:

- exact profile/schema/command/context domain identity;
- domain-definition commitment;
- enumeration algorithm and toolchain;
- canonical input-set root or complete bounded set;
- exact cardinality;
- explicit empty-domain policy.

Bind every case ID to its input and reject duplicate input hashes. Verify set equality between retained cases and the manifest, rather than comparing counts.

## VM-018 — P1: raw normalized decisions can compare exact without semantic reconstruction

### Location

`crates/zeno-fcis-refine/src/lib.rs`:

- public-field `DecisionArtifacts`;
- `NormalizedDecision::try_new`;
- `compare_exact`.

### Finding

`NormalizedDecision::try_new` validates reason shape, artifact-size bounds, unchanged rejection roots, and presence/absence of committed artifact fields. It does not decode or reconstruct:

- patch;
- commit/outbox plans;
- receipt;
- complete bundle;
- candidate ID;
- root/binding relationships;
- catalog or invocation authorization.

A caller can therefore construct two identical, mutually inconsistent normalized decisions. `compare_exact` correctly reports them equal because its job is field comparison, but a promotion path that treats `NormalizedDecision` as authoritative can promote fabricated equivalence.

### Exploit

Populate model and runtime `DecisionArtifacts` with the same arbitrary candidate ID and byte strings that do not form a valid candidate. Give them distinct valid case IDs and claimed inputs. Exact comparison succeeds; combined with VM-017, the case set can be presented as exhaustive.

### Required repair

Split the API:

```text
UntrustedDecisionArtifacts
    -> strict bounded decode
    -> exact receipt/bundle reconstruction
    -> catalog + invocation + provider + domain validation
    -> ValidatedNormalizedDecision
```

Only the validated nominal type may enter production promotion evidence. Keep a raw comparison type for diagnostics, but it must not confer promotion authority.

## VM-019 — P2: promotion policy/report identity is incomplete for durable authorization

`PromotionReport` contains only blockers and is not itself content-addressed to the exact policy, evidence, source revision, importer, verifier implementation, and deployment profile. A consumer must retain those values externally to know what was promoted.

Before production promotion, construct a canonical report identity over all inputs and require an approved importer/verifier witness. A bare `is_promotable()` boolean must never be persisted or transported as sufficient authority.

## Additional adversarial tests

1. duplicate input hashes with distinct case IDs;
2. arbitrary domain hash with matching case count;
3. same cases with changed enumeration algorithm/toolchain;
4. zero cases and cardinality zero under a nonempty profile domain;
5. identical fabricated model/runtime bundle bytes;
6. identical mutations to candidate ID, roots, patch, plans, receipt, and bundle;
7. valid normalized artifact from another invocation/profile;
8. promotion report replay under another policy/verifier/source revision.

## Closure tracking

GitHub issue: #61.
