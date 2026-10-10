# API reference

This guide is included in the `2.3.0-rc.1` candidate. Its retained V1
protocol guidance does not define the normal V2 authority route; see
[the candidate release notes](RELEASE_NOTES.md).

The V2 development branch uses the checked program family by default. `zeno_fcis::Program` is the existing private Authority; `bind_catalog` and `bind_program` perform actual admission. Original envelopes enter `Program::publish`/`publish_genesis`; only successful library evaluation returns a private publication capability. See [migration](V2_PROGRAM_API_MIGRATION.md) for declaration modules, refusal/replay behavior and remaining lower-level obligations.

| Normal goal | Entry point |
|---|---|
| Complete original schema and declarative behavior | `program::{schema, scalar, declaration, law}`, `ProgramDefinition` |
| Checked original policy/catalog and library identity | `bind_catalog`, `bind_program`, `Program` |
| Original invocation and private publication | `Invocation`, `PublicationOutcome` |
| Actual usage and immutable audit | `Evaluation`, `Resource`, `Usage` |
| Capability-consuming SQLite boundary | `zeno_fcis::sqlite` with feature `sqlite-shell` |

The `spec` frontend remains authoring input and still requires checked program
admission. `zeno_fcis::legacy` retains inert data and standalone evidence
utilities; retired native transition and authorization constructors are not
available through that namespace.

## Hosted reference

The historical 1.1.0 API reference is available at:

```text
https://docs.rs/zeno-fcis/1.1.0/zeno_fcis/
```

That reference does not document this breaking V2 development API. Generate
rustdoc from the exact checkout below for its current signatures.

## Local reference

```bash
RUSTDOCFLAGS='-D warnings' cargo +1.97.1 doc \
  --workspace --all-features --locked --no-deps --open
```

The release bundle also contains a static rustdoc archive generated from the exact
release commit.

## Recommended entry points

For machine discovery of the CLI, use `zeno-fcis describe` or
`zeno-fcis describe generate`. The [agent guide](LLM_USAGE.md) describes the
versioned JSON workflow and recovery by exit class.

For V2 application execution, use the normal entry points in the first table.
The `.zeno` parser and elaborator are exposed through `zeno_fcis::spec` with
feature `authoring`; the CLI is the `zeno-fcis-cli` package. Canonical values,
codecs, catalogs and standalone evidence utilities remain available under the
applicable `legacy` feature namespaces. Their data cannot substitute for a
private library publication.

Historical guides below describe earlier protocols and assurance work. They
are not instructions to restore removed callback authority or authenticated
commit constructors. The [scope ledger](V2_LEDGER_SCOPE.md) and
[migration guide](V2_PROGRAM_API_MIGRATION.md) govern the smaller V2 route.

Prefer the [quickstart](QUICKSTART.md) for the first implementation, then use
the [crate map](CRATE_MAP.md) and generated rustdoc for exact signatures.
Foundational budget, value, codec, plan, and patch errors implement
`core::error::Error`, including when their crates build without `std`. A hosted
application can propagate them with `?` into a standard boxed error while
retaining the concrete error type for downcasting. Error variants and display
messages are unchanged.
The [canonical-bytes guide](CANONICAL_BYTES.md) explains ZCVE/1 admission,
decode/re-encode enforcement, commitments, and the boundary between byte
identity and semantic authority.
The [V1 product contract](V1_PRODUCT_CONTRACT.md) identifies the supported
adopter journeys, and the [acceptance guide](ACCEPTANCE_TESTING.md) maps each
journey to fixed executable commands.
The [genesis authorization guide](GENESIS_AUTHORIZATION.md) documents the
required one-time initial-state ceremony and SQLite reopen contract. The
[strict artifact and SQLite history guide](STRICT_ARTIFACT_AND_SQLITE_HISTORY.md)
documents persisted-artifact reauthorization and complete row-set validation.
The [validated refinement guide](VALIDATED_REFINEMENT_AND_EXHAUSTIVE_COVERAGE.md)
documents the separation between untrusted mounted transport, strict artifact
reconstruction, canonical domain manifests, and independently verified
promotion evidence.
The [authenticated authority guide](AUTHENTICATED_AUTHORITY_BOUNDARY.md)
documents retained projector qualification, per-transition projection laws,
strict plan reauthorization, and nominal authenticated publication.

## Stability

This development branch intentionally changes the 1.1.0 execution API. It has
not been released as V2. Protocol identifiers remain versioned; SQLite schema
9 refuses older stores rather than silently migrating them. Final application,
proof, CI and release checks remain open as recorded in the
[V2 plan](V2_VERIFIED_CORE_PLAN.md).
