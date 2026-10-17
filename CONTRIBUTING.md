# Contributing to ZenoFCIS

ZenoFCIS accepts small, bounded changes that preserve its functional-core /
imperative-shell architecture and its explicit protocol identities.

## Architecture rule

```text
bytes -> bounded canonical values -> pure transition -> immutable decision
      -> nominal authorization -> atomic imperative interpreter
```

Semantic crates use `#![forbid(unsafe_code)]`. They contain no ambient clock,
randomness, filesystem, network, database, process, thread, async runtime,
global mutable state, or executable effect closure. Operational mechanisms
belong in an explicit outer adapter or shell.

## Development setup

Use pinned Rust `1.97.1` and the committed `Cargo.lock`. Direct external
dependencies need exact `=version` pins. The optional coding-agent guardrails
use exact Node `22.23.1`, Probity `1.10.0`, and the committed
`package-lock.json`:

```bash
npm ci --ignore-scripts
python3 tools/check_probity.py
```

## Checks to run

Before you submit an ordinary change, run the checks its scope calls for:

- Documentation changes need the documentation guard, checks of the affected
  links, and `git diff --check`.
- Code and tooling changes need the relevant tests, formatting, linting, and
  any affected proof or source-identity checks.

Record the checks you ran and their limits. A full acceptance run is not a
prerequisite for every commit or push.

When you prepare a new version for release, including a release candidate, run
the complete suite on the exact release source and inspect every result:

```bash
python3 tools/check_assurance.py --self-test
python3 tools/check_assurance.py
python3 tools/check_library_docs.py
python3 tools/atdd.py self-test
python3 tools/atdd.py check
cargo +1.97.1 fmt --all -- --check
cargo +1.97.1 clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +1.97.1 test --workspace --all-features --locked
cargo +1.97.1 test --workspace --doc --all-features --locked
RUSTDOCFLAGS='-D warnings' cargo +1.97.1 doc --workspace --all-features --locked --no-deps
python3 tools/atdd.py run --all
```

Changes to packaging or to a public crate must also run:

```bash
python3 tools/rc_package.py self-test
python3 tools/rc_package.py check
```

Permanent CI adds `no_std`, Miri, fuzz-build, kernel-laws, supply-chain,
source-manifest, mounted-runtime, persistence, adopter-acceptance,
deterministic developer guardrails, and assurance-specific gates. See
[`docs/ACCEPTANCE_TESTING.md`](docs/ACCEPTANCE_TESTING.md) and
[`docs/DEVELOPER_GUARDRAILS.md`](docs/DEVELOPER_GUARDRAILS.md).

## Protocol-visible changes

Protocol-visible behavior requires:

- a stable schema and version;
- canonical ordering and encoding;
- typed rejection or committed-failure semantics;
- explicit deterministic resource bounds;
- complete candidate, invocation, profile, law, and deployment binding where
  the value can reach authority;
- positive, exact-boundary, negative, and metamorphic or differential tests;
- an honest list of assumptions and nonclaims.

Do not renumber or reinterpret a stable field, variant, reason, effect,
channel, domain, codec, schema, precedence, profile, or algorithm identifier.
A semantic change under an existing Rust API still requires a new protocol
identity and reviewed migration evidence.

## Pull requests

Keep one conceptual change per PR. Describe:

- the exact base and head commits;
- public API and dependency changes;
- the authority boundary and resource bounds;
- laws, negative cases, and the validation performed;
- assumptions, nonclaims, and unresolved blockers.

Do not commit generated caches, target directories, diagnostic archives,
temporary assembly scripts, write-enabled workflows, credentials, or retained
CI payload fragments. Generated source that is part of the public contract must
have a deterministic generator, a retained manifest, and a compile or replay
gate.

CODEOWNERS declares repository review ownership. Repository settings must
separately enforce protected branches and required approvals. A passing CI run
does not replace independent review of authority-bearing or protocol changes.

## Privacy checks for commits and evidence

Before you commit or publish evidence, run
`python3 tools/check_release_privacy.py check`. For release attachments, pass
their complete directory as the final argument. The check inspects nested
archives and reports categories and locations without echoing private data.

For known private identifiers, such as a personal email or workstation name,
set `ZENO_FCIS_PRIVATE_MARKERS_FILE` to a file outside Git and the upload
directory. The file holds one literal identifier per line: each at least four
bytes, with at most 128 identifiers and 64 KiB in total. Packaging checks
inherit this setting. The standalone check also accepts
`--private-markers-file <private-file>`.

Use a public or GitHub noreply identity for public commits. Do not upload raw
GitHub workflow responses without checking their author and committer metadata.
Review logs and metadata as well, because pattern checks cannot detect every
secret. Public evidence copies use neutral paths and document their
redactions, and their hashes must describe the redacted bytes. Keep the
original local records outside Git.

## Security reports

Do not open a public PR for a suspected exploitable vulnerability. Follow the
private process in [SECURITY.md](SECURITY.md).
