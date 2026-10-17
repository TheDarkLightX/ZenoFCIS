# API reference

This guide documents the `2.4.0` source. Its retained V1
protocol guidance is historical; see [the release notes](RELEASE_NOTES.md)
for the current checked API and assurance boundaries.

The 2.4.0 source uses the checked program family by default. `zeno_fcis::Program` is the existing private Authority; `bind_catalog` and `bind_program` perform actual admission. Original envelopes enter `Program::publish`/`publish_genesis`; only successful library evaluation returns a private publication capability. See [migration](V2_PROGRAM_API_MIGRATION.md) for declaration modules, refusal/replay behavior and remaining lower-level obligations.

| Normal goal | Entry point |
|---|---|
| Complete original schema and declarative behavior | `program::{schema, scalar, declaration, law}`, `ProgramDefinition` |
| Checked original policy/catalog and library identity | `bind_catalog`, `bind_program`, `Program` |
| Original invocation and private publication | `Invocation`, `PublicationOutcome` |
| Actual usage and immutable audit | `Evaluation`, `Resource`, `Usage` |
| Capability-consuming SQLite boundary | `zeno_fcis::sqlite` with feature `sqlite-shell` |

The `spec` frontend remains authoring input and still requires checked program
admission. `zeno_fcis::legacy` retains inert data and standalone evidence
utilities; the retired native transition and authorization constructors are
not available through that namespace.

## Hosted reference

The historical 1.1.0 API reference is available at:

```text
https://docs.rs/zeno-fcis/1.1.0/zeno_fcis/
```

That reference does not document the breaking V2 API. For the current
signatures, generate Rustdoc from your exact checkout as shown below.

## Local reference

```bash
RUSTDOCFLAGS='-D warnings' cargo +1.97.1 doc \
  --workspace --all-features --locked --no-deps --open
```

The release bundle also contains a static Rustdoc archive generated from the
exact release commit.

## Recommended entry points

For V2 application execution, use the normal entry points in the first table.
The `.zeno` parser and elaborator are exposed through `zeno_fcis::spec` with
feature `authoring`; the CLI is the `zeno-fcis-cli` package. Canonical values,
codecs, catalogs and standalone evidence utilities remain available under the
applicable `legacy` feature namespaces. Their data cannot substitute for a
private library publication.

For machine discovery of the CLI, use `zeno-fcis describe` or
`zeno-fcis describe generate`. The [agent guide](LLM_USAGE.md) describes the
versioned JSON workflow and recovery by exit class.

Start your first implementation with the [quickstart](QUICKSTART.md), then use
the [crate map](CRATE_MAP.md) and generated Rustdoc for exact signatures.

Foundational budget, value, codec, plan, and patch errors implement
`core::error::Error`, including when their crates build without `std`. A hosted
application can propagate them with `?` into a standard boxed error, while the
concrete error type stays available for downcasting. Error variants and
display messages are unchanged.

## Boundary guides

Historical guides describe earlier protocols and assurance work. They are not
instructions to restore removed callback authority or authenticated commit
constructors. The [migration guide](V2_PROGRAM_API_MIGRATION.md) describes the
supported V2 route.

- The [canonical-bytes guide](CANONICAL_BYTES.md) explains ZCVE/1 admission,
  decode/re-encode enforcement, commitments, and the boundary between byte
  identity and semantic authority.
- The [V1 product contract](V1_PRODUCT_CONTRACT.md) identifies the supported
  adopter journeys, and the [acceptance guide](ACCEPTANCE_TESTING.md) maps each
  journey to fixed executable commands.
- The [genesis authorization guide](GENESIS_AUTHORIZATION.md) documents the
  required one-time initial-state ceremony and the SQLite reopen contract.
- The [strict artifact and SQLite history guide](STRICT_ARTIFACT_AND_SQLITE_HISTORY.md)
  documents persisted-artifact reauthorization and complete row-set validation.
- The [validated refinement guide](VALIDATED_REFINEMENT_AND_EXHAUSTIVE_COVERAGE.md)
  documents the separation between untrusted mounted transport, strict artifact
  reconstruction, canonical domain manifests, and independently verified
  promotion evidence.
- The [authenticated authority guide](AUTHENTICATED_AUTHORITY_BOUNDARY.md)
  documents retained projector qualification, per-transition projection laws,
  strict plan reauthorization, and nominal authenticated publication.

## Stability

The V2 API, introduced in 2.3.0 and retained in 2.4.0, intentionally changes the
1.1.0 execution API. The earlier public V2 candidates were prereleases.
Protocol identifiers remain versioned. SQLite stores use schema 10. A schema-9
store opens only for an explicit migration after a complete audit, and older
stores are refused rather than silently migrated. Final application, proof, CI
and release checks have the limits stated in the
[2.4.0 release notes](RELEASE_NOTES.md).
