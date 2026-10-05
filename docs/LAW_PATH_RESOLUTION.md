# Mandatory law and claim path resolution

V2 elaboration rejects a law or claim path whose type or field IDs do not
resolve. This applies to `elaborate_project`, `ProjectSpecBuilder::finish`, and
the normal CLI project loader. `zeno-fcis check` exits 1 and reports normal
`UnknownReference` (`ZENO-E0203`) diagnostics for those projects. JSON output has
`status: "invalid"`. The compatibility option `--require-resolved-paths` no
longer controls whether the project is admitted.

A path such as `post.100.110` starts at declared state type 100 and follows
field 110. `pre` and `post` require a state root type, `command` a command root,
and `context` a context root. Each subsequent field must belong to the current
type; the next step follows that field's declared type. The check visits every
law and claim formula, including predicates, quantifiers, and temporal bodies.
A wrong root kind also fails. Stable law/claim order and the existing formula,
quantifier, and path bounds are preserved.

`resolve_path` returns `Resolved`, `UnknownRootType`, or `UnknownField`.
For `effects` and `outbox`, the first segment must name a destination or payload
type linked by a declared effect or channel in that lane. Later segments follow
declared fields, including nested fields. Thus `outbox.104.110` resolves only
when type 104 is a declared channel destination or payload and field 110 belongs
to it. Channel IDs in component footprints, such as `outbox.300`, use a separate
grammar and are unchanged. The DSL declares no event roots or numeric query
operators: `events` formula paths refuse instead of silently accepting arbitrary
IDs. The V2 interpreter's separate typed delivery-observation enum is unchanged.

Accepted projects keep exactly the same canonical bytes. Previously accepted
projects with unresolved formula paths must correct their declarations or IDs;
removing laws is not a migration. Resolution uses only the immutable typed AST
in `no_std + alloc`. It does not evaluate a law, check comparison operand types,
prove a declared statement, or guarantee that a runtime observation is present.
Runtime missing-observation refusal remains required.

Native controls cover undeclared root types, wrong root kinds, missing fields,
traversal past a scalar, quantified and temporal claim bodies, the builder API,
delivery payload types and fields, undeclared event roots, and normal CLI
loading with and without the compatibility flag. Shipped valid examples and
scoped laws are regression inputs.
