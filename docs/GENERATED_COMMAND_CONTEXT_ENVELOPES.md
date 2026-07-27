# Generated command and context envelopes

## Scope

This package closes the generated transition path's remaining raw-hash boundary. The
project bootstrap generator emits nominal command and authenticated-context witnesses,
admits the generated typed values against the exact reviewed schema types, derives their
commitments, and passes only those commitments to `CataloguedTransitionBuilder`.

The generic low-level transition builder remains unchanged. Existing candidate, receipt,
codec, schema, profile, and catalog formats remain unchanged.

## Inputs

Generation receives one reviewed `ProjectCatalog`. It reads, without choosing:

- the command and authenticated-context type identifiers;
- their generated Rust types from the closed schema;
- the exact schema and profile commitments;
- the project domain prefix;
- the selected commitment-provider identity;
- existing catalog and bootstrap resource limits.

At runtime, `GeneratedProject::admit_command` and `admit_context` receive:

- the same selected `CommitmentHasher` used by the catalog;
- only the generated Rust command or context type;
- explicit `ValidationLimits`.

`GeneratedProject::begin_transition` receives the generated command and context witnesses.
It no longer accepts caller-supplied command or context hashes.

## Outputs

Generated source exposes:

- `GeneratedCommandEnvelope` and `GeneratedContextEnvelope` with private fields;
- `GeneratedProject::admit_command` and `admit_context`;
- exact `COMMAND_DOMAIN` and `CONTEXT_DOMAIN` constants derived from the reviewed profile's
  domain prefix;
- `INPUT_COMMITMENT_FORMAT_VERSION = 1`;
- getters for each admitted value, validation report, and derived commitment;
- an updated `begin_transition` that revalidates both witnesses before using their
  commitments.

The schema crate also exposes `SchemaAdmittedTypeEnvelope`, the non-root counterpart to
`SchemaAdmittedEnvelope`. It binds a selected schema type, the exact schema commitment, an
owned structurally admitted value, and the successful validation report into the existing
canonical envelope bytes.

## Commitment construction

The command domain is:

```text
<ProjectProfile.domain_prefix>/command
```

The authenticated-context domain is:

```text
<ProjectProfile.domain_prefix>/context
```

Both use `INPUT_COMMITMENT_FORMAT_VERSION`. The committed payload is the complete canonical
`SchemaAdmittedTypeEnvelope`, whose existing bytes contain the selected type identifier,
schema commitment, and canonical value. The selected provider hashes the ordinary ZenoFCIS
domain preimage. A derived zero commitment is rejected.

These domain names, suffixes, version, envelope bytes, and provider identity are protocol
meaning. Generated source keeps them visible and reviewable.

## Authority boundary

The reviewed schema and `ProjectProfile` retain authority over types, stable identifiers,
the domain prefix, profile version, and profile bindings. The generator copies those values
and fixes the framework-owned `command` and `context` role suffixes. It does not select a
project schema, type identifier, provider, input value, policy, or rejection reason.

The generated command and context witness fields are private and nominally distinct. Safe
callers cannot swap them or pair an admitted value with an invented commitment. Transition
startup rechecks the provider, catalog, root envelope, input schema commitments, input type
identifiers, and derived commitments before entering the generic builder.

Local diagnostic order is:

1. exact generated catalog and provider binding;
2. generated typed conversion;
3. selected-type schema validation;
4. default structural value admission;
5. schema and complete-envelope admission;
6. domain-separated commitment derivation and nonzero check;
7. at transition startup, root, command, and context binding revalidation;
8. generic transition admission.

This order is not application rejection or committed-failure precedence.

## Trusted dependencies

No dependency is added. This package uses the existing:

- generated typed adapters;
- `Schema::validate_value` and `Schema::schema_hash`;
- `AdmittedValue` and `AdmittedEnvelope` boundaries;
- ZCVE canonical envelope encoding;
- `Domain`, `commitment`, and `CommitmentHasher` APIs;
- exact generated catalog reconstruction;
- `CataloguedTransitionBuilder`.

External provider types remain hidden behind `CommitmentHasher`.

## Deterministic resource bounds

- Selected-type validation is bounded by caller-supplied maximum depth and visited nodes.
- The exact successful `ValidationReport` is retained.
- Default structural admission retains its existing depth, node, collection, and aggregate
  payload bounds.
- The complete canonical input envelope remains bounded by the existing 64 MiB decoder-input
  ceiling, including its fixed header.
- Commitment construction allocates at most the bounded canonical envelope plus the bounded
  domain preimage.
- Catalog reconstruction and generated output remain bounded by `CatalogLimits` and
  `BootstrapLimits`.
- All construction is atomic and returns no partial witness.

No ambient clock, randomness, filesystem, network, database, process state, thread, global
mutation, or interior mutability is observed.

## Laws and tests

The implementation checks:

1. A typed witness exists only after validation against the selected schema type.
2. The witness type equals the reviewed command or context type identifier.
3. The witness schema commitment equals the generated catalog schema commitment.
4. The retained validation report is the exact successful report.
5. Repeated admission of the same input is byte- and commitment-identical.
6. Changing an admitted input changes the tested commitment.
7. Command and context commitments use distinct visible domains.
8. The derived commitments are nonzero.
9. Candidate bindings equal the generated witness commitments.
10. Full accepted-decision revalidation still succeeds.

Negative and exact-boundary tests cover:

- a different provider identity;
- typed conversion outside a schema integer bound;
- exhausted validation-node budget;
- a different root schema commitment;
- a different root type after forced test-only schema-hash equality;
- generated-binding failures before transition-input diagnostics;
- source-level absence of raw command/context hash parameters and public witness fields.

## Assumptions

- The reviewed profile's domain prefix is appropriate for its command and context namespace.
- The schema's command and context type identifiers were selected by project authority.
- The selected provider implements its advertised identity and expected cryptographic
  properties.
- Generated output is used at its retained digest or reviewed again after modification.
- Business predicates and authenticated-context provenance are checked outside this
  construction layer.

## Explicit nonclaims

- Schema admission proves no business invariant beyond the selected closed schema type.
- A context witness does not prove that its observations, signatures, timestamps, or evidence
  are authentic.
- Tested commitment inequality is not an unbounded collision-resistance proof.
- This package does not mount a runtime, execute an effect, deliver an outbox entry, or choose
  a state commitment domain.
- It changes no existing stable identifier, schema, precedence position, candidate format,
  codec version, profile format, or authority policy.
- It is not an audit, formal proof, side-channel claim, or production authorization.
