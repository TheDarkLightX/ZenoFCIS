# ZenoFCIS feature matrix

This matrix covers features on the `zeno-fcis` umbrella crate. The current
workspace version is `0.1.0`; “core” means project-neutral architecture, not a
stable Cargo V1 API.

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
| no optional feature | `no_std + alloc` | Core | Core, value, codec, project, patch, plan, receipt, pure shell, composition, and refinement primitives |
| `rustcrypto-sha256` | `no_std + alloc` | Core provider | Pinned RustCrypto SHA-256 provider |
| `verified-sha256` | `no_std + alloc` | Core provider | Independent libcrux SHA-256 provider |
| `sha256-parity` | `no_std + alloc` | Assurance | Both providers and parity evidence |

Production authorization currently selects the sealed approved provider path;
a third-party `CommitmentHasher` remains suitable for reference and research
APIs only.

## Project construction

| Feature | Environment | Class | Enables |
|---|---|---|---|
| `schema` | `no_std + alloc` | Core | Closed schemas and schema-admitted values |
| `catalog` | `no_std + alloc` | Core | Schema plus project reasons, effects, channels, authority rules, and limits |
| `transition` | `no_std + alloc` | Core | Catalog-aware transitions and complete transition validation |
| `laws` | `no_std + alloc` | Core assurance | Project law manifests, evidence subjects, and runtime law evaluation |
| `authority` | `no_std + alloc` | Core authority | Transition + laws + approved provider + nominal commit authorization |
| `domain-machines` | `no_std + alloc` | Project architecture | Fixed typed domain-machine matrices and canonical sequential execution |
| `composed-program` | `no_std + alloc` | Project architecture | Authority + domain machines + root projection and one composed transition program |

For a multi-domain application, `composed-program` is the recommended starting
feature. For a single generated transition, `authority` is sufficient.

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
| `collections` | Host `std` through umbrella | Reference/optimization | Backend-neutral persistent collections |
| `persistent-collections` | Host `std` | Reference/optimization | `collections` plus `rpds` and `imbl` backends |
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

## Common selections

```toml
# Project-neutral semantic values and reference semantics
zeno-fcis = { version = "=0.1.0", default-features = false }

# Single-domain, law-aware authorized transitions
zeno-fcis = { version = "=0.1.0", default-features = false, features = ["authority"] }

# Multi-domain deterministic composition
zeno-fcis = { version = "=0.1.0", default-features = false, features = ["composed-program"] }

# Host-side starter generation
zeno-fcis = { version = "=0.1.0", features = ["bootstrap"] }

# Concrete local authorized persistence
zeno-fcis = { version = "=0.1.0", features = ["sqlite-shell"] }

# Tool-neutral checked backend protocol
zeno-fcis = { version = "=0.1.0", default-features = false, features = ["backend"] }
```

## Deterministic-parallel status

The composition APIs represent footprints, conflicts, commutativity evidence,
and sequential-versus-composed parity. The fixed domain executor and composed
program execute the canonical merge order sequentially.

Current claim:

> ZenoFCIS supplies proof-carrying deterministic-parallel planning and
> promotion surfaces with a canonical sequential oracle.

Current nonclaim:

> ZenoFCIS does not ship a concurrent scheduler, threaded shell, or production
> parallel runtime.

Production parallel promotion uses `CompleteFootprintWitness` for every
component and independently checked equality with the canonical sequential
result. Projects must still supply and qualify the concrete proof artifacts and
verifier used to mint those witnesses.
