# Law path resolution

## Purpose

A law or claim reads observations through projection paths of stable IDs, such
as `post.100.110`. Elaboration does not check that these IDs name declared
types and fields. A mistyped segment therefore elaborates without a diagnostic,
and the law engine later finds no observation for it.

This package reports such paths when a project is checked.

## Inputs and outputs

`zeno_fcis_spec::resolve_path(spec, path)` returns one `PathResolution`:

| Code | Meaning |
| --- | --- |
| `resolved` | The first segment is a declared type of the kind the root reads, and each later segment is a field of the type before it. |
| `unchecked` | The root is `effects`, `outbox`, or `events`. These roots have no declared segment rules. |
| `unknown-root-type` | The first segment is not a declared type of the kind the root reads. `pre` and `post` read state types, `command` reads a command type, and `context` reads a context type. |
| `unknown-field` | A later segment is not a field of the type before it. |

`law_paths` and `claim_paths` return every path a law or claim reads, in syntax
order. That includes paths inside quantifiers, predicate arguments, and
temporal operators.

`zeno-fcis check` reports every unresolved path:

- Human output prints one `warning:` line on stderr per unresolved path. The
  line names the law or claim, the path, and the missing type or field. The
  summary line on stdout is unchanged.
- JSON output adds an `unresolved_paths` object with `laws` and `claims` arrays
  of `{code, id, name, path}` entries.
- `--require-resolved-paths` exits with code 1 when any path is unresolved. JSON
  output then reports `status: "unresolved-paths"`, including when
  `--require-substantive` also fails.

## Authority boundary

Resolution is diagnostic. It creates no evidence and grants no authority. It
changes no canonical project bytes, semantic program hash, or elaboration
result. Only `--require-resolved-paths` changes an exit code, and only when a
caller asks for it.

## Trusted dependencies

Resolution uses only the typed `.zeno` AST in `zeno-fcis-spec`. It runs in
`no_std + alloc` without clocks, randomness, I/O, or solvers.

## Deterministic resource bounds

A path has at most 64 segments, and resolving a segment scans the declared
fields once. Collecting paths is one traversal of each formula, which
elaboration already bounds.

## Laws

1. A path resolves only when its first segment is a declared type of the kind
   its root reads, and each later segment is a field whose owner is the type
   named by the segment before it.
2. After the first segment, each step follows the declared type of the field
   just matched.
3. A path under `effects`, `outbox`, or `events` is never reported as resolved.

## Negative cases

The tests keep these outcomes:

- `post.100.999`, where type 100 declares no field 999, is `unknown-field`.
- `pre.101`, where 101 is a command type, is `unknown-root-type`.
- `pre.100.110.5`, where field 110 has an `int` type, is `unknown-field` in
  that `int` type.
- Every shipped example, template, and the zUSD lane attempt resolves with no
  warning.
- A project with a mistyped path passes `check` by default and fails with
  `--require-resolved-paths`.

## Assumptions

- The project's law engine binds each resolved path to the value the path
  names. Resolution checks the names, not the binding.

## Explicit nonclaims

- A resolved path is not necessarily supplied at evaluation. The law engine
  chooses which paths it observes, and a missing observation is still
  indeterminate.
- Resolution does not check that a path's value has the type a comparison
  expects.
- Component footprint projections (`reads`, `writes`, `contexts`, and
  `effects`) are not checked.
- Elaboration still accepts unresolved paths. Rejecting them is recorded in the
  [V2 ledger](adr/0004-v2-ledger.md).
