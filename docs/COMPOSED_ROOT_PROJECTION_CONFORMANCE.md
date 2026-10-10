# Composed Root Projection Conformance v1

## Purpose

`ComposedDomainProgram` must not project a machine state cell to an aggregate-root
path broader than, narrower than, or unrelated to the exact path declared by the
machine interface. Construction now rejects every such mismatch before a program
can execute.

## Inputs and outputs

For every fixed machine row and state slot, construction receives:

- the exact wildcard-free `MachineInterface` state `AccessPath`;
- the corresponding aggregate-root `ValuePath` from `ProjectionPlan`;
- the catalog root-state namespace and approved commitment provider needed to
  convert the root path into its canonical `AccessPath`.

Successful construction returns the ordinary `ComposedDomainProgram`. Failure
returns `StateProjectionPathMismatch { machine, slot }` and produces no program,
transition, candidate, effect, or outbox obligation. A path deeper than the
composition bound fails earlier with `ProjectionPathTooDeep`.

## Authority boundary

The check is mandatory inside `ComposedDomainProgram::try_new`. A caller cannot
replace it with a predicate hash or opt out after the semantic program identity
has been derived. The check grants no publication authority by itself; production
publication still requires the catalog and law-aware authorization path.

## Construction law

For every machine `m` and state slot `s`:

```text
canonical_root_access_path(projection.state_paths[m][s])
    == executable.interfaces[m].state[s].path
```

`ExecutableComposition` construction separately requires the component's declared
read footprint to cover every exact interface state path. During execution, a
changed state cell is rejected unless the component's declared write footprint
covers that same interface path. Together, the checks establish the following
bounded runtime implication for a projected changed cell:

```text
projected write path
    == exact interface state path
    is covered by the component's declared write footprint
```

Exact equality is intentional. Ancestor, descendant, sibling, namespace, and
wildcard substitutions are not implicit local-to-global mappings. A future abstract
namespace mapping would need its own closed canonical value and authorization law.

## Deterministic resource bounds

Construction performs at most `MACHINES * STATE_SLOTS` path conversions and
comparisons because it exits on the first mismatch. `ProjectionPlan::try_new`
rejects every state, command, or context path deeper than `MAX_PATH_ATOMS` before
conversion; every interface path is bounded by `AccessPath`. V1 passes zero
map-key bytes because direct projections reject map-key segments before this
check.

## Negative cases

The permanent test surface covers:

- interface root with descendant projection;
- descendant interface with root projection;
- sibling interface and projection paths;
- a state namespace different from the catalog root-state namespace;
- one path at the exact depth bound and one path one atom over it;
- exact matching root and non-root paths as positive controls.

Wildcard interface bindings are rejected by `MachineInterface` before this check.

## Trusted dependencies and assumptions

- `canonical_access_path` is the normative conversion from a direct root
  `ValuePath` to a composition `AccessPath`.
- The catalog root-state type identifier is the aggregate-root namespace.
- `ExecutableComposition` has checked that every state slot is readable and
  rejects a changed slot at runtime unless its declared write footprint covers
  the exact interface path.
- The configured approved commitment provider is trusted for any future admitted
  map-key conversion. Map keys remain excluded from V1.

## Explicit nonclaims

- This check does not prove that a declared component footprint is a complete
  all-input over-approximation. Production parallel authorization additionally
  requires an accepted `CompleteFootprintWitness` for every component.
- It does not prove commutativity, parallel parity, project business laws, value
  conservation, machine implementation correctness, or deployment identity.
- It does not add a concurrent scheduler or threaded shell.
- It does not authorize a local namespace to represent an aggregate-root path.
- It is not a mechanized end-to-end correctness proof.
