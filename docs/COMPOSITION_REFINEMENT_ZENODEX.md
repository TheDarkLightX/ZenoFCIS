# Composition, mounted refinement, and the ZenoDEX profile

This layer adds three boundaries that are required before ZenoFCIS can support
large ZenoDEX transitions safely.

## Composition contracts

Each component declares exact read, write, context, and effect paths. Paths are
hierarchical and support a terminal descendant wildcard. Default deterministic
parallel admission rejects write/write and either-direction read/write overlap.

Assumptions, guarantees, frame rules, wiring, and coupling theorems are immutable
values. Evidence hashes do not prove claims by themselves: an external pinned
`EvidenceVerifier` must validate each artifact.

## Mounted refinement

A runtime result is normalized into one exact authority surface:

```text
decision kind and stable reason
profile / command / context / precedence / algorithm / budget bindings
pre-root and post-root
candidate identity
patch bytes
commit-plan bytes
outbox-plan bytes
receipt bytes
complete bundle bytes
```

Promotion compares every field. State-root equality alone is insufficient.
Exhaustive, bounded, and proof-assisted coverage are distinguished explicitly.

## Initial ZenoDEX profile

The first profile records:

- the current 32-field single-vault zUSD state shape;
- hard Oracle, accounting, debt-cap, debt-floor, collateral-ratio, and fee bounds;
- the current 45-code Rust zUSD rejection registry in explicit precedence order;
- the current command tags, marking caller-driven epoch advancement as legacy and
  non-authoritative;
- a high-assurance promotion policy requiring independent solver, theorem,
  bounded-model, translation, codec-vector, and mounted-runtime evidence.

This does not claim the mounted ZenoDEX runtime already implements the profile.
A later adapter must construct complete native ZenoFCIS decisions and demonstrate
exact refinement against the current Rust/Python/formal transitions.
