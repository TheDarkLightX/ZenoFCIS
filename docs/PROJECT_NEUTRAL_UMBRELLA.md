# Project-Neutral Umbrella Boundary

## Scope

The `zeno-fcis` umbrella no longer activates or exports the ZenoDEX profile by
default. Its default `std` surface and its minimal `no_std` surface contain only
project-neutral primitives. Consumers opt into ZenoDEX types with
`zenodex-profile`; `mounted-zenodex` activates that feature automatically.

The ZenoDEX profile crate remains a workspace member and its protocol meaning is
unchanged.

## Inputs and outputs

The input is a Cargo feature selection. The output is one of these dependency
and public-API surfaces:

- default or `--no-default-features`: project-neutral umbrella only;
- `zenodex-profile`: project-neutral umbrella plus ZenoDEX profile exports;
- `mounted-zenodex`: ZenoDEX profile exports plus the concrete mounted adapter;
- `full` or `--all-features`: every reviewed optional surface, including
  ZenoDEX.

## Authority boundary

Project-specific identifiers, constants, reasons, and profile constructors are
not part of the reusable umbrella unless the consumer explicitly selects the
project feature. Feature selection changes Rust dependency and export surfaces;
it does not change any protocol value.

## Trusted dependencies

No dependency is added. `zeno-fcis-profile-zenodex` changes from unconditional
to optional in the umbrella manifest. The profile crate and its own dependency
graph are unchanged.

## Deterministic resource bounds

No semantic resource bound changes. The default umbrella dependency closure is
strictly smaller. Existing ZenoDEX profile limits apply only when that feature
is selected.

## Laws and negative cases

- Default and minimal umbrella dependency trees exclude
  `zeno-fcis-profile-zenodex`.
- Default and minimal umbrella code cannot access ZenoDEX re-exports.
- `zenodex-profile` admits the profile in `no_std + alloc` builds.
- `mounted-zenodex` implies `zenodex-profile` and continues to compile.
- `full` and `--all-features` retain the existing complete workspace surface.

## Assumptions

- Downstream users that relied on default ZenoDEX re-exports will enable
  `zenodex-profile` explicitly.
- Cargo's feature resolver enforces optional dependency activation as described
  by the checked manifest and lockfile.

## Explicit nonclaims

- This does not alter or generalize the ZenoDEX protocol profile itself.
- This does not make experimental adapters part of Core V1.
- The current [feature matrix](FEATURE_MATRIX.md) classifies the implemented
  surfaces; it does not freeze a final 1.0 feature-stability policy.
- This makes no production-readiness or compatibility claim for project-specific
  integrations.
