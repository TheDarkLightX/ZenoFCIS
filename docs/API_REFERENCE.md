# API reference

## Hosted reference

After publication, rustdoc for the umbrella crate is available at:

```text
https://docs.rs/zeno-fcis/1.0.0-rc.1/zeno_fcis/
```

Every public subcrate is published at the same exact version and receives its
own docs.rs reference.

## Local reference

```bash
RUSTDOCFLAGS='-D warnings' cargo +1.97.1 doc \
  --workspace --all-features --locked --no-deps --open
```

The RC bundle also contains a static rustdoc archive generated from the exact
release commit.

## Recommended entry points

| Goal | Entry point |
|---|---|
| Basic decision and budget algebra | `zeno_fcis::core` |
| Canonical admitted values | `zeno_fcis::value`, `zeno_fcis::codec` |
| Project schema and policy | `zeno_fcis::project`, `schema`, `catalog` |
| Pure transition construction | `zeno_fcis::transition` |
| Project invariants and conservation | `zeno_fcis::laws` |
| Nominal commit authorization | `zeno_fcis::authority` |
| Fixed domain machines | `zeno_fcis::domain` |
| Global composed program | `zeno_fcis::composed_program` |
| Composition proof obligations | `zeno_fcis::compose` |
| Formal tool protocol | `zeno_fcis::backend`, `evidence`, `refine` |
| Reference and concrete shells | `zeno_fcis::shell`, `zeno-fcis-shell-sqlite` |

Prefer the [quickstart](QUICKSTART.md) for the first implementation, then use
the [crate map](CRATE_MAP.md) and generated rustdoc for exact signatures.

## Stability

`1.0.0-rc.1` freezes a candidate Rust API for review. Corrections may change
that API in a later release candidate. Stable protocol identifiers remain
independent of Cargo versions and may not be silently reinterpreted.
