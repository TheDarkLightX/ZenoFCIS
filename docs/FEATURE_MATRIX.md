# ZenoFCIS feature matrix

This matrix covers the current `zeno-fcis` umbrella source API. The workspace
declares `2.6.0`, the current release source, and the breaking V2 API below
needs a pinned source dependency on the matching tag, as the
[installation guide](INSTALLATION.md) describes. The historical `1.1.0`
package and its API stability promise have their own scope. The earlier public
[2.2.0-rc.1 and 2.3.0-rc.1 candidates](RELEASE_CANDIDATES.md) advertised only
the Linux x86_64 GNU CLI, which needs glibc 2.39 or later; crates.io and other
host binaries were not claimed for them.

The normal application API is `zeno_fcis::program`, which needs no optional
features. It binds complete declarations to the library-owned evaluator and to
private publication capabilities. Historical reference surfaces live under
`zeno_fcis::legacy`. Enabling a dependency feature does not restore a retired
native callback API.

## Host platforms

The 2.6.0 release workflow builds two required host executables:

- Linux x86_64 GNU, on Ubuntu 24.04;
- macOS on Apple silicon (`aarch64-apple-darwin`, minimum-version field
  macOS 11.0), on GitHub's macOS 15 arm64 image.

Its release-set job requires both host jobs and their installed-binary
records. The table lists the configured workflow checks. Only a completed run
bound to the release source and artifact qualifies a host executable.

| Workflow check | Linux x86_64 | macOS arm64 |
|---|---|---|
| Build, archive and custody check | yes | yes |
| Installed-binary journey | yes | yes |
| Clippy and tests | yes | portable tests only |
| Contained checker runtime | yes | unavailable (exit 2) |
| `synth verify` execution | yes | blocked (exit 2) |
| ATDD, packaged-workspace verification, Verus, Miri, formal tools, Tau kit, OCaml profile | yes | no |

Linux evidence is not claimed for macOS. Intel Macs, Windows and other hosts
have no executable and are not qualified. See
[supported hosts](INSTALLATION.md#supported-hosts).

## Environment labels

- **`no_std + alloc`**: supported by the umbrella feature path with default
  features disabled.
- **Host `std`**: deliberately uses or requires the standard library.
- **Mixed**: semantic values are portable, but concrete providers or adapters
  may require a host.

## Base and cryptography

| Feature | Environment | Class | Enables |
|---|---|---|---|
| default (`std`) | Host `std` | Convenience | Standard-library support for the base project-neutral exports |
| no optional feature | `no_std + alloc` | Core | Checked program declarations, catalog/program binding, evaluation and publication; historical reference primitives under `legacy` |
| `rustcrypto-sha256` | `no_std + alloc` | Core provider | Pinned RustCrypto SHA-256 provider |
| `verified-sha256` | `no_std + alloc` | Core provider | Independent libcrux SHA-256 provider |
| `sha256-parity` | `no_std + alloc` | Assurance | Both providers and parity evidence |

V2 program binding derives the library identity. Applications cannot supply a
hasher or a decision callback to obtain publication authority. The
cryptographic provider features also serve the historical reference APIs.

## Project construction

| Feature | Environment | Class | Enables |
|---|---|---|---|
| `authoring` | `no_std + alloc` | Specification | Bounded `.zeno` compiler, canonical `ProjectSpec`, relational/temporal evaluator, builders, composition derivation, and Mini Determinator |
| `schema` | `no_std + alloc` | Core | Closed schemas and schema-admitted values |
| `catalog` | `no_std + alloc` | Core | Schema plus project reasons, effects, channels, authority rules, and limits |
| `transition` | `no_std + alloc` | Historical | Native transition implementations are retired; normal transitions use `program` |
| `laws` | `no_std + alloc` | Metadata | Law declarations and evidence subjects; the checked evaluator is reached through `program::law` |
| `authority` | `no_std + alloc` | Core authority | Library-owned V2 authority and publication re-exports |
| `domain-machines` | `no_std + alloc` | Historical | Native domain-machine implementations are retired |
| `composed-program` | `no_std + alloc` | Historical | Native composed-program implementations are retired |

Start both single-domain and multi-domain applications with complete
declarations through `zeno_fcis::program`. V2.2 has three finite component
families; V2.3 expands the [catalogue](../core-components/README.md) to ten
families, 77 declared instances and 107,518 raw inputs. Stored references are
Identified; successful exhaustive replay supports Proved for the closed finite
scope under the [named trusted base](../core-components/FINITE_PROOFS.md).
Six closed coupled application profiles are in `core-apps/`; their combined
qualification and composition obligations are pending. The retired crates do
not supply a composed runtime or discharge those obligations.

## Tooling and mounted runtimes

| Feature | Environment | Class | Enables |
|---|---|---|---|
| `codegen` | Host tooling | Tooling | Inspectable Rust/Python schema generation and vectors |
| `bootstrap` | Host `std` | Tooling | Catalog-bound starter package generation |
| `mounted-runtime` | Host `std` | Shell/assurance | Callable and strict JSON-line runtime adapters |
| `zenodex-profile` | `no_std + alloc` | Project-specific | Initial ZenoDEX zUSD profile |
| `mounted-zenodex` | Host `std` | Project-specific shell | ZenoDEX profile plus concrete Python/Rust mount |

Generation keeps source and documentation in step with reviewed inputs. It is
not a proof or a grant of authority. Mounted comparison establishes only the
stated bounded refinement evidence.

## Formal and search integrations

| Feature | Environment | Class | Enables |
|---|---|---|---|
| `evidence` | Host `std` through umbrella | Formal evidence | Canonical evidence import and promotion gates |
| `synthesis` | `no_std + alloc` | Formal/search kernel | Complete-within-bounds canonical candidate enumeration |
| `backend` | `no_std + alloc` protocol | Formal adapter | Synthesis plus checked engine/verifier requests, responses, and certificates |

Public Lean, SMT/Z3, CVC5, Kani, Flux, or other adapters can implement the
backend protocol. Users with a private checker can implement it in a private
crate. No backend decides release or commit authority by itself.

Checked transform and optimization commands operate within their declared finite
domains. Search proposals do not grant publication authority.

## Reference data structures and shells

| Feature | Environment | Class | Enables |
|---|---|---|---|
| `authenticated-state` | Host `std` through umbrella | Reference | Configured projector-bound planning and context-verified sparse-proof witnesses |
| `authenticated-authority` | Host `std` | Core authority adapter | Authenticated state + semantic authority + retained projector qualification + nominal candidate-bound publication |
| `collections` | Host `std` through umbrella | Reference/optimization | Backend-neutral persistent collections |
| `persistent-collections` | Host `std` | Reference/optimization | `collections` plus `rpds` and `OrderedMap` backends |
| `sqlite-shell` | Host `std` | Concrete shell | Authorized crash-atomic SQLite publication and delivery |

The pure reference shell is part of the base exports and needs no feature.
The reference shell's acceptance of a raw `CommitBundle` is not a production
authorization path.

## Security surfaces

| Feature | Environment | Class | Enables |
|---|---|---|---|
| `secret` | `no_std + alloc` | Security support | Zeroizing secret containers and explicit exposure authority |
| `security` | `no_std + alloc` | Security assurance | Information-flow, leakage, channel-capacity, and deployment-evidence policies |

These features encode reviewed rules and evidence. They do not prove compiled
code constant-time or eliminate physical side/covert channels.

## Aggregate feature

| Feature | Environment | Class | Enables |
|---|---|---|---|
| `full` | Host `std` | Development/integration | All major generic, reference, ZenoDEX, backend, and persistent-collection surfaces |

`full` is useful for workspace CI and exploration. Reusable libraries should
select explicit features, which keeps their dependencies, trusted computing
base, and API surface small.

The repository's BDD/ATDD and Probity configuration are development and
acceptance tooling, not Cargo features. They do not enter any crate dependency
graph or production authority.

## Common selections

These are alternative dependency declarations against a checkout of
`v2.6.0` at `../ZenoFCIS`. Select the one declaration whose features your
application needs. `version = "=1.1.0"` does not supply the current API.

```toml
# Project-neutral semantic values and reference semantics
zeno-fcis = { path = "../ZenoFCIS/crates/zeno-fcis", default-features = false }

# Normal checked application declarations, binding and evaluation
zeno-fcis = { path = "../ZenoFCIS/crates/zeno-fcis", default-features = false }

# Host-side starter generation
zeno-fcis = { path = "../ZenoFCIS/crates/zeno-fcis", features = ["bootstrap"] }

# Concrete local authorized persistence
zeno-fcis = { path = "../ZenoFCIS/crates/zeno-fcis", features = ["sqlite-shell"] }

# Candidate-bound authenticated index publication
zeno-fcis = { path = "../ZenoFCIS/crates/zeno-fcis", features = ["authenticated-authority"] }

# Tool-neutral checked backend protocol
zeno-fcis = { path = "../ZenoFCIS/crates/zeno-fcis", default-features = false, features = ["backend"] }

# Pure specification compiler and Mini Determinator
zeno-fcis = { path = "../ZenoFCIS/crates/zeno-fcis", default-features = false, features = ["authoring"] }
```

## Deterministic-parallel status

The historical composition evidence API represents footprints, conflicts,
commutativity and sequential-versus-composed parity. Its external evidence
verifier is a named trust boundary. Native domain and composed-program execution
are retired; the normal route evaluates one complete checked program.

ZenoFCIS does not yet ship a production parallel component runtime. The 2.6.0
source includes an offline parallel exhaustive checker whose qualification is
bound to its release-source checks. That checker is separate from parallel
application execution. Component composition needs its own coupled
invariants, binding and atomic publication evidence.
