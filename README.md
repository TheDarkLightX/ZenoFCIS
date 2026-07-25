# ZenoFCIS

ZenoFCIS is a high-assurance Rust library family for functional-core / imperative-shell systems.

Its primary design rule is:

```text
immutable state + command + policy + authenticated context
    -> pure total transition
    -> Accept | Reject | CommittedFailure
    -> exact immutable candidate
    -> atomic imperative-shell interpretation
```

The semantic kernel treats values, decisions, resource budgets, canonical bytes, and commitments as explicit protocol data. It forbids unsafe Rust and is designed for `no_std + alloc` use without clocks, randomness, networking, filesystems, databases, or executable effect closures.

## Initial workspace

The first pull request contains:

- `zeno-fcis-core`: three-way decisions, stable reason precedence, deterministic budgets, and the transition trait;
- `zeno-fcis-value`: closed transitively immutable values, bounded owners, canonical record/map shape, and structural limits;
- `zeno-fcis-codec`: the initial ZCVE/1 canonical reference codec and a narrow cryptographic-hasher provider interface;
- `zeno-fcis`: umbrella re-exports.

Later pull requests add preconditioned patches, candidate sealing, effect and outbox plans, receipts, atomic commit bundles, composition contracts, authenticated-state adapters, synthesis, formal verification, and ZenoDEX profiles.

## Assurance posture

This repository is pre-release research software. The initial kernel does not claim production authority, economic correctness, cryptographic implementation, shell refinement, or audit completion.

## License

Dual-licensed under Apache-2.0 or MIT, at your option.
