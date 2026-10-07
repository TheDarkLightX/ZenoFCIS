# Known answers of the transform checker

Each directory holds the known answers of one checker semantics version,
named by the version's last component: `1/` is `zeno-fcis/transform-check/1`,
the version every receipt's `checker.semantics` binds. `vectors.json` lists the
pairs. Each pair is two canonical programs (`program-N.zcve`, written from
`program-N.json` by `zeno-fcis loop encode`) with a Step limit and a tuple cap.
For each pair, `NAME.expected.json` is the complete `zeno-fcis transform check`
report, and `NAME.receipt.json` holds the exact receipt bytes when the pair is
equivalent.

`known_answers_pin_the_checker_semantics` in `tests/transform_cli.rs` runs every
pair through the binary and requires these exact answers. The pairs cover:
equal usage, different usage, identical failures, a counterexample, a binding
Step limit, a domain over the cap, and a refused ABI.

Never edit an existing version's answers. A change to the checker that alters
any answer needs a new `CHECKER` version in `src/transform.rs` and a new
directory with that version's answers. Applications then rebind their receipts
with `zeno-fcis contract refresh-receipts`.
