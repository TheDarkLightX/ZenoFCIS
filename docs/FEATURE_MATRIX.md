# ZenoFCIS feature matrix

This matrix covers features on the `zeno-fcis` umbrella crate. The current
workspace version is `1.1.0`; “core” means the implemented
project-neutral library surface. Cargo API stability began at `1.0.0`.

The normal application API is `zeno_fcis::program`, available without optional
features. It binds complete declarations to the library-owned evaluator and
private publication capabilities. Historical reference surfaces live under
`zeno_fcis::legacy`; enabling a dependency feature does not restore a retired
native authoring API.

## Environment labels

- **`no_std + alloc`**: supported by the umbrella feature path with default
  features disabled.
- **Host `std`**: intentionally uses or requires the standard library.
- **Mixed**: semantic values are portable, while concrete providers or
  adapters may require a host.

## Base and cryptography

| Feature | Environment | Class | Enables |
|---|---|---|---|
| default (`std`) | Host `std` | Convenience | Standard-library support for the base project-neutral exports |
| no optional feature | `no_std + alloc` | Core | Checked program declarations, catalog/program binding, evaluation and publication; historical reference primitives under `legacy` |
| `rustcrypto-sha256` | `no_std + alloc` | Core provider | Pinned RustCrypto SHA-256 provider |
| `verified-sha256` | `no_std + alloc` | Core provider | Independent libcrux SHA-256 provider |
| `sha256-parity` | `no_std + alloc` | Assurance | Both providers and parity evidence |

V2 program binding derives the library identity; applications cannot supply a
hasher or decision callback to obtain publication authority. Cryptographic
provider features also serve the historical reference APIs.

## Project construction

| Feature | Environment | Class | Enables |
|---|---|---|---|
| `authoring` | `no_std + alloc` | Authoring | Bounded `.zeno` compiler, canonical `ProjectSpec`, relational/temporal evaluator, builders, composition derivation, and Mini Determinator |
| `schema` | `no_std + alloc` | Core | Closed schemas and schema-admitted values |
| `catalog` | `no_std + alloc` | Core | Schema plus project reasons, effects, channels, authority rules, and limits |
| `transition` | `no_std + alloc` | Historical | Native transition authoring is retired; normal transitions use `program` |
| `laws` | `no_std + alloc` | Metadata | Law declarations and evidence subjects; the checked evaluator is reached through `program::law` |
| `authority` | `no_std + alloc` | Core authority | Library-owned V2 authority and publication reexports |
| `domain-machines` | `no_std + alloc` | Historical | Native domain-machine authoring is retired |
| `composed-program` | `no_std + alloc` | Historical | Native composed-program authoring is retired |

Start both single- and multi-domain applications with complete declarations
through `zeno_fcis::program`. Reusable component composition is planned in
[G1](V2_1_V2_2_PLAN.md) and the [V2.3 roadmap](V2_3_PLAN.md); the retired crates
do not implement it.

## Tooling and mounted runtimes

| Feature | Environment | Class | Enables |
|---|---|---|---|
| `codegen` | Host tooling | Tooling | Inspectable Rust/Python schema generation and vectors |
| `bootstrap` | Host `std` | Tooling | Catalog-bound starter package generation |
| `mounted-runtime` | Host `std` | Shell/assurance | Callable and strict JSON-line runtime adapters |
| `zenodex-profile` | `no_std + alloc` | Project-specific | Initial ZenoDEX zUSD profile |
| `mounted-zenodex` | Host `std` | Project-specific shell | ZenoDEX profile plus concrete Python/Rust mount |

Generation synchronizes source and documentation with reviewed inputs. It is
not a proof or authority grant. Mounted comparison establishes only the stated
bounded refinement evidence.

## Formal and search integrations

| Feature | Environment | Class | Enables |
|---|---|---|---|
| `evidence` | Host `std` through umbrella | Formal evidence | Canonical evidence import and promotion gates |
| `synthesis` | `no_std + alloc` | Formal/search kernel | Complete-within-bounds canonical candidate enumeration |
| `backend` | `no_std + alloc` protocol | Formal adapter | Synthesis plus checked engine/verifier requests, responses, and certificates |

The backend protocol can be implemented by public Lean, SMT/Z3, CVC5, Kani,
Flux, or other adapters. Users with private ESSO access can implement it in a
private crate. No backend decides release or commit authority by itself.

## Reference data structures and shells

| Feature | Environment | Class | Enables |
|---|---|---|---|
| `authenticated-state` | Host `std` through umbrella | Reference | Configured projector-bound planning and context-verified sparse-proof witnesses |
| `authenticated-authority` | Host `std` | Core authority adapter | Authenticated state + semantic authority + retained projector qualification + nominal candidate-bound publication |
| `collections` | Host `std` through umbrella | Reference/optimization | Backend-neutral persistent collections |
| `persistent-collections` | Host `std` | Reference/optimization | `collections` plus `rpds` and `OrderedMap` backends |
| `sqlite-shell` | Host `std` | Concrete shell | Authorized crash-atomic SQLite publication and delivery |

The pure reference shell is part of the base exports and needs no feature. Raw
reference-shell acceptance of `CommitBundle` is not a production authorization
path.

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
select explicit features to keep their dependency, trusted-computing, and API
surface small.

The repository's BDD/ATDD and Probity configuration are development and
acceptance tooling, not Cargo features. They do not enter any crate dependency
graph or production authority.

## Common selections

```toml
# Project-neutral semantic values and reference semantics
zeno-fcis = { version = "=1.1.0", default-features = false }

# Normal checked application declarations, binding and evaluation
zeno-fcis = { version = "=1.1.0", default-features = false }

# Host-side starter generation
zeno-fcis = { version = "=1.1.0", features = ["bootstrap"] }

# Concrete local authorized persistence
zeno-fcis = { version = "=1.1.0", features = ["sqlite-shell"] }

# Candidate-bound authenticated index publication
zeno-fcis = { version = "=1.1.0", features = ["authenticated-authority"] }

# Tool-neutral checked backend protocol
zeno-fcis = { version = "=1.1.0", default-features = false, features = ["backend"] }

# Pure authoring compiler and Mini Determinator
zeno-fcis = { version = "=1.1.0", default-features = false, features = ["authoring"] }
```

## Deterministic-parallel status

The historical composition evidence API represents footprints, conflicts,
commutativity and sequential-versus-composed parity. Its external evidence
verifier is a named trust boundary. Native domain and composed-program execution
are retired; the normal route evaluates one complete checked program.

ZenoFCIS does not yet ship a production parallel component runtime. G10's
planned parallel exhaustive checker is separate from parallel application
execution. Component composition needs its own coupled invariants, binding and
atomic publication evidence.
