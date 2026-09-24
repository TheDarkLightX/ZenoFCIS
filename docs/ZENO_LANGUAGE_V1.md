# `.zeno` language version 1

This is the implemented language contract for ZenoFCIS `1.1.0`.
`ZENO_DSL_VERSION`, `PROJECT_SPEC_FORMAT_VERSION`, and
`TEMPORAL_SPEC_FORMAT_VERSION` are all `1`.

## Lexical contract

A project is one UTF-8 file with ASCII identifiers, unsigned decimal stable
IDs, braces, semicolons, and `//` comments. Identifiers begin with an ASCII
letter or underscore and continue with ASCII letters, digits, underscores, or
ASCII hyphens. Commitments accepted by tool manifests are lowercase hexadecimal.

The language has no import, include, macro, interpolation, filesystem path,
embedded command, implicit stable-ID allocation, or recursion construct.
Shell syntax and traversal text are either invalid tokens or ordinary inert
identifier data.

## Resource limits

One source file is limited to 1 MiB and 262,144 tokens. Parsing retains at most
256 diagnostics and stops formula nesting at depth 256 before recursive descent.
Elaboration accepts finite temporal horizons from 1 through 256. Existing
composition limits still apply after the typed project has been built.

These are admission limits. Exceeding one returns a stable resource diagnostic
and never turns a partial project into an accepted `ProjectSpec`.

## Declaration forms

```text
zeno 1;
project ID name;
namespace ID name;
type ID KIND name;
type ID int name in MIN..=MAX;
field ID OWNER_TYPE name FIELD_TYPE;
variant ID OWNER_TYPE name PAYLOAD_TYPE|none;
reason ID name precedence RANK;
effect ID name destination TYPE payload TYPE;
channel ID name destination TYPE payload TYPE;
component ID name { COMPONENT_ITEMS }
wire COMPONENT.PORT -> COMPONENT.PORT;
merge [COMPONENT, ...];
law ID name = RELATIONAL_FORMULA;
claim ID name BACKEND MODE = FORMULA;
claim ID name BACKEND inductive [assume [LAW, ...]] [accept [LAW, ...]] [failure [LAW, ...]] = STATE_FORMULA;
```

Type kinds are `state`, `command`, `context`, `effect`, `destination`,
`payload`, `data`, `bool`, and `int`.

An `int` type may declare its inclusive range, for example
`type 105 int Attempts in 0..=2;`:
- each bound is an optional `-` and an unsigned 64-bit decimal integer;
- `..=` is required, because quantifiers read `..` as half-open;
- the least value comes first.

The range is part of the project's meaning and of its canonical bytes. A type
without a range encodes exactly as before. Schema lowering takes a declared
range as the type's `I128` bounds, and refuses an application binding that
states other bounds. Inductive steps assume every declared range (see
[inductive claims](INDUCTIVE_CLAIMS.md)). An `int` without a range still gets
its bounds from the application's schema binding, which `project.zeno` does
not see.

Backends are `cvc5`, `z3`, `lean`, and
`all`. Claim modes are `relational`, `finite N`, `unbounded`, and `inductive`.
An inductive claim states an invariant over `pre.` state paths, and names the
laws its induction step may assume. `assume` lists laws enforced on every
committing decision, `accept` laws enforced on accepts, and `failure` laws
enforced on committed failures. See [inductive claims](INDUCTIVE_CLAIMS.md).

Component items are:

```text
owns TYPE;
port ID input|output name PAYLOAD_TYPE;
reads PROJECTION;
writes PROJECTION;
contexts PROJECTION;
effects PROJECTION;
budget steps|nodes|quantifier_iterations|predicate_calls LIMIT;
assume LAW_ID;
guarantee LAW_ID;
```

Projection roots are `pre`, `post`, `command`, `context`, `effects`, `outbox`,
and `events`, followed by explicit stable-ID segments.

## Ordering and identity

All order-insensitive declaration collections are sorted by stable ID during
elaboration. Reason precedence must be total. The `merge` list is explicit
semantic order and must contain every component exactly once. Port wiring is
type checked. Duplicate IDs or names, unknown references, invalid precedence,
incomplete merge order, incompatible backend/mode combinations, and limit
exhaustion are accumulated when recovery is safe.

`ProjectSpec::canonical_bytes` excludes source spans and presentation. Two
equivalent files, a `ProjectSpecBuilder`, and a `CompositionAstBuilder` lower
through the same validation path.

## Relational formulas

Boolean syntax is `!`, `not`, `&&`, `||`, and `->`. Comparisons are `==`,
`!=`, `<`, `<=`, `>`, and `>=`. Scalar syntax includes checked `+`, `-`, `*`,
`div_exact(a,b)`, `div_floor(a,b)`, `div_ceil(a,b)`, bounded
`sum x in START..END { VALUE }`, and projections.

Bounded predicates use `forall x in START..END { FORMULA }` and
`exists x in START..END { FORMULA }`. A call such as `host_rule(a, b)` is a
typed named predicate supplied by the host. A missing predicate, overflow,
division error, or exhausted evaluation limit is `Indeterminate`.

## Diagnostics

Every diagnostic carries a stable code, stage, AST path, half-open UTF-8 byte
span, expected value, actual value, and remediation. Sets sort by source span,
then AST path, then code, and report whether retention was truncated.

The closed codes are `ZENO-E0001` through `ZENO-E0004` for source/lexing,
`ZENO-E0101` through `ZENO-E0105` for parsing, `ZENO-E0201` through
`ZENO-E0207` for elaboration, and `ZENO-E0301` through `ZENO-E0302` for
trace/evaluation failure.

The accumulated-diagnostic UX reports independent actionable failures in one
run. Recovery never invents semantic acceptance; any retained error makes
parsing or elaboration fail.
