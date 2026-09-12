# ZenoFCIS 1.0.0

ZenoFCIS 1.0.0 establishes the stable Cargo API for the 36-crate library family.
It keeps the functional core pure and requires nominal authorization before
the production-facing shell can publish a transition. The release continues
to use Rust 1.97.1, Node 22.23.1, and the existing Lean 4.30.0 integration.

## What developers gain over the published RC3

- Describe a finite contract, search a bounded instruction language, and
  independently check the selected implementation. The CLI can emit Rust,
  Python, or JavaScript and run the target conformance check. A new language
  integrates through the target interface and needs its own conformance
  implementation; arbitrary languages are not already qualified.
- Generate a runnable Rust application with the `durable-counter` template.
  Its consumer checks exercise project laws, nominal authorization, SQLite
  persistence, restart, replay, and idempotent notification delivery.
- Let a coding agent discover commands with `zeno-fcis describe`, consume
  versioned JSON diagnostics, and check generated-file drift without writing
  files. Input reads and artifact comparisons have explicit bounds.
- Qualify the generated application using the CLI and dependencies extracted
  from actual `.crate` archives. The release retains
  `PACKAGED-APPLICATION.json` alongside source, package and checker identities.
- Use the standard error trait on foundational error types and select a
  smaller dependency surface for `no_std + alloc` consumers.

The bounded synthesis protocol existed before this release. The concrete
finite-contract workflow and language target implementations extend it.

```bash
cargo +1.97.1 install zeno-fcis-cli --version 1.0.0 --locked
zeno-fcis new counter --template durable-counter
zeno-fcis describe synth
zeno-fcis check counter/project.zeno --format json
```

See the [generated application journey](GENERATED_APPLICATION_MILESTONE.md)
and [synthesis and target conformance guide](LANGUAGE_NEUTRAL_SYNTHESIS.md)
for the complete execution and verification commands.

## Applications and workflows this makes easier

| Application | Practical workflow |
| --- | --- |
| Agent-operated task or approval service | Define permitted state transitions, let an agent propose an implementation, and check the finite contract before mounting it behind an authorized shell. The host still controls credentials and external actions. |
| Auditable workflow service | Generate a durable starting application, supply the project's laws, then exercise restart, replay and idempotent notifications before replacing the example transport. |
| Policy editor serving several runtimes | Express one bounded contract, generate Rust, Python or JavaScript, and replay the complete declared input domain for each selected target. |
| CI for generated application changes | Regenerate without overwriting files, report drift through versioned JSON, and verify an application built from the same package contents users will install. |

These applications were possible with hand-written integration before V1.
The new workflows reduce that integration work and retain explicit evidence
for the declared checks; they do not infer missing requirements.

## Correctness and performance work

The release fixes nested temporal variable capture in the Lean exporter and
adds a relational and temporal translation corpus that can use an existing
Lean executable. It also makes CLI acquisition failures and timeouts explicit,
preserves SQLite diagnostics, and checks complete durable history at public
storage boundaries. A pending delivery now checks membership in its approved
bundle directly, closing a row-substitution race between SQLite reads.

Python adapters reject wrong primitive types instead of silently converting
them; malformed composite values, inconsistent map keys and generated name
collisions also fail explicitly. Synthesis runner directories are private from creation and source files
cannot overwrite pre-existing files or symlink targets. The QEMU runner avoids
a nested Cargo build lock deadlock; the retained V1 capture comes from a real
boot. The developer-tool Hono dependency is patched to 4.13.5; Probity stays
pinned at 1.10.0.

Canonical decoding, commitment hashing, finite evaluation, outbox encoding,
and parallel-authorization ordering avoid redundant traversal or allocation.
Retained differential and mutation checks cover the changed behavior.
Measurements are workload-specific; the release makes no universal speedup,
constant-time, SIMD, or hardware qualification claim. See
[design improvements](DESIGN_IMPROVEMENTS.md) for the evidence and limits.

## Compatibility

All internal Cargo dependencies move together to exact version `1.0.0`.
Existing canonical protocol identifiers, encodings and authorization formats
keep their own versions. A Cargo version change does not migrate stored data.
Generated source headers and source-bound checker identities can change;
regenerate and recheck the corresponding artifacts. The generated durable
counter binds its Cargo manifest into its example policy identity, so
regenerating it at 1.0.0 requires a new example database unless the adopter
provides a reviewed migration.

The public `ReplayFixture` name remains an alias for `DecisionMismatchRecord`.
The former `imbl` map and feature names remain compatible aliases for
`OrderedMap`, implemented with the existing pinned `rpds` dependency. The
unsound `bitmaps` dependency has been removed. Repository examples now live
under `test-projects` and `test-data`.

## Assurance boundary

Synthesis checks the declared finite domain and selected target. It cannot
establish missing requirements or correctness of arbitrary external code.
Applications still supply reviewed policies, laws, authenticated inputs,
deployment bindings and effect interpreters. The SQLite shell and Mini
Determinator are reference integrations, with their documented limits.

Stable Cargo APIs do not establish a mechanized end-to-end theorem for every
project, production deployment qualification, or an independent security
audit. See the [product contract](V1_PRODUCT_CONTRACT.md),
[security policy](../SECURITY.md), [release assurance](RELEASE_ASSURANCE.md),
and [release checklist](V1_RELEASE_CHECKLIST.md), and
[implementation review](V1_REVIEW.md).
